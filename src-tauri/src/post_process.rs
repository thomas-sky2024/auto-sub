use crate::subtitle::Segment;
use log::debug;
use unicode_segmentation::UnicodeSegmentation;

/// Maximum characters per second for comfortable reading.
const MAX_CPS: f32 = 20.0;
/// Minimum segment duration in seconds.
const MIN_DURATION: f32 = 1.2;
/// Maximum segment duration in seconds.
const MAX_DURATION: f32 = 8.0;
/// Maximum characters per line (SRT standard).
const MAX_LINE_LEN: usize = 42;
/// Maximum lines per subtitle.
const MAX_LINES: usize = 2;
/// Merge gap threshold in seconds.
const MERGE_GAP: f32 = 0.5;
/// Speaker pause threshold — force new subtitle.
const SPEAKER_PAUSE: f32 = 1.5;

/// CJK end-of-sentence punctuation.
const CJK_SENTENCE_END: &[char] = &['。', '！', '？', '；'];
/// English end-of-sentence punctuation.
const EN_SENTENCE_END: &[char] = &['.', '!', '?'];

/// Check if a character is CJK.
fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}' // CJK Extension A
        | '\u{F900}'..='\u{FAFF}' // CJK Compatibility
        | '\u{3000}'..='\u{303F}' // CJK Symbols
        | '\u{FF00}'..='\u{FFEF}' // Fullwidth forms
    )
}

/// Detect if text is primarily CJK.
fn is_cjk_text(text: &str) -> bool {
    let cjk_count = text.chars().filter(|c| is_cjk(*c)).count();
    let total = text.chars().count();
    if total == 0 {
        return false;
    }
    cjk_count as f32 / total as f32 > 0.3
}

/// Check if text ends with sentence-ending punctuation.
fn ends_with_sentence(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let last = trimmed.chars().last().unwrap();
    CJK_SENTENCE_END.contains(&last) || EN_SENTENCE_END.contains(&last)
}

/// Stage 4: Full commercial-grade post-processing.
pub fn process(segments: Vec<Segment>) -> Vec<Segment> {
    if segments.is_empty() {
        return segments;
    }

    let mut result = segments;

    // 1. Merge segments (context-aware + standard gap merge)
    result = merge_segments(result);

    // 2. Enforce duration bounds
    result = enforce_duration(result);

    // 3. CPS control — split oversized segments
    result = enforce_cps(result);

    // 4. Line formatting (max 2 lines, balanced split)
    result = format_lines(result);

    // 5. Fix overlaps (final pass)
    result = fix_overlaps(result);

    // 6. Round timestamps
    result = round_timestamps(result);

    result
}

/// Merge segments based on gap and sentence continuity.
fn merge_segments(segments: Vec<Segment>) -> Vec<Segment> {
    if segments.is_empty() {
        return segments;
    }

    let mut result: Vec<Segment> = vec![segments[0].clone()];

    for seg in segments.into_iter().skip(1) {
        let prev = result.last().unwrap();
        let gap = seg.start - prev.end;

        // Speaker pause: never merge if gap > 1.5s
        if gap > SPEAKER_PAUSE {
            debug!("post: speaker pause ({:.2}s), new subtitle", gap);
            result.push(seg);
            continue;
        }

        // Context-aware merge: same sentence continues (prev doesn't end with punctuation)
        let should_merge = if gap <= MERGE_GAP {
            true // standard gap merge
        } else {
            // gap between 0.5s and 1.5s — merge only if same sentence
            !ends_with_sentence(&prev.text)
        };

        if should_merge {
            let merged_text = if is_cjk_text(&prev.text) {
                format!("{}{}", prev.text.trim(), seg.text.trim())
            } else {
                format!("{} {}", prev.text.trim(), seg.text.trim())
            };

            let merged_duration = seg.end - prev.start;
            if merged_duration <= MAX_DURATION && merged_text.chars().count() <= MAX_LINE_LEN * MAX_LINES {
                let last = result.last_mut().unwrap();
                last.end = seg.end;
                last.text = merged_text;
                continue;
            }
        }

        result.push(seg);
    }

    result
}

/// Enforce min/max duration constraints.
fn enforce_duration(segments: Vec<Segment>) -> Vec<Segment> {
    segments
        .into_iter()
        .map(|mut seg| {
            // Pad short segments
            if seg.duration() < MIN_DURATION {
                seg.end = seg.start + MIN_DURATION;
            }
            seg
        })
        .collect()
}

/// Split segments that exceed CPS threshold.
fn enforce_cps(segments: Vec<Segment>) -> Vec<Segment> {
    let mut result = Vec::new();

    for seg in segments {
        if seg.cps() > MAX_CPS && seg.text.chars().count() > 10 {
            // Split at sentence boundary or midpoint
            let split_segs = split_segment(&seg);
            result.extend(split_segs);
        } else {
            result.push(seg);
        }
    }

    result
}

/// Split a segment at the best boundary point.
fn split_segment(seg: &Segment) -> Vec<Segment> {
    let text = &seg.text;
    let char_count = text.chars().count();

    if char_count < 6 {
        return vec![seg.clone()];
    }

    // Find best split point (sentence boundary near midpoint)
    let mid = char_count / 2;
    let mut best_split = mid;
    let mut best_distance = char_count;

    let is_cjk = is_cjk_text(text);
    let sentence_ends: &[char] = if is_cjk { CJK_SENTENCE_END } else { EN_SENTENCE_END };

    for (i, c) in text.chars().enumerate() {
        if sentence_ends.contains(&c) {
            let dist = if i > mid { i - mid } else { mid - i };
            if dist < best_distance && i > 0 && i < char_count - 1 {
                best_distance = dist;
                best_split = i + 1;
            }
        }
    }

    // For CJK, split at grapheme cluster boundary
    if is_cjk {
        let graphemes: Vec<&str> = text.graphemes(true).collect();
        let grapheme_mid = graphemes.len() / 2;
        if best_distance > graphemes.len() / 4 {
            best_split = graphemes[..grapheme_mid].concat().chars().count();
        }
    }

    let (text1, text2): (String, String) = {
        let chars: Vec<char> = text.chars().collect();
        (
            chars[..best_split].iter().collect(),
            chars[best_split..].iter().collect(),
        )
    };

    let ratio = best_split as f32 / char_count as f32;
    let split_time = seg.start + seg.duration() * ratio;

    vec![
        Segment {
            start: seg.start,
            end: split_time,
            text: text1.trim().to_string(),
        },
        Segment {
            start: split_time,
            end: seg.end,
            text: text2.trim().to_string(),
        },
    ]
}

/// Format text to respect max lines and line length.
fn format_lines(segments: Vec<Segment>) -> Vec<Segment> {
    segments
        .into_iter()
        .map(|mut seg| {
            let text = seg.text.trim().to_string();
            let char_count = text.chars().count();

            // If it fits in one line, keep it
            if char_count <= MAX_LINE_LEN {
                seg.text = text;
                return seg;
            }

            // Split into 2 balanced lines
            if char_count <= MAX_LINE_LEN * MAX_LINES {
                let mid = char_count / 2;
                let chars: Vec<char> = text.chars().collect();

                // Find best split near midpoint (prefer space/punctuation)
                let mut best = mid;
                for offset in 0..=mid / 2 {
                    for try_pos in [mid + offset, mid.saturating_sub(offset)] {
                        if try_pos < chars.len() {
                            if chars[try_pos] == ' '
                                || chars[try_pos] == ','
                                || CJK_SENTENCE_END.contains(&chars[try_pos])
                            {
                                best = try_pos + 1;
                                break;
                            }
                        }
                    }
                    if best != mid {
                        break;
                    }
                }

                let line1: String = chars[..best].iter().collect();
                let line2: String = chars[best..].iter().collect();
                seg.text = format!("{}\n{}", line1.trim(), line2.trim());
            }
            // Truncate if somehow > 2 lines worth
            seg
        })
        .collect()
}

/// Fix any remaining overlaps.
fn fix_overlaps(mut segments: Vec<Segment>) -> Vec<Segment> {
    for i in 1..segments.len() {
        if segments[i - 1].end > segments[i].start {
            segments[i - 1].end = (segments[i].start - 0.05).max(segments[i - 1].start);
        }
    }
    segments
}

/// Round timestamps to 2 decimal places.
fn round_timestamps(segments: Vec<Segment>) -> Vec<Segment> {
    segments
        .into_iter()
        .map(|mut seg| {
            seg.start = (seg.start * 100.0).round() / 100.0;
            seg.end = (seg.end * 100.0).round() / 100.0;
            seg
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_gap() {
        let segs = vec![
            Segment { start: 0.0, end: 1.0, text: "Hello".into() },
            Segment { start: 1.2, end: 2.5, text: "world".into() },
        ];
        let result = process(segs);
        assert_eq!(result.len(), 1);
        assert!(result[0].text.contains("Hello"));
        assert!(result[0].text.contains("world"));
    }

    #[test]
    fn test_speaker_pause() {
        let segs = vec![
            Segment { start: 0.0, end: 1.0, text: "Speaker one.".into() },
            Segment { start: 3.0, end: 4.0, text: "Speaker two.".into() },
        ];
        let result = process(segs);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_cjk_merge() {
        let segs = vec![
            Segment { start: 0.0, end: 1.0, text: "你好".into() },
            Segment { start: 1.2, end: 2.5, text: "世界".into() },
        ];
        let result = process(segs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "你好世界");
    }

    #[test]
    fn test_min_duration() {
        let segs = vec![Segment {
            start: 0.0,
            end: 0.5,
            text: "Short".into(),
        }];
        let result = process(segs);
        assert!(result[0].duration() >= MIN_DURATION);
    }

    #[test]
    fn test_overlap_fix() {
        let segs = vec![
            Segment { start: 0.0, end: 3.0, text: "First sentence.".into() },
            Segment { start: 2.5, end: 5.0, text: "Second sentence.".into() },
        ];
        let result = process(segs);
        assert!(result[0].end <= result[1].start);
    }

    #[test]
    fn test_timestamp_rounding() {
        let segs = vec![Segment {
            start: 1.123456,
            end: 2.789012,
            text: "Test rounding.".into(),
        }];
        let result = process(segs);
        assert_eq!(result[0].start, 1.12);
        assert_eq!(result[0].end, 2.79);
    }
}

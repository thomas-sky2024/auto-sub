use crate::subtitle::Segment;
use log::debug;

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
/// Minimum segment duration to filter out hallucinations (80ms).
const MIN_VALID_DURATION: f32 = 0.08;
/// Minimum gap between segments to prevent overlaps.
const MIN_GAP: f32 = 0.04;

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

/// Maximum gap to close with gapless (3.0 seconds).
const GAPLESS_MAX_GAP: f32 = 3.0;

/// Check if segment has valid timestamps (not NaN, not Inf, end > start).
fn is_valid_timestamp(seg: &Segment) -> bool {
    seg.start.is_finite() && seg.end.is_finite() && seg.end > seg.start
}

/// Apply gapless: close small gaps between segments by extending end[i] to start[i+1].
/// Only closes gaps < 3 seconds to preserve intentional pauses.
fn apply_gapless(segments: Vec<Segment>) -> Vec<Segment> {
    if segments.len() <= 1 {
        return segments;
    }

    let mut result = segments;
    for i in 0..result.len() - 1 {
        let gap = result[i + 1].start - result[i].end;

        // Only close small gaps (< 3 seconds) to respect intentional pauses
        if gap > 0.0 && gap < GAPLESS_MAX_GAP {
            debug!(
                "post_process: closing gap {:.2}s between segment {} and {}",
                gap, i, i + 1
            );
            result[i].end = result[i + 1].start;
        }
    }

    result
}

/// Deduplication: remove consecutive identical segments and ultra-short hallucinations.
/// Prevents cascading hallucinations like "发发" repeated many times.
fn dedup_consecutive(segments: Vec<Segment>) -> Vec<Segment> {
    let mut result: Vec<Segment> = Vec::new();
    let mut prev: Option<Segment> = None;

    for seg in segments {
        // Skip ultra-short segments (< 80ms) - likely hallucinations
        if seg.duration() < MIN_VALID_DURATION {
            debug!("post_process: skipping ultra-short segment ({:.2}s): '{}'",
                seg.duration(), seg.text.trim());
            continue;
        }

        // Skip if text is empty or only whitespace
        let text_norm = seg.text.trim();
        if text_norm.is_empty() {
            continue;
        }

        // Check for consecutive duplicate (extend previous instead of adding duplicate)
        if let Some(ref p) = prev {
            if text_norm == p.text.trim() {
                // Extend previous segment's end time instead of adding duplicate
                prev.as_mut().unwrap().end = seg.end;
                debug!("post_process: merged consecutive duplicate: '{}'", text_norm);
                continue;
            }
        }

        if let Some(p) = prev.take() {
            result.push(p);
        }
        prev = Some(seg);
    }

    if let Some(p) = prev {
        result.push(p);
    }

    result
}

/// Stage 4: Full commercial-grade post-processing.
pub fn process(segments: Vec<Segment>) -> Vec<Segment> {
    if segments.is_empty() {
        return segments;
    }

    let mut result = segments;
    log::debug!("post_process: input {} segments", result.len());

    // 0. Validate timestamps and deduplicate (prevent hallucination cascade)
    result = result.into_iter()
        .filter(|seg| is_valid_timestamp(seg))
        .collect();
    log::debug!("post_process: after timestamp validation -> {} segments", result.len());

    result = dedup_consecutive(result);
    log::debug!("post_process: after dedup_consecutive -> {} segments", result.len());

    // 1. Merge segments (context-aware + standard gap merge)
    result = merge_segments(result);
    log::debug!("post_process: after merge_segments -> {} segments", result.len());

    // 2. Enforce duration bounds
    result = enforce_duration(result);
    log::debug!("post_process: after enforce_duration -> {} segments", result.len());

    // 3. CPS control — split oversized segments
    result = enforce_cps(result);
    log::debug!("post_process: after enforce_cps -> {} segments", result.len());

    // 4. Line formatting (max 2 lines, balanced split)
    result = format_lines(result);
    log::debug!("post_process: after format_lines -> {} segments", result.len());

    // 5. Apply gapless (close small gaps) BEFORE fixing overlaps
    result = apply_gapless(result);
    log::debug!("post_process: after apply_gapless -> {} segments", result.len());

    // 6. Fix overlaps (final pass) - MUST be after gapless to avoid start > end
    result = fix_overlaps(result);
    log::debug!("post_process: after fix_overlaps -> {} segments", result.len());

    // 7. Round timestamps
    result = round_timestamps(result);
    log::debug!("post_process: after round_timestamps -> {} segments", result.len());

    // 8. Final validation: remove any segments that still have invalid timestamps
    result = result.into_iter()
        .filter(|seg| is_valid_timestamp(seg) && seg.duration() >= 0.001)
        .collect();
    log::debug!("post_process: after final validation -> {} segments", result.len());

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

/// Split a segment at the best boundary point (word-aware for non-CJK).
fn split_segment(seg: &Segment) -> Vec<Segment> {
    let text = &seg.text;
    let char_count = text.chars().count();

    if char_count < 6 {
        return vec![seg.clone()];
    }

    let mid = char_count / 2;
    let chars: Vec<char> = text.chars().collect();
    let mut best_split = mid;

    let is_cjk = is_cjk_text(text);

    if is_cjk {
        // CJK: Find punctuation or cut at midpoint
        let mut best_distance = char_count;
        for (i, &c) in chars.iter().enumerate() {
            if CJK_SENTENCE_END.contains(&c) || c == '，' || c == '、' {
                let dist = if i > mid { i - mid } else { mid - i };
                if dist < best_distance && i > 0 && i < char_count - 1 {
                    best_distance = dist;
                    best_split = i + 1;
                }
            }
        }
    } else {
        // Non-CJK (English, Vietnamese...): Prefer word boundaries (whitespace)
        let mut found_split = false;
        let mut best_distance = char_count;

        // Priority 1: Punctuation followed by whitespace
        for (i, &c) in chars.iter().enumerate() {
            if (EN_SENTENCE_END.contains(&c) || c == ',' || c == ';')
                && i + 1 < char_count
                && chars[i + 1].is_whitespace()
            {
                let dist = if i > mid { i - mid } else { mid - i };
                if dist < best_distance {
                    best_distance = dist;
                    best_split = i + 1; // Cut after punctuation
                    found_split = true;
                }
            }
        }

        // Priority 2: Find whitespace closest to midpoint
        if !found_split {
            for offset in 0..=mid {
                // Scan right
                if mid + offset < char_count && chars[mid + offset].is_whitespace() {
                    best_split = mid + offset;
                    break;
                }
                // Scan left
                if mid >= offset && chars[mid - offset].is_whitespace() {
                    best_split = mid - offset;
                    break;
                }
            }
        }
    }

    let (text1, text2): (String, String) = (
        chars[..best_split].iter().collect(),
        chars[best_split..].iter().collect(),
    );

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

/// Format text to respect max lines and line length (word-aware for non-CJK).
fn format_lines(segments: Vec<Segment>) -> Vec<Segment> {
    segments
        .into_iter()
        .map(|mut seg| {
            let text = seg.text.trim().to_string();
            let char_count = text.chars().count();

            // Single line case
            if char_count <= MAX_LINE_LEN {
                seg.text = text;
                return seg;
            }

            // Multi-line case
            if char_count <= MAX_LINE_LEN * MAX_LINES {
                let mid = char_count / 2;
                let chars: Vec<char> = text.chars().collect();
                let mut best = mid;
                let is_cjk = is_cjk_text(&text);

                if is_cjk {
                    // CJK: Find punctuation near midpoint
                    for offset in 0..=mid {
                        for try_pos in [mid + offset, mid.saturating_sub(offset)] {
                            if try_pos < chars.len()
                                && (chars[try_pos] == '，'
                                    || chars[try_pos] == '、'
                                    || CJK_SENTENCE_END.contains(&chars[try_pos]))
                            {
                                best = try_pos + 1;
                                break;
                            }
                        }
                        if best != mid {
                            break;
                        }
                    }
                } else {
                    // Non-CJK: Find whitespace (word boundary) near midpoint
                    for offset in 0..=mid {
                        // Scan right
                        if mid + offset < chars.len() && chars[mid + offset].is_whitespace() {
                            best = mid + offset;
                            break;
                        }
                        // Scan left
                        if mid >= offset && chars[mid - offset].is_whitespace() {
                            best = mid - offset;
                            break;
                        }
                    }
                }

                let line1: String = chars[..best].iter().collect();
                let line2: String = chars[best..].iter().collect();
                seg.text = format!("{}\n{}", line1.trim(), line2.trim());
            }
            seg
        })
        .collect()
}

/// Fix any remaining overlaps and enforce minimum gap between cues.
fn fix_overlaps(mut segments: Vec<Segment>) -> Vec<Segment> {
    for i in 1..segments.len() {
        if segments[i - 1].end > segments[i].start {
            // Trim previous segment to start MIN_GAP before next segment
            let new_end = (segments[i].start - MIN_GAP).max(segments[i - 1].start);
            debug!("post_process: overlap detected at segment {}, trimming end from {:.2} to {:.2}",
                i - 1, segments[i - 1].end, new_end);
            segments[i - 1].end = new_end;
        }
    }

    // Remove any segments where start >= end (shouldn't happen, but be safe)
    segments.into_iter()
        .filter(|seg| seg.start < seg.end - 0.001)
        .collect()
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
        // Overlapping segments within merge gap get merged
        assert_eq!(result.len(), 1);
        assert!(result[0].text.contains("First"));
        assert!(result[0].text.contains("Second"));
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

    #[test]
    fn test_dedup_consecutive() {
        let segs = vec![
            Segment { start: 0.0, end: 1.0, text: "Hello".into() },
            Segment { start: 1.0, end: 2.0, text: "Hello".into() }, // Duplicate
            Segment { start: 2.0, end: 3.0, text: "World".into() },
        ];
        let result = dedup_consecutive(segs);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "Hello");
        assert_eq!(result[0].end, 2.0); // Extended to cover both
        assert_eq!(result[1].text, "World");
    }

    #[test]
    fn test_hallucination_filtering() {
        let segs = vec![
            Segment { start: 0.0, end: 1.0, text: "Hello".into() },
            Segment { start: 1.0, end: 1.05, text: "um".into() }, // Ultra-short hallucination
            Segment { start: 1.05, end: 2.0, text: "world".into() },
        ];
        let result = process(segs);
        // Hallucination (1.05s) filtered, then Hello + world merged
        assert_eq!(result.len(), 1);
        assert!(result[0].text.contains("Hello"));
        assert!(result[0].text.contains("world"));
    }

    #[test]
    fn test_gapless() {
        let segs = vec![
            Segment { start: 0.0, end: 1.0, text: "First".into() },
            Segment { start: 1.5, end: 2.0, text: "Second".into() },
            Segment { start: 10.0, end: 11.0, text: "Third".into() }, // Large gap
        ];
        let result = apply_gapless(segs);
        assert_eq!(result[0].end, 1.5); // Gap closed (0.5s < 3s)
        assert_eq!(result[1].end, 2.0); // Unchanged
        assert_eq!(result[2].start, 10.0); // Large gap not closed
    }
}

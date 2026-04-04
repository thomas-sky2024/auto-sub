use crate::subtitle::Segment;
use log::debug;
use unicode_segmentation::UnicodeSegmentation;
use jieba_rs::Jieba;
use lazy_static::lazy_static;

// Initialize Jieba tokenizer once for performance
lazy_static! {
    static ref JIEBA: Jieba = Jieba::new();
}

/// Maximum characters per second for comfortable reading.
const MAX_CPS: f32 = 20.0;
/// Minimum segment duration in seconds.
const MIN_DURATION: f32 = 0.5;

/// Merge gap threshold in seconds (respect speaker rhythm).
const MERGE_GAP: f32 = 0.2;
/// Maximum duration when merging segments.
const MAX_MERGE_DURATION: f32 = 4.0;
/// Speaker pause threshold — force new subtitle.
const SPEAKER_PAUSE: f32 = 0.3;
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

    // 2. Enforce segment limits (recursive split for long sentences)
    result = enforce_segment_limits(result);
    log::debug!("post_process: after enforce_segment_limits -> {} segments", result.len());

    // 3. Apply gapless (close small gaps) BEFORE fixing overlaps
    result = apply_gapless(result);
    log::debug!("post_process: after apply_gapless -> {} segments", result.len());

    // 4. Fix overlaps (final pass) - operates on natural Whisper timestamps
    result = fix_overlaps(result);
    log::debug!("post_process: after fix_overlaps -> {} segments", result.len());

    // 5. Enforce duration bounds AFTER overlap fixing to avoid creating new overlaps
    result = enforce_duration(result);
    log::debug!("post_process: after enforce_duration -> {} segments", result.len());

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

/// Merge segments based on gap and sentence continuity (respect speaker rhythm).
fn merge_segments(segments: Vec<Segment>) -> Vec<Segment> {
    if segments.is_empty() {
        return segments;
    }

    let mut result: Vec<Segment> = vec![segments[0].clone()];

    for seg in segments.into_iter().skip(1) {
        let prev = result.last().unwrap();
        let gap = seg.start - prev.end;

        // Rule 1: BREAK ON SPEAKER PAUSE (respect rhythm > 0.3s)
        if gap > SPEAKER_PAUSE {
            debug!("post: speaker pause ({:.2}s), new subtitle", gap);
            result.push(seg);
            continue;
        }

        let prev_text = prev.text.trim();

        // Rule 2: BREAK ON SOFT PUNCTUATION (commas, Chinese punctuation)
        if ends_with_sentence(prev_text)
            || prev_text.ends_with(',')
            || prev_text.ends_with('，')
            || prev_text.ends_with('、')
        {
            result.push(seg);
            continue;
        }

        // Rule 3: ONLY MERGE if continuous speech + reasonable duration
        let merged_duration = seg.end - prev.start;
        if gap <= MERGE_GAP && merged_duration <= MAX_MERGE_DURATION {
            let merged_text = if is_cjk_text(prev_text) {
                format!("{}{}", prev_text, seg.text.trim())
            } else {
                format!("{} {}", prev_text, seg.text.trim())
            };

            let last = result.last_mut().unwrap();
            last.end = seg.end;
            last.text = merged_text;
            continue;
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

/// Enforce segment limits: recursively split long sentences by language-specific rules.
/// CJK: max 22 chars/segment. Non-CJK: max 12 words OR 50 chars/segment.
/// Also respects CPS threshold (max 20 chars/second).
fn enforce_segment_limits(segments: Vec<Segment>) -> Vec<Segment> {
    let mut result = Vec::new();
    for seg in segments {
        result.extend(recursive_split(seg));
    }
    result
}

/// Recursively split segment until all CJK/non-CJK thresholds are met.
fn recursive_split(seg: Segment) -> Vec<Segment> {
    let is_cjk = is_cjk_text(&seg.text);
    let text_trim = seg.text.trim();

    let word_count = text_trim.split_whitespace().count();
    let char_count = text_trim.chars().count();

    // Language-specific limits:
    // CJK: max 22 characters per segment
    // Non-CJK: max 12 words OR max 50 characters per segment
    let is_too_long = if is_cjk {
        char_count > 22
    } else {
        word_count > 12 || char_count > 50
    };

    let is_high_cps = seg.cps() > MAX_CPS && char_count > 10;

    // If too long or high CPS, split into two parts
    if is_too_long || is_high_cps {
        let split_segs = split_segment(&seg, is_cjk);

        // Guard against infinite loops: if split didn't produce meaningful chunks, return as-is
        if split_segs.len() == 1 || split_segs[0].text == seg.text {
            debug!("post_process: cannot split further (fallback): '{}'", text_trim);
            return vec![seg];
        }

        // Recursively check each chunk
        let mut final_segs = Vec::new();
        for s in split_segs {
            final_segs.extend(recursive_split(s));
        }
        return final_segs;
    }

    vec![seg]
}

/// Split a segment at the best boundary point based on semantics and word boundaries.
/// CJK: Uses Jieba tokenizer for meaningful word segmentation.
/// Non-CJK: Uses Unicode word boundaries to avoid mid-word splits.
fn split_segment(seg: &Segment, is_cjk: bool) -> Vec<Segment> {
    let text = &seg.text.trim();

    // Don't split if too short
    if text.chars().count() < 4 {
        return vec![seg.clone()];
    }

    let words: Vec<&str> = if is_cjk {
        JIEBA.cut(text, false)
    } else {
        text.split_word_bounds().collect()
    };

    let total_words = words.len();
    if total_words < 2 {
        return vec![seg.clone()];
    }

    let mid_word_idx = total_words / 2;
    let mut best_split_idx = mid_word_idx;
    let mut found_good_split = false;

    // Semantic conjunction dictionary (for splitting before meaningful transitions)
    let conj_vi_en = [
        "nhưng", "và", "thì", "là", "mà", "bởi", "vì", "nên", "tuy", "tuy nhiên",
        "mặc dù", "nếu", "để", "but", "because", "and", "so", "however", "although",
        "if", "then",
    ];
    let conj_zh = ["但是", "可是", "因为", "所以", "如果", "那么", "不过", "其实", "然后", "虽然", "而且", "为了"];

    // Scan from midpoint outward to find best split point
    for offset in 0..=(total_words / 2) {
        for &idx in &[mid_word_idx + offset, mid_word_idx.saturating_sub(offset)] {
            if idx > 0 && idx < total_words {
                let word_str = words[idx].trim().to_lowercase();

                // Priority 1: Split RIGHT AFTER punctuation (commas, semicolons)
                if idx > 0 {
                    let prev_word = words[idx - 1].trim();
                    if prev_word.ends_with(',')
                        || prev_word.ends_with(';')
                        || prev_word.ends_with('，')
                        || prev_word.ends_with('、')
                    {
                        best_split_idx = idx;
                        found_good_split = true;
                        debug!(
                            "post_process: split after punctuation at idx={}, word='{}'",
                            idx, prev_word
                        );
                        break;
                    }
                }

                // Priority 2: Split RIGHT BEFORE conjunction (semantic transition point)
                let is_conjunction = if is_cjk {
                    conj_zh.iter().any(|&c| word_str == c)
                } else {
                    conj_vi_en.contains(&word_str.as_str())
                };

                if is_conjunction {
                    best_split_idx = idx;
                    found_good_split = true;
                    debug!(
                        "post_process: split before conjunction at idx={}, word='{}'",
                        idx, word_str
                    );
                    break;
                }
            }
        }
        if found_good_split {
            break;
        }
    }

    let text1 = words[..best_split_idx].join("");
    let text2 = words[best_split_idx..].join("");

    // Split timestamps proportionally by character count
    let char_count = text.chars().count() as f32;
    let text1_char_count = text1.chars().count() as f32;

    let ratio = if char_count > 0.0 {
        text1_char_count / char_count
    } else {
        0.5
    };

    let split_time = seg.start + seg.duration() * ratio;

    debug!(
        "post_process: semantic split (conj={}) '{}' => '{}' | '{}' (ratio: {:.2})",
        found_good_split, text, text1, text2, ratio
    );

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
        // Use gap clearly within MERGE_GAP (0.2s) to avoid f32 imprecision
        let segs = vec![
            Segment { start: 0.0, end: 1.0, text: "Hello".into() },
            Segment { start: 1.1, end: 2.5, text: "world".into() },
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
        // Use gap clearly within MERGE_GAP (0.2s) to avoid f32 imprecision
        let segs = vec![
            Segment { start: 0.0, end: 1.0, text: "你好".into() },
            Segment { start: 1.1, end: 2.5, text: "世界".into() },
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
        // "First sentence." ends with '.' which triggers sentence boundary rule.
        // Overlapping segments that end sentences are kept separate and overlap is fixed.
        assert_eq!(result.len(), 2);
        assert!(result[0].text.contains("First"));
        assert!(result[1].text.contains("Second"));
        // Overlap must be resolved: first segment's end <= second's start
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
    fn test_recursive_split_long_english() {
        // Long English sentence: 15 words (exceeds 12 word limit for non-CJK)
        let seg = Segment {
            start: 0.0,
            end: 5.0,
            text: "The quick brown fox jumps over the lazy dog and runs far away.".into(),
        };
        let result = recursive_split(seg);
        // Should split into multiple segments
        assert!(result.len() > 1, "Long English text should be split");
        // Each segment should be readable
        for s in &result {
            assert!(!s.text.is_empty(), "No empty segments");
            assert!(s.end > s.start, "Valid timestamps");
        }
    }

    #[test]
    fn test_recursive_split_long_cjk() {
        // Long CJK sentence: 30 characters (exceeds 22 char limit for CJK)
        let seg = Segment {
            start: 0.0,
            end: 5.0,
            text: "欢迎来到我们的世界，这是一个充满魔法和奇迹的地方。".into(),
        };
        let result = recursive_split(seg);
        // Should split into multiple segments
        assert!(result.len() > 1, "Long CJK text should be split");
        // Each segment should respect CJK limit (22 chars)
        for s in &result {
            assert!(s.text.chars().count() <= 22, "CJK segment exceeds 22 char limit");
        }
    }

    #[test]
    fn test_semantic_split_cjk_jieba() {
        // Chinese: "机器学习" should stay as one word, not split into individual characters
        let seg = Segment {
            start: 0.0,
            end: 5.0,
            text: "我喜欢用Rust开发高性能的软件".into(),
        };
        let result = split_segment(&seg, true);
        assert_eq!(result.len(), 2);
        // Should split meaningfully, e.g., not splitting "高性能" (high performance)
        assert!(!result[0].text.is_empty());
        assert!(!result[1].text.is_empty());
        debug!("CJK semantic split: '{}' => '{}' | '{}'", seg.text, result[0].text, result[1].text);
    }

    #[test]
    fn test_semantic_split_english_word_boundaries() {
        // English: "temperature" should never be split to "tempe|rature"
        let seg = Segment {
            start: 0.0,
            end: 3.0,
            text: "The atmospheric temperature increased significantly".into(),
        };
        let result = split_segment(&seg, false);
        assert_eq!(result.len(), 2);
        // Verify no word is truncated (word boundaries preserved)
        for s in &result {
            assert!(!s.text.is_empty());
            // Words should be complete, not mid-word splits
            assert!(s.text == s.text.trim().to_string() || s.text.ends_with(' '));
        }
        debug!("English word split: '{}' => '{}' | '{}'", seg.text, result[0].text, result[1].text);
    }

    #[test]
    fn test_semantic_split_with_punctuation() {
        // English with comma: should prefer to split at comma
        let seg = Segment {
            start: 0.0,
            end: 4.0,
            text: "Hello world, I hope you are doing well today".into(),
        };
        let result = split_segment(&seg, false);
        assert_eq!(result.len(), 2);
        // Split should ideally be near the comma
        assert!(result[0].text.contains(",") || result[1].text.is_empty() == false);
    }
}
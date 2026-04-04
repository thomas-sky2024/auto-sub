use crate::subtitle::Segment;
use log::{debug, warn};

/// Stage 3: Post-whisper validation.
/// Drops garbage segments and fixes timestamp issues.
pub fn validate(segments: Vec<Segment>) -> Vec<Segment> {
    let mut result: Vec<Segment> = Vec::new();

    for seg in segments {
        // Drop empty text
        if seg.text.trim().is_empty() {
            debug!("validator: dropping empty segment at {:.2}", seg.start);
            continue;
        }

        // Drop zero or negative duration
        if seg.duration() <= 0.0 {
            warn!(
                "validator: dropping zero-duration segment at {:.2}: {}",
                seg.start, seg.text
            );
            continue;
        }

        // Drop segments with impossibly high CPS (> 40 = whisper error)
        if seg.cps() > 40.0 {
            warn!(
                "validator: dropping CPS>40 segment at {:.2}: {} (CPS={:.1})",
                seg.start,
                seg.text,
                seg.cps()
            );
            continue;
        }

        result.push(seg);
    }

    // Fix timestamp overlaps
    for i in 1..result.len() {
        if result[i - 1].end > result[i].start {
            let fixed_end = result[i].start - 0.05;
            debug!(
                "validator: fixing overlap at seg {} ({:.2} → {:.2})",
                i,
                result[i - 1].end,
                fixed_end
            );
            result[i - 1].end = fixed_end.max(result[i - 1].start);
        }
    }

    // Remove any zero-duration segments created by overlap fixing
    result.retain(|seg| seg.duration() > 0.001);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drop_empty() {
        let segs = vec![
            Segment { start: 0.0, end: 1.0, text: "Hello".into() },
            Segment { start: 1.0, end: 2.0, text: "  ".into() },
            Segment { start: 2.0, end: 3.0, text: "World".into() },
        ];
        let result = validate(segs);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_drop_zero_duration() {
        let segs = vec![
            Segment { start: 1.0, end: 1.0, text: "Bad".into() },
            Segment { start: 2.0, end: 3.0, text: "Good".into() },
        ];
        let result = validate(segs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "Good");
    }

    #[test]
    fn test_drop_high_cps() {
        let segs = vec![
            // 50 chars in 0.5s = CPS 100
            Segment {
                start: 0.0,
                end: 0.5,
                text: "A".repeat(50),
            },
            Segment { start: 1.0, end: 3.0, text: "Normal".into() },
        ];
        let result = validate(segs);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_fix_overlap() {
        let segs = vec![
            Segment { start: 0.0, end: 2.5, text: "First".into() },
            Segment { start: 2.0, end: 4.0, text: "Second".into() },
        ];
        let result = validate(segs);
        assert!(result[0].end <= result[1].start);
    }
}

use serde::{Deserialize, Serialize};

/// A single subtitle segment with timing and text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

impl Segment {
    pub fn duration(&self) -> f32 {
        self.end - self.start
    }

    /// Characters per second.
    pub fn cps(&self) -> f32 {
        let d = self.duration();
        if d <= 0.0 {
            return 0.0;
        }
        self.text.chars().count() as f32 / d
    }
}

/// Format a float seconds value into SRT timestamp: HH:MM:SS,mmm
fn format_srt_time(seconds: f32) -> String {
    let total_ms = (seconds * 1000.0).round() as u64;
    let h = total_ms / 3_600_000;
    let m = (total_ms % 3_600_000) / 60_000;
    let s = (total_ms % 60_000) / 1000;
    let ms = total_ms % 1000;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}

/// Serialize segments into SRT format.
pub fn to_srt(segments: &[Segment]) -> String {
    let mut output = String::new();
    for (i, seg) in segments.iter().enumerate() {
        output.push_str(&format!("{}\n", i + 1));
        output.push_str(&format!(
            "{} --> {}\n",
            format_srt_time(seg.start),
            format_srt_time(seg.end)
        ));
        output.push_str(&seg.text);
        output.push_str("\n\n");
    }
    output
}

/// Serialize segments into plain text (one line per segment).
pub fn to_txt(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|s| s.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srt_timestamp() {
        assert_eq!(format_srt_time(0.0), "00:00:00,000");
        assert_eq!(format_srt_time(61.5), "00:01:01,500");
        assert_eq!(format_srt_time(3661.123), "01:01:01,123");
    }

    #[test]
    fn test_cps() {
        let seg = Segment {
            start: 0.0,
            end: 2.0,
            text: "Hello".to_string(),
        };
        assert!((seg.cps() - 2.5).abs() < 0.01);
    }

    #[test]
    fn test_to_srt() {
        let segs = vec![Segment {
            start: 1.0,
            end: 3.5,
            text: "Hello world".to_string(),
        }];
        let srt = to_srt(&segs);
        assert!(srt.contains("1\n"));
        assert!(srt.contains("00:00:01,000 --> 00:00:03,500"));
        assert!(srt.contains("Hello world"));
    }
}

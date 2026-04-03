use crate::error::AutoSubError;
use crate::subtitle::Segment;

/// Apply point-based synchronization using linear interpolation.
///
/// This function takes two reference points (A and B) with their time adjustments
/// and applies a linear transformation to all subtitle segments.
///
/// # Algorithm
///
/// Given two control points:
/// - Point A: original_time = segments[start_idx].start, shift = shift_start_sec
/// - Point B: original_time = segments[end_idx].start, shift = shift_end_sec
///
/// We calculate:
/// - speed_factor = (shift_end - shift_start) / (time_B - time_A)
/// - offset = shift_start - (time_A * speed_factor)
///
/// Then for each segment:
/// - new_time = offset + (old_time * speed_factor)
///
/// # Arguments
///
/// * `segments` - Vector of subtitle segments (will be cloned for safety)
/// * `start_idx` - Index of the first reference point (Point A)
/// * `shift_start_sec` - Time adjustment for Point A (in seconds)
/// * `end_idx` - Index of the second reference point (Point B)
/// * `shift_end_sec` - Time adjustment for Point B (in seconds)
///
/// # Returns
///
/// Result containing adjusted segments or an error if validation fails
///
/// # Errors
///
/// Returns `AutoSubError` if:
/// - Segments vector is empty
/// - Indices are out of bounds
/// - start_idx >= end_idx (must be in ascending order)
/// - Time range between points is zero or negative
pub fn apply_point_sync(
    segments: Vec<Segment>,
    start_idx: usize,
    shift_start_sec: f32,
    end_idx: usize,
    shift_end_sec: f32,
) -> Result<Vec<Segment>, AutoSubError> {
    // ─────────────────────────────────────────────────────────────────────────────
    // VALIDATION
    // ─────────────────────────────────────────────────────────────────────────────

    // Check: Segments not empty
    if segments.is_empty() {
        return Err(AutoSubError::Validation(
            "Segments vector is empty".to_string(),
        ));
    }

    // Check: Indices are valid and in bounds
    if start_idx >= segments.len() || end_idx >= segments.len() {
        return Err(AutoSubError::Validation(
            format!(
                "Index out of bounds: start_idx={}, end_idx={}, len={}",
                start_idx,
                end_idx,
                segments.len()
            ),
        ));
    }

    // Check: Indices are in correct order
    if start_idx >= end_idx {
        return Err(AutoSubError::Validation(
            format!(
                "Invalid index order: start_idx ({}) must be < end_idx ({})",
                start_idx, end_idx
            ),
        ));
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // CALCULATION
    // ─────────────────────────────────────────────────────────────────────────────

    let time_a = segments[start_idx].start;
    let time_b = segments[end_idx].start;

    // Calculate time range between the two points
    let time_diff = time_b - time_a;

    // Ensure we're not dividing by zero
    if time_diff.abs() < 1e-6 {
        return Err(AutoSubError::Validation(
            format!(
                "Time range too small: time_A={:.3}s, time_B={:.3}s (diff={:.6}s)",
                time_a, time_b, time_diff
            ),
        ));
    }

    // Calculate shift rate (m_shift) and shift intercept (c_shift)
    let shift_diff = shift_end_sec - shift_start_sec;
    let m_shift = shift_diff / time_diff;
    let c_shift = shift_start_sec - (time_a * m_shift);

    // Final transformation: new_time = old_time + (m_shift * old_time + c_shift)
    // which is: new_time = (1.0 + m_shift) * old_time + c_shift
    let speed_factor = 1.0 + m_shift;
    let offset = c_shift;

    // ─────────────────────────────────────────────────────────────────────────────
    // TRANSFORMATION
    // ─────────────────────────────────────────────────────────────────────────────

    let mut result = segments;

    result.iter_mut().for_each(|seg| {
        // Apply linear transformation to start time
        seg.start = offset + (seg.start * speed_factor);

        // Apply linear transformation to end time
        seg.end = offset + (seg.end * speed_factor);

        // Ensure times don't go negative (clip to 0.0)
        if seg.start < 0.0 {
            seg.start = 0.0;
        }
        if seg.end < 0.0 {
            seg.end = 0.0;
        }

        // Ensure start <= end (safety measure)
        if seg.start > seg.end {
            std::mem::swap(&mut seg.start, &mut seg.end);
        }
    });

    Ok(result)
}

// ─────────────────────────────────────────────────────────────────────────────
// UNIT TESTS
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_segments() -> Vec<Segment> {
        vec![
            Segment {
                start: 0.0,
                end: 2.5,
                text: "Xin chào".to_string(),
            },
            Segment {
                start: 2.5,
                end: 5.23,
                text: "Bạn khỏe?".to_string(),
            },
            Segment {
                start: 5.23,
                end: 8.10,
                text: "Tôi khỏe".to_string(),
            },
            Segment {
                start: 60.0,
                end: 62.5,
                text: "Middle subtitle".to_string(),
            },
            Segment {
                start: 118.0,
                end: 120.51,
                text: "Tạm biệt".to_string(),
            },
        ]
    }

    #[test]
    fn test_empty_segments() {
        let result = apply_point_sync(vec![], 0, 0.0, 1, 0.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_start_index() {
        let segments = create_test_segments();
        let result = apply_point_sync(segments, 10, 0.0, 2, 0.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_end_index() {
        let segments = create_test_segments();
        let result = apply_point_sync(segments, 0, 0.0, 10, 0.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_incorrect_index_order() {
        let segments = create_test_segments();
        let result = apply_point_sync(segments, 4, 0.0, 0, 0.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_same_time_points() {
        let segments = create_test_segments();
        // Both points refer to the same time
        let result = apply_point_sync(segments, 0, 0.0, 0, 0.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_basic_sync_simple_shift() {
        // A at 0s: shift -0.5s
        // B at 120s: shift +1.2s
        let segments = create_test_segments();
        let result = apply_point_sync(segments, 0, -0.5, 4, 1.2);

        assert!(result.is_ok());
        let synced = result.unwrap();

        // Check first segment: started at 0.0s, shifted -0.5s, clipped to 0.0s
        assert!((synced[0].start - 0.0).abs() < 0.01);
        assert!((synced[0].end - (2.5 * 1.0141 - 0.5)).abs() < 0.01);

        // Check last segment: started at 118.0, shifted by 1.2s
        // time_diff = 118.0 - 0.0 = 118.0
        // shift_diff = 1.2 - (-0.5) = 1.7
        // m_shift = 1.7 / 118.0 = 0.014406...
        // speed_factor = 1.0 + m_shift = 1.014406...
        // new_last_start = 1.014406 * 118.0 - 0.5 = 118.0 + 1.7 - 0.5 = 119.2
        assert!((synced[4].start - 119.2).abs() < 0.01);
    }

    #[test]
    fn test_no_shift() {
        // Both points have 0 shift
        let segments = create_test_segments();
        let original_count = segments.len();
        let result = apply_point_sync(segments.clone(), 0, 0.0, 4, 0.0);

        assert!(result.is_ok());
        let synced = result.unwrap();

        // All times should remain approximately the same
        for i in 0..original_count {
            assert!((synced[i].start - segments[i].start).abs() < 0.01);
        }
    }

    #[test]
    fn test_negative_times_clipped() {
        let mut segments = create_test_segments();
        segments.insert(
            0,
            Segment {
                start: 0.1,
                end: 0.5,
                text: "Very early".to_string(),
            },
        );

        // Shift everything backwards by 1s
        let result = apply_point_sync(segments, 0, -1.0, 5, 0.0);

        assert!(result.is_ok());
        let synced = result.unwrap();

        // First segment should be clipped to 0.0
        assert!(synced[0].start >= 0.0);
        assert!(synced[0].end >= 0.0);
    }

    #[test]
    fn test_preserves_segment_count() {
        let segments = create_test_segments();
        let count = segments.len();
        let result = apply_point_sync(segments, 0, -0.5, 4, 1.2);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), count);
    }

    #[test]
    fn test_preserves_text_content() {
        let segments = create_test_segments();
        for seg in &segments {
            println!("Segment: {}", seg.text);
        }

        let result = apply_point_sync(segments.clone(), 0, -0.5, 4, 1.2);
        assert!(result.is_ok());

        let synced = result.unwrap();
        for i in 0..segments.len() {
            assert_eq!(synced[i].text, segments[i].text);
        }
    }
}

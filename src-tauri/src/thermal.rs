use log::{debug, info};
use sysinfo::System;

/// Performance mode for the pipeline.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum PerformanceMode {
    Balanced,
    MaxSpeed,
}

/// Get recommended thread count based on system state.
pub fn recommended_threads(mode: PerformanceMode) -> usize {
    let base = match mode {
        PerformanceMode::Balanced => 8,
        PerformanceMode::MaxSpeed => 12,
    };

    // Check CPU load and thermal state
    let mut sys = System::new_all();
    sys.refresh_cpu_usage();

    let cpu_count = sys.cpus().len();
    let avg_load: f32 = if cpu_count > 0 {
        sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / cpu_count as f32
    } else {
        0.0
    };

    let threads = if avg_load > 90.0 {
        // System under heavy load — reduce threads
        let reduced = (base as f32 * 0.75).ceil() as usize;
        debug!(
            "thermal: high CPU load ({:.0}%), reducing threads {} → {}",
            avg_load, base, reduced
        );
        reduced
    } else {
        base
    };

    // Never exceed physical CPU count
    let final_threads = threads.min(cpu_count).max(1);
    info!(
        "thermal: mode={:?}, load={:.0}%, threads={}",
        mode, avg_load, final_threads
    );
    final_threads
}

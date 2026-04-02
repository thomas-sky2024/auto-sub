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
    let mut sys = System::new_all();
    
    // Initial refresh to start the counters
    sys.refresh_cpu_usage();
    // Wait a tiny bit for a more accurate delta (sysinfo requirement)
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_usage();

    let physical_cores = sys.physical_core_count().unwrap_or(4);
    let logical_cores = sys.cpus().len();
    
    let base = match mode {
        PerformanceMode::Balanced => physical_cores.min(8),
        PerformanceMode::MaxSpeed => logical_cores.min(12),
    };

    let avg_load: f32 = if logical_cores > 0 {
        sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / logical_cores as f32
    } else {
        0.0
    };

    let threads = if avg_load > 85.0 {
        // System under heavy load — reduce threads more aggressively
        let reduced = (base as f32 * 0.6).ceil() as usize;
        debug!(
            "thermal: high CPU load ({:.0}%), reducing threads {} → {}",
            avg_load, base, reduced
        );
        reduced
    } else if avg_load > 60.0 {
        // Moderate load — slight reduction
        let reduced = (base as f32 * 0.85).ceil() as usize;
        reduced
    } else {
        base
    };

    // Never exceed physical core count even in max speed if load is high, 
    // but allow going up to logical cores if system is idle.
    let final_threads = threads.min(logical_cores).max(1);
    
    info!(
        "thermal: mode={:?}, load={:.0}%, logical={}, physical={}, recommended={}",
        mode, avg_load, logical_cores, physical_cores, final_threads
    );
    final_threads
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PerformanceMode {
    Balanced,
    MaxSpeed,
}

pub fn recommended_threads(mode: PerformanceMode) -> usize {
    match mode {
        PerformanceMode::Balanced => 4,
        PerformanceMode::MaxSpeed => 8,
    }
}

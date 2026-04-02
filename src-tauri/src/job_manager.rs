use crate::error::{AutoSubError, Result};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::process::Child;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

/// Current state of a job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Idle,
    Running { stage: String, percent: f32 },
    Completed,
    Failed { error: String },
    Cancelled,
}

/// Handle to a running job for cancellation.
pub struct JobHandle {
    /// Cancellation sender — sending signals the pipeline to abort.
    pub cancel_tx: oneshot::Sender<()>,
    /// Active child processes to kill on cancel.
    pub children: Vec<Arc<Mutex<Option<Child>>>>,
}

/// Thread-safe job manager.
/// Holds at most one active job at a time (queue is frontend-side).
pub struct JobManager {
    state: Arc<Mutex<JobState>>,
    handle: Arc<Mutex<Option<JobHandle>>>,
}

impl JobManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(JobState::Idle)),
            handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Create a new cancellation token and register the job.
    pub fn start_job(&self) -> Result<(oneshot::Receiver<()>, Arc<Mutex<Option<Child>>>, Arc<Mutex<Option<Child>>>)> {
        let mut state = self.state.lock().unwrap();
        if matches!(*state, JobState::Running { .. }) {
            return Err(AutoSubError::AudioExtract(
                "A job is already running".to_string(),
            ));
        }

        let (cancel_tx, cancel_rx) = oneshot::channel();
        let ffmpeg_child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        let whisper_child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));

        let handle = JobHandle {
            cancel_tx,
            children: vec![ffmpeg_child.clone(), whisper_child.clone()],
        };

        *self.handle.lock().unwrap() = Some(handle);
        *state = JobState::Running {
            stage: "Starting".to_string(),
            percent: 0.0,
        };

        info!("job_manager: job started");
        Ok((cancel_rx, ffmpeg_child, whisper_child))
    }

    /// Update running job stage and progress.
    pub fn update_progress(&self, stage: &str, percent: f32) {
        let mut state = self.state.lock().unwrap();
        if matches!(*state, JobState::Running { .. }) {
            *state = JobState::Running {
                stage: stage.to_string(),
                percent,
            };
            debug!("job_manager: {} - {:.0}%", stage, percent);
        }
    }

    /// Mark job as completed.
    pub fn complete(&self) {
        let mut state = self.state.lock().unwrap();
        *state = JobState::Completed;
        *self.handle.lock().unwrap() = None;
        info!("job_manager: job completed");
    }

    /// Mark job as failed.
    pub fn fail(&self, error: String) {
        let mut state = self.state.lock().unwrap();
        *state = JobState::Failed { error: error.clone() };
        *self.handle.lock().unwrap() = None;
        info!("job_manager: job failed: {}", error);
    }

    /// Cancel the current job — kills child processes.
    pub fn cancel(&self) {
        let mut handle = self.handle.lock().unwrap();
        if let Some(h) = handle.take() {
            // Kill all child processes
            for child_arc in &h.children {
                let mut c = child_arc.lock().unwrap();
                if let Some(ref mut child) = *c {
                    let _ = child.kill();
                    let _ = child.wait(); // Clean up zombies
                    info!("job_manager: killed and reaped child process");
                }
            }
            // Signal the pipeline
            let _ = h.cancel_tx.send(());
        }

        let mut state = self.state.lock().unwrap();
        *state = JobState::Cancelled;
        info!("job_manager: job cancelled");
    }

    /// Get current job state.
    pub fn state(&self) -> JobState {
        self.state.lock().unwrap().clone()
    }

    /// Reset to idle after completion/failure/cancel.
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        *state = JobState::Idle;
    }
}

impl Default for JobManager {
    fn default() -> Self {
        Self::new()
    }
}

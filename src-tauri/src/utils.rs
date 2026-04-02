use crate::error::{AutoSubError, Result};
use std::fs;
use std::io::Write;
use std::time::Duration;
use log::debug;

/// Generic retry helper for transient failures.
/// Retries the provided closure `f` up to `attempts` times with a 2-second delay.
pub async fn retry<F, Fut, T>(mut f: F, attempts: u8) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut last_err = AutoSubError::AudioExtract("Retry failed".to_string());
    for i in 0..attempts {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_err = e;
                if i == attempts - 1 {
                    return Err(last_err);
                }
                debug!("Retry attempt {}/{} failed: {}. Retrying in 2s...", i + 1, attempts, last_err);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
    Err(last_err)
}

/// Atomically write content to a file by writing to a .tmp file and then renaming it.
/// This prevents file corruption if the process is interrupted during write.
pub fn atomic_write(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "Parent directory not found")
    })?;
    
    let file_name = path.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid file name")
    })?;
    
    let tmp_name = format!("{}.tmp", file_name.to_string_lossy());
    let tmp_path = parent.join(tmp_name);
    
    {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?; // Ensure data is flushed to disk
    }
    
    fs::rename(tmp_path, path)?;
    Ok(())
}

#[allow(dead_code)]
/// Log a debug message if the DEBUG environment variable is set.
pub fn log_debug(msg: &str) {
    if std::env::var("DEBUG").is_ok() {
        debug!("[DEBUG] {}", msg);
    }
}


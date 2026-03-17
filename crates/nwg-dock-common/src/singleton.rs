use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::config::paths::temp_dir;

/// Creates a lock file for single-instance enforcement.
///
/// Returns `Ok(LockFile)` if we acquired the lock, or `Err(pid)` if another
/// instance holds it (pid is the running instance's PID, if readable).
pub fn acquire_lock(app_name: &str) -> Result<LockFile, Option<u32>> {
    let user = std::env::var("USER").unwrap_or_default();
    let user_hash = md5_hash(&user);
    let lock_path = temp_dir().join(format!("{}-{}.lock", app_name, user_hash));

    if lock_path.exists() {
        // Try to read the existing PID
        if let Ok(content) = fs::read_to_string(&lock_path)
            && let Ok(pid) = content.trim().parse::<u32>()
        {
            // Check if the process is still running
            let proc_path = format!("/proc/{}", pid);
            if Path::new(&proc_path).exists() {
                return Err(Some(pid));
            }
            // Process is dead, remove stale lock
            log::info!("Removing stale lock file (pid {} no longer running)", pid);
        }
        let _ = fs::remove_file(&lock_path);
    }

    // Create new lock file with our PID
    let mut file = fs::File::create(&lock_path).map_err(|_| None)?;
    write!(file, "{}", std::process::id()).map_err(|_| None)?;

    Ok(LockFile { path: lock_path })
}

/// RAII guard that removes the lock file on drop.
pub struct LockFile {
    path: PathBuf,
}

impl LockFile {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for LockFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Finds the PID of a running instance (if any) without acquiring the lock.
pub fn find_running_pid(app_name: &str) -> Option<u32> {
    let user = std::env::var("USER").unwrap_or_default();
    let user_hash = md5_hash(&user);
    let lock_path = temp_dir().join(format!("{}-{}.lock", app_name, user_hash));

    let content = fs::read_to_string(&lock_path).ok()?;
    let pid: u32 = content.trim().parse().ok()?;
    let proc_path = format!("/proc/{}", pid);
    if Path::new(&proc_path).exists() {
        Some(pid)
    } else {
        None
    }
}

/// Simple MD5 hash of a string, returned as hex.
fn md5_hash(text: &str) -> String {
    // Minimal MD5 implementation to avoid adding a dependency.
    // We only use this for lock file naming, not cryptography.
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

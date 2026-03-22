use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::config::paths::temp_dir;

/// Creates a lock file for single-instance enforcement.
///
/// Returns `Ok(LockFile)` if we acquired the lock, or `Err(pid)` if another
/// instance holds it (pid is the running instance's PID, if readable).
pub fn acquire_lock(app_name: &str) -> Result<LockFile, Option<u32>> {
    let user = std::env::var("USER").unwrap_or_default();
    let user_hash = stable_hash(&user);
    let lock_path = temp_dir().join(format!("{}-{}.lock", app_name, user_hash));

    if lock_path.exists() {
        // Try to read the existing PID
        if let Ok(content) = fs::read_to_string(&lock_path)
            && let Ok(pid) = content.trim().parse::<u32>()
        {
            // Check if the process is still running AND is our binary
            if pid_is_our_instance(pid) {
                return Err(Some(pid));
            }
            // Process is dead, remove stale lock
            log::info!("Removing stale lock file (pid {} no longer running)", pid);
        }
        if let Err(e) = fs::remove_file(&lock_path) {
            log::warn!("Failed to remove stale lock {}: {}", lock_path.display(), e);
        }
    }

    // Create lock file atomically (O_CREAT | O_EXCL — fails if file exists)
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .or_else(|_| {
            // Another instance may have just created it — retry after removing stale
            let _ = fs::remove_file(&lock_path);
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
        })
        .map_err(|_| None)?;
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
/// Validates the PID via `pid_is_our_instance` to prevent stale lock PID reuse.
pub fn find_running_pid(app_name: &str) -> Option<u32> {
    let user = std::env::var("USER").unwrap_or_default();
    let user_hash = stable_hash(&user);
    let lock_path = temp_dir().join(format!("{}-{}.lock", app_name, user_hash));

    let content = fs::read_to_string(&lock_path).ok()?;
    let pid: u32 = content.trim().parse().ok()?;
    pid_is_our_instance(pid).then_some(pid)
}

/// Returns true if the given PID is a running process whose executable matches ours.
/// Prevents acting on recycled PIDs from unrelated processes.
fn pid_is_our_instance(pid: u32) -> bool {
    let exe_path = format!("/proc/{}/exe", pid);
    match fs::read_link(&exe_path) {
        Ok(exe) => {
            let exe_name = exe
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let our_exe = std::env::current_exe()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()));
            if our_exe.as_deref() == Some(&exe_name) {
                true
            } else {
                log::debug!(
                    "PID {} exe '{}' doesn't match ours ({:?}), stale lock",
                    pid,
                    exe_name,
                    our_exe
                );
                false
            }
        }
        Err(e) => {
            log::debug!("Failed to read {}: {}", exe_path, e);
            false
        }
    }
}

/// Stable hash of a string for lock file naming.
///
/// Uses djb2 algorithm — deterministic across Rust versions and platforms.
/// Not cryptographic, only used for unique file naming per user.
fn stable_hash(text: &str) -> String {
    let mut hash: u64 = 5381; // djb2 initial seed
    for b in text.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64); // djb2: hash * 33 + byte
    }
    format!("{:016x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_stability() {
        let h1 = stable_hash("testuser");
        let h2 = stable_hash("testuser");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_different_inputs() {
        let h_alice = stable_hash("alice");
        let h_bob = stable_hash("bob");
        assert_ne!(h_alice, h_bob);
    }

    #[test]
    fn acquire_and_release() {
        // Use a unique app name to avoid interference with other tests.
        let unique_suffix = std::process::id();
        let app_name = format!("test-singleton-{}", unique_suffix);
        let lock = acquire_lock(&app_name).expect("should acquire lock");
        let lock_path = lock.path().to_path_buf();
        assert!(lock_path.exists(), "lock file should exist after acquire");

        // Drop the LockFile RAII guard, which should remove the file.
        drop(lock);
        assert!(
            !lock_path.exists(),
            "lock file should be removed after drop"
        );
    }

    #[test]
    fn find_running_pid_no_lock() {
        // A non-existent app name should return None.
        let result = find_running_pid("nonexistent-app-zzz-test-12345");
        assert!(result.is_none());
    }
}

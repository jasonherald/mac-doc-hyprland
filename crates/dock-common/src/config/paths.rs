use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Returns the XDG cache directory.
pub fn cache_dir() -> Option<PathBuf> {
    if let Ok(dir) = env::var("XDG_CACHE_HOME") {
        return Some(PathBuf::from(dir));
    }
    dirs::cache_dir()
}

/// Returns the XDG config directory for the given app name.
pub fn config_dir(app_name: &str) -> PathBuf {
    if let Ok(dir) = env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join(app_name);
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".config").join(app_name);
    }
    PathBuf::from(".config").join(app_name)
}

/// Returns the temp directory, checking TMPDIR/TEMP/TMP then falling back to /tmp.
pub fn temp_dir() -> PathBuf {
    for var in &["TMPDIR", "TEMP", "TMP"] {
        if let Ok(dir) = env::var(var) {
            return PathBuf::from(dir);
        }
    }
    PathBuf::from("/tmp")
}

/// Searches XDG data directories for a subdirectory matching `app_name`.
/// Returns the parent data dir (e.g. `/usr/share` if `/usr/share/app_name` exists).
pub fn find_data_home(app_name: &str) -> Option<PathBuf> {
    let mut search_dirs = Vec::new();

    if let Ok(xdg) = env::var("XDG_DATA_HOME") {
        search_dirs.push(PathBuf::from(xdg));
    } else if let Ok(home) = env::var("HOME") {
        search_dirs.push(PathBuf::from(home).join(".local/share"));
    }

    if let Ok(xdg_dirs) = env::var("XDG_DATA_DIRS") {
        search_dirs.extend(xdg_dirs.split(':').map(PathBuf::from));
    } else {
        search_dirs.push(PathBuf::from("/usr/local/share"));
        search_dirs.push(PathBuf::from("/usr/share"));
    }

    search_dirs
        .into_iter()
        .find(|d| d.join(app_name).exists())
}

/// Creates a directory and all parents if it doesn't exist.
pub fn ensure_dir(dir: &Path) -> std::io::Result<()> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
        log::info!("Created directory: {}", dir.display());
    }
    Ok(())
}

/// Copies a file from `src` to `dst`, preserving permissions.
pub fn copy_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    log::info!("Copying file: {}", dst.display());
    fs::copy(src, dst)?;
    let metadata = fs::metadata(src)?;
    fs::set_permissions(dst, metadata.permissions())?;
    Ok(())
}

/// Reads a text file and returns non-empty trimmed lines.
pub fn load_text_lines(path: &Path) -> std::io::Result<Vec<String>> {
    let content = fs::read_to_string(path)?;
    Ok(content
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// Reads a text file and returns its full contents.
pub fn read_text_file(path: &Path) -> std::io::Result<String> {
    fs::read_to_string(path)
}

//! SHM file cleanup watchdog.
//!
//! Ensures SHM files are cleaned up even on SIGKILL/SIGSEGV by:
//! 1. Forking a watchdog process that monitors parent via pipe
//! 2. Cleaning up stale SHM files on startup
//! 3. Writing .meta files to prevent PID reuse issues

#[cfg(unix)]
use std::io::Write;
use std::path::{Path, PathBuf};

/// Atomic file logger for watchdog process.
/// Uses temp file + fsync + rename for atomic writes.
#[cfg(unix)]
fn watchdog_log(msg: &str) {
    use std::os::unix::fs::OpenOptionsExt;

    let log_path = std::env::temp_dir().join("roam-shm-watchdog.log");
    let tmp_path =
        std::env::temp_dir().join(format!("roam-shm-watchdog.log.tmp.{}", std::process::id()));

    // Read existing log (or start empty)
    let mut contents = std::fs::read(&log_path).unwrap_or_default();

    // Rotate if > 1MB: keep last 500KB
    if contents.len() > 1_000_000 {
        let keep_from = contents.len().saturating_sub(500_000);
        contents = contents[keep_from..].to_vec();
    }

    // Append new line with timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("[{}] {}\n", timestamp, msg);
    contents.extend_from_slice(line.as_bytes());

    // Atomic write: temp file + fsync + rename
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp_path)
    {
        let _ = file.write_all(&contents);
        let _ = file.sync_all(); // fsync
        drop(file);
        let _ = std::fs::rename(&tmp_path, &log_path); // atomic
    }
}

/// Spawn a watchdog process that cleans up SHM files when parent dies.
/// Uses a pipe to detect parent death - no polling needed.
#[cfg(unix)]
pub fn spawn_watchdog(shm_path: PathBuf) -> std::io::Result<()> {
    // Create pipe for parent death detection
    let (reader, writer) = std::os::unix::net::UnixStream::pair()?;

    let parent_pid = std::process::id();

    match unsafe { libc::fork() } {
        -1 => Err(std::io::Error::last_os_error()),
        0 => {
            // Child: watchdog process
            drop(writer); // Close write end

            watchdog_log(&format!(
                "Watchdog started for parent PID {} ({})",
                parent_pid,
                shm_path.display()
            ));

            // Block on read - when parent dies, pipe closes and read returns 0
            let mut buf = [0u8; 1];
            let _ = std::io::Read::read(&mut &reader, &mut buf);

            watchdog_log(&format!("Parent PID {} died, cleaning up", parent_pid));

            // Parent is dead - clean up
            let meta_path = shm_path.with_extension("meta");
            let _ = std::fs::remove_file(&shm_path);
            let _ = std::fs::remove_file(&meta_path);

            watchdog_log(&format!("Cleanup complete for PID {}", parent_pid));

            std::process::exit(0);
        }
        _pid => {
            // Parent: keep writer alive for lifetime of process
            std::mem::forget(writer);
            drop(reader); // Close read end
            // Watchdog spawned successfully
            Ok(())
        }
    }
}

/// Write a .meta file alongside the SHM file containing the current exe path.
/// Used for PID verification during startup cleanup.
pub fn write_meta_file(shm_path: &Path) -> std::io::Result<()> {
    let meta_path = shm_path.with_extension("meta");
    let exe_path = std::env::current_exe()?;
    std::fs::write(&meta_path, exe_path.to_string_lossy().as_bytes())?;
    // Meta file written successfully
    Ok(())
}

/// Get the executable path for a given PID.
/// Returns None if process doesn't exist or we can't read it.
#[cfg(target_os = "linux")]
fn get_process_exe(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{}/exe", pid)).ok()
}

#[cfg(target_os = "macos")]
fn get_process_exe(pid: u32) -> Option<PathBuf> {
    use libproc::libproc::proc_pid::pidpath;

    pidpath(pid as i32).ok().map(PathBuf::from)
}

#[cfg(target_os = "windows")]
fn get_process_exe(_pid: u32) -> Option<PathBuf> {
    // Windows uses Auto cleanup, no need for this
    None
}

/// Get the SHM directory for storing segment files.
/// Creates the directory if it doesn't exist.
fn get_shm_dir() -> std::io::Result<PathBuf> {
    let shm_dir = std::env::temp_dir().join("roam-shm");
    std::fs::create_dir_all(&shm_dir)?;
    Ok(shm_dir)
}

/// Clean up stale SHM files from previous runs.
/// Checks if the process is still alive and matches the expected exe.
///
/// # Arguments
///
/// * `prefix` - File prefix (e.g., "dodeca-shm", "myapp-shm")
pub fn cleanup_stale_shm_files(prefix: &str) -> std::io::Result<()> {
    let shm_dir = get_shm_dir()?;
    let prefix_with_dash = format!("{}-", prefix);

    // Find all {prefix}-*.meta files in roam-shm directory
    let entries = std::fs::read_dir(&shm_dir)?;

    for entry in entries.flatten() {
        let path = entry.path();

        // Only process .meta files
        if path.extension().and_then(|s| s.to_str()) != Some("meta") {
            continue;
        }

        let file_name = match path.file_name().and_then(|s| s.to_str()) {
            Some(name) => name,
            None => continue,
        };

        // Check if it's a {prefix}-*.meta file
        if !file_name.starts_with(&prefix_with_dash) {
            continue;
        }

        // Extract PID from filename: {prefix}-12345.meta
        let pid_str = file_name
            .strip_prefix(&prefix_with_dash)
            .and_then(|s| s.strip_suffix(".meta"));

        let pid: u32 = match pid_str.and_then(|s| s.parse().ok()) {
            Some(p) => p,
            None => continue,
        };

        // Read expected exe path from .meta file
        let expected_exe = match std::fs::read_to_string(&path) {
            Ok(contents) => PathBuf::from(contents.trim()),
            Err(_) => continue,
        };

        // Check if process is alive and matches expected exe
        let should_delete = if let Some(actual_exe) = get_process_exe(pid) {
            // Process exists - check if it's the same exe
            actual_exe != expected_exe
        } else {
            // Process doesn't exist
            true
        };

        if should_delete {
            let shm_path = shm_dir.join(format!("{}-{}", prefix, pid));

            // Cleaning up stale SHM files

            let _ = std::fs::remove_file(&shm_path);
            let _ = std::fs::remove_file(&path);
        }
    }

    Ok(())
}

/// Get the path for a new SHM file.
/// Creates the roam-shm directory if it doesn't exist.
pub fn get_shm_path(prefix: &str, pid: u32) -> std::io::Result<PathBuf> {
    let shm_dir = get_shm_dir()?;
    Ok(shm_dir.join(format!("{}-{}", prefix, pid)))
}

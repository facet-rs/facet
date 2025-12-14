//! macOS implementation using pipe-based parent death detection.
//!
//! Since macOS doesn't have PR_SET_PDEATHSIG, we use a pipe:
//! 1. Parent creates a pipe before spawning the child
//! 2. Parent keeps the write end open (it's automatically closed on parent death)
//! 3. Child inherits the read end and spawns a watchdog thread
//! 4. Watchdog blocks on read() - when parent dies, pipe closes, read returns 0
//! 5. Watchdog calls exit() to terminate the child

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

use std::process::{Child, Command};

use crate::DEATH_PIPE_ENV;

/// Check for death pipe and start watchdog if present.
///
/// This should be called early in the child process. If the parent used
/// `spawn_dying_with_parent`, this will start a background thread that
/// monitors the pipe and exits when the parent dies.
pub fn die_with_parent() {
    if let Ok(fd_str) = std::env::var(DEATH_PIPE_ENV)
        && let Ok(fd) = fd_str.parse::<RawFd>()
    {
        // Take ownership of the FD
        let owned_fd = unsafe { OwnedFd::from_raw_fd(fd) };
        start_watchdog(owned_fd);
    }
}

/// Start the watchdog thread that monitors the death pipe.
fn start_watchdog(fd: OwnedFd) {
    std::thread::spawn(move || {
        let raw_fd = fd.as_raw_fd();
        let mut buf = [0u8; 1];

        // Block on read - this will return 0 (EOF) when parent closes the pipe
        // (either explicitly or by dying)
        loop {
            let result = unsafe { libc::read(raw_fd, buf.as_mut_ptr() as *mut _, 1) };

            if result <= 0 {
                // EOF or error - parent is gone, time to die
                eprintln!("ur-taking-me-with-you: parent died, exiting");
                std::process::exit(0);
            }
            // If we somehow got data, just keep reading
        }
    });
}

/// Death pipe file descriptors.
struct DeathPipe {
    read_fd: RawFd,
    write_fd: RawFd,
}

/// Create a death pipe and configure FD flags.
fn create_death_pipe() -> io::Result<DeathPipe> {
    let mut fds = [0 as libc::c_int; 2];
    let result = unsafe { libc::pipe(fds.as_mut_ptr()) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }

    let read_fd = fds[0];
    let write_fd = fds[1];

    // Clear FD_CLOEXEC on the read end so child inherits it
    unsafe {
        let flags = libc::fcntl(read_fd, libc::F_GETFD);
        if flags != -1 {
            libc::fcntl(read_fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
        }
    }

    // Set FD_CLOEXEC on write end so it stays in parent only
    unsafe {
        let flags = libc::fcntl(write_fd, libc::F_GETFD);
        if flags != -1 {
            libc::fcntl(write_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
        }
    }

    Ok(DeathPipe { read_fd, write_fd })
}

/// Finalize the death pipe after spawning the child.
fn finalize_death_pipe(pipe: DeathPipe) {
    // Close read end in parent - child has its own copy
    unsafe { libc::close(pipe.read_fd) };

    // Keep write_fd open in parent - it will close when parent exits
    // Leak it intentionally so it stays open for the lifetime of the parent
    std::mem::forget(unsafe { OwnedFd::from_raw_fd(pipe.write_fd) });
}

/// Spawn a child that will die when this process dies.
pub fn spawn_dying_with_parent(mut command: Command) -> io::Result<Child> {
    let pipe = create_death_pipe()?;
    command.env(DEATH_PIPE_ENV, pipe.read_fd.to_string());
    let child = command.spawn()?;
    finalize_death_pipe(pipe);
    Ok(child)
}

/// Spawn a child (async) that will die when this process dies.
#[cfg(feature = "tokio")]
pub fn spawn_dying_with_parent_async(
    mut command: tokio::process::Command,
) -> io::Result<tokio::process::Child> {
    let pipe = create_death_pipe()?;
    command.env(DEATH_PIPE_ENV, pipe.read_fd.to_string());
    let child = command.spawn()?;
    finalize_death_pipe(pipe);
    Ok(child)
}

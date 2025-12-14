//! Fallback for unsupported platforms

use std::io;
use std::process::{Child, Command};

pub fn die_with_parent() {
    eprintln!("ur-taking-me-with-you: die_with_parent() not supported on this platform");
}

pub fn spawn_dying_with_parent(mut command: Command) -> io::Result<Child> {
    eprintln!(
        "ur-taking-me-with-you: spawn_dying_with_parent() not supported on this platform, \
         child may outlive parent"
    );
    command.spawn()
}

#[cfg(feature = "tokio")]
pub fn spawn_dying_with_parent_async(
    mut command: tokio::process::Command,
) -> io::Result<tokio::process::Child> {
    eprintln!(
        "ur-taking-me-with-you: spawn_dying_with_parent_async() not supported on this platform, \
         child may outlive parent"
    );
    command.spawn()
}

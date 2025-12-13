use std::io;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::{CREATE_SUSPENDED, PROCESS_INFORMATION};

/// Configure the current process to die when its parent dies.
///
/// On Windows, this creates a job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`
/// and assigns the current process to it. The job handle is intentionally leaked
/// so that when the parent process exits, all handles close and the job (including
/// this process) is terminated.
///
/// # Safety
///
/// This function creates a job object and assigns the current process to it.
/// Once assigned, the process cannot be moved to a different job (Windows limitation).
pub fn die_with_parent() {
    unsafe {
        // Create a job object
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job == 0 {
            eprintln!("ur-taking-me-with-you: Failed to create job object");
            return;
        }

        // Configure the job to kill all processes when the last handle closes
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        let result = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );

        if result == 0 {
            eprintln!("ur-taking-me-with-you: Failed to set job object information");
            CloseHandle(job);
            return;
        }

        // Assign current process to the job
        let current_process = windows_sys::Win32::System::Threading::GetCurrentProcess();
        let result = AssignProcessToJobObject(job, current_process);

        if result == 0 {
            eprintln!("ur-taking-me-with-you: Failed to assign process to job object");
            CloseHandle(job);
            return;
        }

        // Intentionally leak the job handle so it stays open for the lifetime of the process
        // When the parent dies, all handles (including this one) will be closed,
        // triggering JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        std::mem::forget(job);
    }
}

/// Spawn a child process that will die when this (parent) process dies.
///
/// On Windows, this creates a job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`,
/// spawns the child process, and immediately assigns it to the job.
///
/// The job handle is intentionally leaked so that when the parent process exits,
/// all handles close and the job (including the child) is terminated.
///
/// Note: There is a small race window between spawn and job assignment. For maximum
/// reliability, have the child call `die_with_parent()` early in its main function.
pub fn spawn_dying_with_parent(command: Command) -> io::Result<Child> {
    unsafe {
        // Create a job object
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job == 0 {
            return Err(io::Error::last_os_error());
        }

        // Configure the job to kill all processes when the last handle closes
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        let result = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );

        if result == 0 {
            let err = io::Error::last_os_error();
            CloseHandle(job);
            return Err(err);
        }

        // Spawn the child normally
        let child = command.spawn()?;

        // Get the child process handle
        use std::os::windows::io::AsRawHandle;
        let child_handle = child.as_raw_handle() as HANDLE;

        // Assign child to the job
        // This works as long as the child hasn't created its own child processes yet
        let result = AssignProcessToJobObject(job, child_handle);

        if result == 0 {
            let err = io::Error::last_os_error();
            CloseHandle(job);
            // Note: Child is already running. We can't easily kill it here without
            // potentially breaking the Child handle's Drop impl, so we just return error
            return Err(err);
        }

        // Leak the job handle so it stays open for the lifetime of the parent
        std::mem::forget(job);

        Ok(child)
    }
}

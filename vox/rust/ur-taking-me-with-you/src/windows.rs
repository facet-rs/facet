use std::io;
use std::process::{Child, Command};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};

/// Create a job object configured to kill processes when closed.
fn create_job_object() -> io::Result<HANDLE> {
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job == 0 {
            return Err(io::Error::last_os_error());
        }

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

        Ok(job)
    }
}

/// Assign a process to a job and leak the job handle.
fn assign_and_leak_job(job: HANDLE, process_handle: HANDLE) -> io::Result<()> {
    unsafe {
        let result = AssignProcessToJobObject(job, process_handle);
        if result == 0 {
            let err = io::Error::last_os_error();
            CloseHandle(job);
            return Err(err);
        }
        // Leak the job handle so it stays open for the lifetime of the parent
        std::mem::forget(job);
        Ok(())
    }
}

/// Configure the current process to die when its parent dies.
pub fn die_with_parent() {
    let job = match create_job_object() {
        Ok(j) => j,
        Err(e) => {
            eprintln!("ur-taking-me-with-you: Failed to create job object: {}", e);
            return;
        }
    };

    let current_process = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() };
    if let Err(e) = assign_and_leak_job(job, current_process) {
        eprintln!(
            "ur-taking-me-with-you: Failed to assign process to job: {}",
            e
        );
    }
}

/// Spawn a child process that will die when this (parent) process dies.
pub fn spawn_dying_with_parent(command: Command) -> io::Result<Child> {
    let job = create_job_object()?;
    let child = command.spawn()?;

    use std::os::windows::io::AsRawHandle;
    let child_handle = child.as_raw_handle() as HANDLE;
    assign_and_leak_job(job, child_handle)?;

    Ok(child)
}

/// Spawn a child process (async) that will die when this (parent) process dies.
#[cfg(feature = "tokio")]
pub fn spawn_dying_with_parent_async(
    command: tokio::process::Command,
) -> io::Result<tokio::process::Child> {
    let job = create_job_object()?;
    let child = command.spawn()?;

    use std::os::windows::io::AsRawHandle;
    let child_handle = child.as_raw_handle() as HANDLE;
    assign_and_leak_job(job, child_handle)?;

    Ok(child)
}

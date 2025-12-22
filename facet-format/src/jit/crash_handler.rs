//! Signal handler to pause on crash for lldb debugging.

use std::sync::atomic::{AtomicBool, Ordering};

static HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Install signal handlers that pause on crash to allow lldb attachment.
///
/// Call this in debug builds to enable crash debugging. When a signal is caught,
/// the process will print its PID and sleep for 60 seconds to allow:
///   lldb -p <pid>
pub fn install_crash_handler() {
    if HANDLER_INSTALLED.swap(true, Ordering::SeqCst) {
        return; // Already installed
    }

    extern "C" fn crash_handler(sig: libc::c_int) {
        let pid = unsafe { libc::getpid() };

        eprintln!();
        eprintln!("╔══════════════════════════════════════════════════════════╗");
        eprintln!(
            "║  CRASH DETECTED - Signal {}                              ",
            sig
        );
        eprintln!("╚══════════════════════════════════════════════════════════╝");
        eprintln!();
        eprintln!("Process ID: {}", pid);
        eprintln!();
        eprintln!("To attach with lldb:");
        eprintln!("  lldb -p {}", pid);
        eprintln!();
        eprintln!("Then in lldb:");
        eprintln!("  (lldb) bt all");
        eprintln!("  (lldb) frame variable");
        eprintln!("  (lldb) register read");
        eprintln!("  (lldb) memory read <address>");
        eprintln!();
        eprintln!("Sleeping for 60 seconds...");
        eprintln!();

        std::thread::sleep(std::time::Duration::from_secs(60));

        // Re-raise with default handler
        unsafe {
            libc::signal(sig, libc::SIG_DFL);
            libc::raise(sig);
        }
    }

    unsafe {
        libc::signal(libc::SIGSEGV, crash_handler as libc::sighandler_t);
        libc::signal(libc::SIGABRT, crash_handler as libc::sighandler_t);
        libc::signal(libc::SIGBUS, crash_handler as libc::sighandler_t);
        libc::signal(libc::SIGILL, crash_handler as libc::sighandler_t);
        libc::signal(libc::SIGTRAP, crash_handler as libc::sighandler_t);
    }

    eprintln!("[JIT] Crash handler installed (catches SIGSEGV, SIGABRT, SIGBUS, SIGILL, SIGTRAP)");
}

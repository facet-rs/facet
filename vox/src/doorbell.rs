// src/doorbell.rs

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

/// A doorbell for waking up a peer.
///
/// On Linux, this uses eventfd for efficiency.
/// On other platforms, falls back to a pipe.
pub struct Doorbell {
    #[cfg(target_os = "linux")]
    fd: OwnedFd,

    #[cfg(not(target_os = "linux"))]
    read_fd: OwnedFd,
    #[cfg(not(target_os = "linux"))]
    write_fd: OwnedFd,
}

/// A doorbell that can only ring (write side)
pub struct DoorbellSender {
    #[cfg(target_os = "linux")]
    fd: RawFd,

    #[cfg(not(target_os = "linux"))]
    fd: OwnedFd,
}

/// A doorbell that can only wait (read side)
pub struct DoorbellReceiver {
    #[cfg(target_os = "linux")]
    fd: OwnedFd,

    #[cfg(not(target_os = "linux"))]
    fd: OwnedFd,
}

impl Doorbell {
    /// Create a new doorbell.
    pub fn new() -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            // Use eventfd with EFD_NONBLOCK | EFD_CLOEXEC
            let fd = unsafe {
                let fd = libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC);
                if fd < 0 {
                    return Err(io::Error::last_os_error());
                }
                OwnedFd::from_raw_fd(fd)
            };
            Ok(Doorbell { fd })
        }

        #[cfg(not(target_os = "linux"))]
        {
            // Fallback to pipe
            let mut fds = [0i32; 2];
            let result = unsafe { libc::pipe(fds.as_mut_ptr()) };
            if result < 0 {
                return Err(io::Error::last_os_error());
            }

            // Set non-blocking
            for &fd in &fds {
                unsafe {
                    let flags = libc::fcntl(fd, libc::F_GETFL);
                    libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                    libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                }
            }

            Ok(Doorbell {
                read_fd: unsafe { OwnedFd::from_raw_fd(fds[0]) },
                write_fd: unsafe { OwnedFd::from_raw_fd(fds[1]) },
            })
        }
    }

    /// Ring the doorbell (wake up the waiter).
    pub fn ring(&self) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            let val: u64 = 1;
            let result = unsafe {
                libc::write(
                    self.fd.as_raw_fd(),
                    &val as *const u64 as *const libc::c_void,
                    8,
                )
            };
            if result < 0 {
                let err = io::Error::last_os_error();
                // EAGAIN is ok for non-blocking
                if err.kind() != io::ErrorKind::WouldBlock {
                    return Err(err);
                }
            }
            Ok(())
        }

        #[cfg(not(target_os = "linux"))]
        {
            let val: [u8; 1] = [1];
            let result = unsafe {
                libc::write(
                    self.write_fd.as_raw_fd(),
                    val.as_ptr() as *const libc::c_void,
                    1,
                )
            };
            if result < 0 {
                let err = io::Error::last_os_error();
                if err.kind() != io::ErrorKind::WouldBlock {
                    return Err(err);
                }
            }
            Ok(())
        }
    }

    /// Wait for the doorbell to ring (blocking).
    /// Returns the number of times it was rung since last wait.
    pub fn wait_blocking(&self) -> io::Result<u64> {
        #[cfg(target_os = "linux")]
        {
            // First, make it blocking temporarily
            let fd = self.fd.as_raw_fd();
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
            }

            let mut val: u64 = 0;
            let result = unsafe {
                libc::read(fd, &mut val as *mut u64 as *mut libc::c_void, 8)
            };

            // Restore non-blocking
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }

            if result < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(val)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let fd = self.read_fd.as_raw_fd();
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
            }

            let mut buf = [0u8; 64];
            let result = unsafe {
                libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
            };

            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }

            if result < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(result as u64)
        }
    }

    /// Try to consume any pending rings (non-blocking).
    /// Returns Ok(count) if rings were consumed, Ok(0) if none pending.
    pub fn try_consume(&self) -> io::Result<u64> {
        #[cfg(target_os = "linux")]
        {
            let mut val: u64 = 0;
            let result = unsafe {
                libc::read(
                    self.fd.as_raw_fd(),
                    &mut val as *mut u64 as *mut libc::c_void,
                    8,
                )
            };
            if result < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::WouldBlock {
                    return Ok(0);
                }
                return Err(err);
            }
            Ok(val)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let mut buf = [0u8; 64];
            let result = unsafe {
                libc::read(
                    self.read_fd.as_raw_fd(),
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                )
            };
            if result < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::WouldBlock {
                    return Ok(0);
                }
                return Err(err);
            }
            Ok(result as u64)
        }
    }

    /// Get the raw fd for use with poll/epoll/kqueue.
    pub fn as_raw_fd(&self) -> RawFd {
        #[cfg(target_os = "linux")]
        { self.fd.as_raw_fd() }

        #[cfg(not(target_os = "linux"))]
        { self.read_fd.as_raw_fd() }
    }

    /// Split into sender and receiver halves.
    ///
    /// Note: On Linux with eventfd, both halves share the same fd.
    /// The sender keeps a raw fd (doesn't own), receiver owns the fd.
    pub fn split(self) -> (DoorbellSender, DoorbellReceiver) {
        #[cfg(target_os = "linux")]
        {
            let raw = self.fd.as_raw_fd();
            (
                DoorbellSender { fd: raw },
                DoorbellReceiver { fd: self.fd },
            )
        }

        #[cfg(not(target_os = "linux"))]
        {
            (
                DoorbellSender { fd: self.write_fd },
                DoorbellReceiver { fd: self.read_fd },
            )
        }
    }
}

impl DoorbellSender {
    /// Ring the doorbell
    pub fn ring(&self) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            let val: u64 = 1;
            let result = unsafe {
                libc::write(self.fd, &val as *const u64 as *const libc::c_void, 8)
            };
            if result < 0 {
                let err = io::Error::last_os_error();
                if err.kind() != io::ErrorKind::WouldBlock {
                    return Err(err);
                }
            }
            Ok(())
        }

        #[cfg(not(target_os = "linux"))]
        {
            let val: [u8; 1] = [1];
            let result = unsafe {
                libc::write(self.fd.as_raw_fd(), val.as_ptr() as *const libc::c_void, 1)
            };
            if result < 0 {
                let err = io::Error::last_os_error();
                if err.kind() != io::ErrorKind::WouldBlock {
                    return Err(err);
                }
            }
            Ok(())
        }
    }
}

impl DoorbellReceiver {
    /// Try to consume pending rings (non-blocking)
    pub fn try_consume(&self) -> io::Result<u64> {
        #[cfg(target_os = "linux")]
        {
            let mut val: u64 = 0;
            let result = unsafe {
                libc::read(self.fd.as_raw_fd(), &mut val as *mut u64 as *mut libc::c_void, 8)
            };
            if result < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::WouldBlock {
                    return Ok(0);
                }
                return Err(err);
            }
            Ok(val)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let mut buf = [0u8; 64];
            let result = unsafe {
                libc::read(self.fd.as_raw_fd(), buf.as_mut_ptr() as *mut libc::c_void, buf.len())
            };
            if result < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::WouldBlock {
                    return Ok(0);
                }
                return Err(err);
            }
            Ok(result as u64)
        }
    }

    /// Get raw fd for polling
    pub fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_doorbell() {
        let db = Doorbell::new().unwrap();
        assert!(db.as_raw_fd() >= 0);
    }

    #[test]
    fn ring_and_consume() {
        let db = Doorbell::new().unwrap();

        // Nothing pending initially
        assert_eq!(db.try_consume().unwrap(), 0);

        // Ring it
        db.ring().unwrap();

        // Should have something now
        let count = db.try_consume().unwrap();
        assert!(count > 0);

        // Should be empty again
        assert_eq!(db.try_consume().unwrap(), 0);
    }

    #[test]
    fn multiple_rings_accumulate() {
        let db = Doorbell::new().unwrap();

        db.ring().unwrap();
        db.ring().unwrap();
        db.ring().unwrap();

        // On Linux eventfd, this accumulates to 3
        // On pipe fallback, we just get bytes
        let count = db.try_consume().unwrap();
        assert!(count >= 1); // At least one notification
    }

    #[test]
    fn split_works() {
        let db = Doorbell::new().unwrap();
        let (sender, receiver) = db.split();

        sender.ring().unwrap();
        let count = receiver.try_consume().unwrap();
        assert!(count > 0);
    }
}

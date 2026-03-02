//! Unix implementation using SCM_RIGHTS for fd passing over Unix domain sockets.

use passfd::FdPassingExt;
use std::io::{self, ErrorKind};
use std::os::fd::{AsRawFd, RawFd};
use tokio::io::Interest;
use tokio::net::UnixStream;

/// Send a file descriptor over a Unix stream.
///
/// The file descriptor remains valid in the sender after this call.
/// The receiver gets a new file descriptor pointing to the same underlying
/// kernel object.
pub async fn send_fd(stream: &UnixStream, fd: RawFd) -> io::Result<()> {
    loop {
        stream.writable().await?;
        match stream.try_io(Interest::WRITABLE, || stream.as_raw_fd().send_fd(fd)) {
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => continue,
            other => return other,
        }
    }
}

/// Receive a file descriptor from a Unix stream.
///
/// Returns a new file descriptor that the caller is responsible for closing.
pub async fn recv_fd(stream: &UnixStream) -> io::Result<RawFd> {
    loop {
        stream.readable().await?;
        match stream.try_io(Interest::READABLE, || stream.as_raw_fd().recv_fd()) {
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => continue,
            other => return other,
        }
    }
}

/// Send multiple file descriptors over a Unix stream in one SCM_RIGHTS message.
pub async fn send_fds(stream: &UnixStream, fds: &[RawFd]) -> io::Result<()> {
    if fds.is_empty() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "send_fds requires at least one fd",
        ));
    }

    loop {
        stream.writable().await?;
        match stream.try_io(Interest::WRITABLE, || send_fds_now(stream.as_raw_fd(), fds)) {
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => continue,
            other => return other,
        }
    }
}

/// Receive exactly `expected` fds from one SCM_RIGHTS message.
pub async fn recv_fds(stream: &UnixStream, expected: usize) -> io::Result<Vec<RawFd>> {
    if expected == 0 {
        return Ok(Vec::new());
    }

    loop {
        stream.readable().await?;
        match stream.try_io(Interest::READABLE, || {
            recv_fds_now(stream.as_raw_fd(), expected)
        }) {
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => continue,
            other => return other,
        }
    }
}

fn send_fds_now(sock_fd: RawFd, fds: &[RawFd]) -> io::Result<()> {
    let mut payload = [0xA5u8; 1];
    let mut iov = libc::iovec {
        iov_base: payload.as_mut_ptr().cast(),
        iov_len: payload.len(),
    };

    let data_len = std::mem::size_of_val(fds);
    // SAFETY: libc macro-like function with valid integer argument.
    let cmsg_space = unsafe { libc::CMSG_SPACE(data_len as u32) as usize };
    let mut control = vec![0u8; cmsg_space];

    // SAFETY: We initialize all fields before use.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr().cast();
    msg.msg_controllen = control.len() as _;

    // SAFETY: msg_control points to a valid writable buffer.
    let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msg) };
    if cmsg.is_null() {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "failed to build cmsg header",
        ));
    }

    // SAFETY: cmsg is valid for writes in the control buffer.
    unsafe {
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(data_len as u32) as _;
        let data_ptr = libc::CMSG_DATA(cmsg).cast::<RawFd>();
        std::ptr::copy_nonoverlapping(fds.as_ptr(), data_ptr, fds.len());
    }

    // SAFETY: msg references valid iov/control buffers for the duration of call.
    let n = unsafe { libc::sendmsg(sock_fd, &msg, 0) };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    if n == 0 {
        return Err(io::Error::new(
            ErrorKind::WriteZero,
            "sendmsg wrote 0 bytes",
        ));
    }
    Ok(())
}

fn recv_fds_now(sock_fd: RawFd, expected: usize) -> io::Result<Vec<RawFd>> {
    let mut payload = [0u8; 256];
    let mut iov = libc::iovec {
        iov_base: payload.as_mut_ptr().cast(),
        iov_len: payload.len(),
    };

    let max_expected = expected.max(4);
    let data_len = max_expected * std::mem::size_of::<RawFd>();
    // SAFETY: libc macro-like function with valid integer argument.
    let cmsg_space = unsafe { libc::CMSG_SPACE(data_len as u32) as usize };
    let mut control = vec![0u8; cmsg_space];

    // SAFETY: We initialize all fields before use.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr().cast();
    msg.msg_controllen = control.len() as _;

    // SAFETY: msg references valid iov/control buffers for the duration of call.
    let n = unsafe { libc::recvmsg(sock_fd, &mut msg, 0) };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    if n == 0 {
        return Err(io::Error::new(ErrorKind::UnexpectedEof, "early eof"));
    }
    if (msg.msg_flags & libc::MSG_CTRUNC) != 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "control message truncated",
        ));
    }

    let mut out = Vec::with_capacity(expected);
    // SAFETY: iterating cmsg headers as provided by kernel in `msg`.
    unsafe {
        let mut cmsg = libc::CMSG_FIRSTHDR(&msg);
        while !cmsg.is_null() {
            if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                let cmsg_len = (*cmsg).cmsg_len as usize;
                let base_len = libc::CMSG_LEN(0) as usize;
                if cmsg_len >= base_len {
                    let bytes = cmsg_len - base_len;
                    let count = bytes / std::mem::size_of::<RawFd>();
                    let data_ptr = libc::CMSG_DATA(cmsg).cast::<RawFd>();
                    for i in 0..count {
                        out.push(*data_ptr.add(i));
                    }
                }
            }
            cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
        }
    }

    if out.len() < expected {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("expected {expected} fds, received {}", out.len()),
        ));
    }
    out.truncate(expected);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::FromRawFd;
    use std::os::fd::IntoRawFd;
    use std::os::unix::net::UnixStream as StdUnixStream;

    #[tokio::test]
    async fn send_fd_does_not_close_sender_fd() {
        let (a_std, b_std) = StdUnixStream::pair().expect("unix pair");
        a_std.set_nonblocking(true).expect("nonblocking");
        b_std.set_nonblocking(true).expect("nonblocking");

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("tcp bind");
        let fd = listener.into_raw_fd();

        let a = UnixStream::from_std(a_std).expect("tokio unix stream");
        let b = UnixStream::from_std(b_std).expect("tokio unix stream");

        send_fd(&a, fd).await.expect("send fd");
        let received_fd = recv_fd(&b).await.expect("recv fd");

        // If the sender FD got closed, fcntl(F_GETFD) will return -1 with EBADF.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        assert_ne!(flags, -1, "sender fd unexpectedly closed");

        unsafe {
            libc::close(fd);
            libc::close(received_fd);
        }
    }

    #[tokio::test]
    async fn roundtrip_tcp_listener() {
        let (a_std, b_std) = StdUnixStream::pair().expect("unix pair");
        a_std.set_nonblocking(true).expect("nonblocking");
        b_std.set_nonblocking(true).expect("nonblocking");

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("tcp bind");
        let addr = listener.local_addr().expect("local addr");
        let fd = listener.into_raw_fd();

        let a = UnixStream::from_std(a_std).expect("tokio unix stream");
        let b = UnixStream::from_std(b_std).expect("tokio unix stream");

        send_fd(&a, fd).await.expect("send fd");
        let received_fd = recv_fd(&b).await.expect("recv fd");

        // Recreate the listener from the received fd
        let received_listener = unsafe { std::net::TcpListener::from_raw_fd(received_fd) };
        let received_addr = received_listener.local_addr().expect("received local addr");

        assert_eq!(addr, received_addr);

        // Cleanup - close original fd (received_fd is owned by received_listener now)
        unsafe { libc::close(fd) };
    }

    #[tokio::test]
    async fn send_fds_rejects_empty_slice() {
        let (a_std, _b_std) = StdUnixStream::pair().expect("unix pair");
        a_std.set_nonblocking(true).expect("nonblocking");
        let a = UnixStream::from_std(a_std).expect("tokio unix stream");

        let err = send_fds(&a, &[])
            .await
            .expect_err("empty fd list must fail");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[tokio::test]
    async fn recv_fds_expected_zero_returns_empty_without_io() {
        let (a_std, _b_std) = StdUnixStream::pair().expect("unix pair");
        a_std.set_nonblocking(true).expect("nonblocking");
        let a = UnixStream::from_std(a_std).expect("tokio unix stream");

        let fds = recv_fds(&a, 0).await.expect("expected=0 should succeed");
        assert!(fds.is_empty());
    }

    #[tokio::test]
    async fn recv_fds_reports_count_mismatch_when_too_few_fds_received() {
        let (a_std, b_std) = StdUnixStream::pair().expect("unix pair");
        a_std.set_nonblocking(true).expect("nonblocking");
        b_std.set_nonblocking(true).expect("nonblocking");

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("tcp bind");
        let fd = listener.into_raw_fd();

        let a = UnixStream::from_std(a_std).expect("tokio unix stream");
        let b = UnixStream::from_std(b_std).expect("tokio unix stream");

        send_fds(&a, &[fd]).await.expect("send single fd");
        let err = recv_fds(&b, 2)
            .await
            .expect_err("expecting more fds should fail");
        assert_eq!(err.kind(), ErrorKind::InvalidData);

        unsafe { libc::close(fd) };
    }

    #[tokio::test]
    async fn recv_fd_reports_unexpected_eof_when_peer_closed_without_sending() {
        let (a_std, b_std) = StdUnixStream::pair().expect("unix pair");
        a_std.set_nonblocking(true).expect("nonblocking");
        b_std.set_nonblocking(true).expect("nonblocking");

        let _a = UnixStream::from_std(a_std).expect("tokio unix stream");
        let b = UnixStream::from_std(b_std).expect("tokio unix stream");

        drop(_a);
        let err = recv_fd(&b)
            .await
            .expect_err("closed peer should produce eof");
        assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn roundtrip_multiple_fds() {
        let (a_std, b_std) = StdUnixStream::pair().expect("unix pair");
        a_std.set_nonblocking(true).expect("nonblocking");
        b_std.set_nonblocking(true).expect("nonblocking");

        let listener1 = std::net::TcpListener::bind("127.0.0.1:0").expect("tcp bind 1");
        let listener2 = std::net::TcpListener::bind("127.0.0.1:0").expect("tcp bind 2");
        let addr1 = listener1.local_addr().expect("local addr1");
        let addr2 = listener2.local_addr().expect("local addr2");
        let fd1 = listener1.into_raw_fd();
        let fd2 = listener2.into_raw_fd();

        let a = UnixStream::from_std(a_std).expect("tokio unix stream");
        let b = UnixStream::from_std(b_std).expect("tokio unix stream");

        send_fds(&a, &[fd1, fd2]).await.expect("send fds");
        let received = recv_fds(&b, 2).await.expect("recv fds");
        assert_eq!(received.len(), 2);

        let l1 = unsafe { std::net::TcpListener::from_raw_fd(received[0]) };
        let l2 = unsafe { std::net::TcpListener::from_raw_fd(received[1]) };

        let got = [
            l1.local_addr().expect("recv addr1"),
            l2.local_addr().expect("recv addr2"),
        ];
        let expected = [addr1, addr2];
        assert_eq!(got, expected);

        // Original sender FDs should still be valid.
        let flags1 = unsafe { libc::fcntl(fd1, libc::F_GETFD) };
        let flags2 = unsafe { libc::fcntl(fd2, libc::F_GETFD) };
        assert_ne!(flags1, -1, "fd1 unexpectedly closed on sender");
        assert_ne!(flags2, -1, "fd2 unexpectedly closed on sender");

        unsafe {
            libc::close(fd1);
            libc::close(fd2);
        }
    }
}

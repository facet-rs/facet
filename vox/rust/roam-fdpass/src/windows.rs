//! Windows implementation using WSADuplicateSocket for socket handle passing.
//!
//! On Windows, you cannot directly pass handles over a pipe like Unix does with SCM_RIGHTS.
//! Instead, we use WSADuplicateSocket to serialize socket state into a WSAPROTOCOL_INFOW
//! structure, send that over a pipe, and the receiver uses WSASocketW to recreate the socket.
//!
//! The sender must know the receiver's process ID for this to work.

use std::io::{self, Read, Write};
use std::mem::{self, MaybeUninit};
use std::net::TcpListener;
use std::os::windows::io::{AsRawSocket, FromRawSocket, RawSocket};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use windows_sys::Win32::Networking::WinSock::{
    WSADuplicateSocketW, WSAGetLastError, WSASocketW, INVALID_SOCKET, SOCKET, WSAPROTOCOL_INFOW,
    WSA_FLAG_OVERLAPPED,
};

/// Size of the WSAPROTOCOL_INFOW structure.
const PROTOCOL_INFO_SIZE: usize = mem::size_of::<WSAPROTOCOL_INFOW>();

/// Send a TCP listener to another process over an async stream.
///
/// The receiver must call `recv_tcp_listener` with their end of the stream.
///
/// # Arguments
///
/// * `stream` - An async stream (e.g., named pipe) to send over
/// * `listener` - The TCP listener to send
/// * `target_pid` - The process ID of the receiving process
///
/// # Safety
///
/// The `target_pid` must be the actual process ID of the receiver. Using an incorrect
/// PID will result in the receiver being unable to recreate the socket.
pub async fn send_tcp_listener<S>(
    stream: &mut S,
    listener: &TcpListener,
    target_pid: u32,
) -> io::Result<()>
where
    S: AsyncWriteExt + Unpin,
{
    let socket = listener.as_raw_socket() as SOCKET;
    let protocol_info = duplicate_socket(socket, target_pid)?;

    // Send the protocol info as raw bytes
    let bytes =
        unsafe { std::slice::from_raw_parts(&protocol_info as *const _ as *const u8, PROTOCOL_INFO_SIZE) };
    stream.write_all(bytes).await?;

    Ok(())
}

/// Receive a TCP listener from another process over an async stream.
///
/// The sender must have called `send_tcp_listener` with this process's PID.
pub async fn recv_tcp_listener<S>(stream: &mut S) -> io::Result<TcpListener>
where
    S: AsyncReadExt + Unpin,
{
    // Read the protocol info
    let mut buf = [0u8; PROTOCOL_INFO_SIZE];
    stream.read_exact(&mut buf).await?;

    let protocol_info: WSAPROTOCOL_INFOW = unsafe { std::ptr::read(buf.as_ptr() as *const _) };

    // Recreate the socket
    let socket = create_socket_from_info(&protocol_info)?;

    // Wrap in TcpListener
    Ok(unsafe { TcpListener::from_raw_socket(socket as RawSocket) })
}

/// Send a TCP listener to another process over a synchronous stream.
///
/// Synchronous version for use outside of async contexts.
pub fn send_tcp_listener_sync<S>(
    stream: &mut S,
    listener: &TcpListener,
    target_pid: u32,
) -> io::Result<()>
where
    S: Write,
{
    let socket = listener.as_raw_socket() as SOCKET;
    let protocol_info = duplicate_socket(socket, target_pid)?;

    let bytes =
        unsafe { std::slice::from_raw_parts(&protocol_info as *const _ as *const u8, PROTOCOL_INFO_SIZE) };
    stream.write_all(bytes)?;

    Ok(())
}

/// Receive a TCP listener from another process over a synchronous stream.
///
/// Synchronous version for use outside of async contexts.
pub fn recv_tcp_listener_sync<S>(stream: &mut S) -> io::Result<TcpListener>
where
    S: Read,
{
    let mut buf = [0u8; PROTOCOL_INFO_SIZE];
    stream.read_exact(&mut buf)?;

    let protocol_info: WSAPROTOCOL_INFOW = unsafe { std::ptr::read(buf.as_ptr() as *const _) };
    let socket = create_socket_from_info(&protocol_info)?;

    Ok(unsafe { TcpListener::from_raw_socket(socket as RawSocket) })
}

/// Duplicate a socket for use in another process.
fn duplicate_socket(socket: SOCKET, target_pid: u32) -> io::Result<WSAPROTOCOL_INFOW> {
    let mut protocol_info: MaybeUninit<WSAPROTOCOL_INFOW> = MaybeUninit::uninit();

    let result =
        unsafe { WSADuplicateSocketW(socket, target_pid, protocol_info.as_mut_ptr()) };

    if result != 0 {
        let err = unsafe { WSAGetLastError() };
        return Err(io::Error::from_raw_os_error(err));
    }

    Ok(unsafe { protocol_info.assume_init() })
}

/// Create a socket from protocol info received from another process.
fn create_socket_from_info(protocol_info: &WSAPROTOCOL_INFOW) -> io::Result<SOCKET> {
    let socket = unsafe {
        WSASocketW(
            protocol_info.iAddressFamily,
            protocol_info.iSocketType,
            protocol_info.iProtocol,
            protocol_info as *const _ as *mut _,
            0,
            WSA_FLAG_OVERLAPPED,
        )
    };

    if socket == INVALID_SOCKET {
        let err = unsafe { WSAGetLastError() };
        return Err(io::Error::from_raw_os_error(err));
    }

    Ok(socket)
}

/// Get the current process ID.
///
/// Useful for the receiver to communicate their PID to the sender.
pub fn current_pid() -> u32 {
    unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_protocol_info_size() {
        // Sanity check - WSAPROTOCOL_INFOW should be a fixed size
        assert!(PROTOCOL_INFO_SIZE > 0);
        assert!(PROTOCOL_INFO_SIZE < 1024); // Should be around 628 bytes
    }

    #[test]
    fn test_current_pid() {
        let pid = current_pid();
        assert!(pid > 0);
    }

    #[test]
    fn test_roundtrip_same_process() {
        // Create a TCP listener
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("local_addr");

        // Use an in-memory buffer for testing
        let mut buffer = Vec::new();

        // Send the listener to ourselves
        let pid = current_pid();
        send_tcp_listener_sync(&mut buffer, &listener, pid).expect("send");

        // Receive it
        let mut cursor = Cursor::new(buffer);
        let received = recv_tcp_listener_sync(&mut cursor).expect("recv");
        let received_addr = received.local_addr().expect("received local_addr");

        assert_eq!(addr, received_addr);
    }
}

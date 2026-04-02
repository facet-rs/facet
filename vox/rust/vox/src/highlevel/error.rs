use vox_core::SessionError;

/// Error returned by [`super::serve()`].
#[derive(Debug)]
pub enum ServeError {
    /// I/O error (bind failure, etc.).
    Io(std::io::Error),
    /// Another healthy process is already serving on this address.
    AddrInUse { addr: String },
    /// Another process holds the lock but is not responding to connections.
    LockHeldUnhealthy { addr: String },
    /// Unknown or unsupported transport scheme.
    UnsupportedScheme { scheme: String },
    /// Session-level error from the accept loop.
    Session(SessionError),
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::AddrInUse { addr } => {
                write!(f, "another healthy process is already serving on {addr}")
            }
            Self::LockHeldUnhealthy { addr } => write!(
                f,
                "another process holds the lock on {addr} but is not responding"
            ),
            Self::UnsupportedScheme { scheme } => {
                write!(f, "unsupported transport scheme: {scheme:?}")
            }
            Self::Session(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ServeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Session(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ServeError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<SessionError> for ServeError {
    fn from(e: SessionError) -> Self {
        Self::Session(e)
    }
}

use facet::Facet;
use std::fmt;

// r[rpc.fallible.vox-error]
/// Protocol-level error wrapper distinguishing application errors from vox infrastructure errors.
///
/// On the caller side, all return types are wrapped as `Result<T, VoxError<E>>`:
///   * Infallible `fn foo() -> T` becomes `Result<T, VoxError>`
///   * Fallible `fn foo() -> Result<T, E>` becomes `Result<T, VoxError<E>>`
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum VoxError<E = ::core::convert::Infallible> {
    /// The handler ran and returned an application error.
    User(Box<E>),

    /// No handler recognized the method ID.
    UnknownMethod,

    /// The arguments could not be deserialized.
    InvalidPayload(String),

    /// The call was cancelled before completion (e.g. handler dropped without replying).
    Cancelled,

    /// The underlying connection closed while the call was in flight.
    ConnectionClosed,

    /// The connection driver shut down while the call was in flight.
    ConnectionShutdown,

    /// The call could not be sent because the transport is dead.
    SendFailed,

    /// The call made no request-scoped progress within its idle timeout.
    TimedOut,

    /// The runtime refused to guess after recovery.
    Indeterminate,
}

impl<E> VoxError<E> {
    // r[impl rpc.fallible.vox-error.outcome]
    // r[impl schema.errors.same-peer-terminal]
    /// Returns `true` when the call ended because the connection or transport died.
    ///
    /// `InvalidPayload`, `UnknownMethod`, `User`, and `Cancelled` are terminal
    /// call outcomes. `Indeterminate` is separate: the runtime explicitly does
    /// not know whether the call reached a terminal outcome.
    pub fn is_connection_interruption(&self) -> bool {
        matches!(
            self,
            Self::ConnectionClosed | Self::ConnectionShutdown | Self::SendFailed
        )
    }
}

impl<E: fmt::Display> fmt::Display for VoxError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User(error) => write!(f, "{error}"),
            Self::UnknownMethod => f.write_str("unknown vox method call"),
            Self::InvalidPayload(message) => write!(f, "invalid vox payload: {message}"),
            Self::Cancelled => f.write_str("vox request cancelled"),
            Self::ConnectionClosed => f.write_str("vox connection closed"),
            Self::ConnectionShutdown => f.write_str("vox connection shutdown"),
            Self::SendFailed => f.write_str("vox send failed"),
            Self::TimedOut => f.write_str("vox request timed out"),
            Self::Indeterminate => f.write_str("indeterminate vox error"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for VoxError<E> {}

use std::convert::Infallible;

use facet::Facet;
use roam_frame::OwnedMessage;

#[derive(Debug)]
pub enum ClientError<TransportError> {
    Transport(TransportError),
    Encode(facet_postcard::SerializeError),
    Decode(facet_postcard::DeserializeError<facet_postcard::PostcardError>),
}

impl<TransportError> From<TransportError> for ClientError<TransportError> {
    fn from(value: TransportError) -> Self {
        Self::Transport(value)
    }
}

#[derive(Debug)]
pub enum DispatchError {
    Encode(facet_postcard::SerializeError),
}

/// Error when routing stream data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelError {
    /// Stream ID not found in registry.
    Unknown,
    /// Data received after stream was closed.
    DataAfterClose,
    /// r[impl flow.channel.credit-overrun] - Data exceeded remaining credit.
    CreditOverrun,
}

/// Call error type encoded in RPC responses.
///
/// r[impl core.error.roam-error] - Wraps call results to distinguish app vs protocol errors
/// r[impl call.response.encoding] - Response is `Result<T, RoamError<E>>`
/// r[impl call.error.roam-error] - Protocol errors use RoamError variants
/// r[impl call.error.protocol] - Discriminants 1-3 are protocol-level errors
///
/// Spec: `docs/content/spec/_index.md` "RoamError".
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum RoamError<E> {
    /// r[impl core.error.call-vs-connection] - User errors affect only this call
    /// r[impl call.error.user] - User(E) carries the application's error type
    User(E) = 0,
    /// r[impl call.error.unknown-method] - Method ID not recognized
    UnknownMethod = 1,
    /// r[impl call.error.invalid-payload] - Request payload deserialization failed
    InvalidPayload = 2,
    Cancelled = 3,
}

impl<E> RoamError<E> {
    /// Map the user error type to a different type.
    pub fn map_user<F, E2>(self, f: F) -> RoamError<E2>
    where
        F: FnOnce(E) -> E2,
    {
        match self {
            RoamError::User(e) => RoamError::User(f(e)),
            RoamError::UnknownMethod => RoamError::UnknownMethod,
            RoamError::InvalidPayload => RoamError::InvalidPayload,
            RoamError::Cancelled => RoamError::Cancelled,
        }
    }
}

pub type CallResult<T, E> = ::core::result::Result<T, RoamError<E>>;
pub type BorrowedCallResult<T, E> = OwnedMessage<CallResult<T, E>>;

/// Error from making an outgoing call.
///
/// This flattens the nested `Result<Result<T, RoamError<E>>, CallError>` pattern
/// into a single `Result<T, CallError<E>>` for better ergonomics.
///
/// The type parameter `E` represents the user's error type from fallible methods.
/// For infallible methods, use `CallError<Infallible>`.
#[derive(Debug)]
pub enum CallError<E = Infallible> {
    /// The remote returned a roam-level error (user error or protocol error).
    Roam(RoamError<E>),
    /// Failed to encode request payload.
    Encode(facet_postcard::SerializeError),
    /// Failed to decode response payload.
    Decode(facet_postcard::DeserializeError<facet_postcard::PostcardError>),
    /// Protocol-level decode error (malformed response structure).
    Protocol(DecodeError),
    /// Connection was closed before response.
    ConnectionClosed,
    /// Driver task is gone.
    DriverGone,
}

impl<E> CallError<E> {
    /// Map the user error type to a different type.
    pub fn map_user<F, E2>(self, f: F) -> CallError<E2>
    where
        F: FnOnce(E) -> E2,
    {
        match self {
            CallError::Roam(roam_err) => CallError::Roam(roam_err.map_user(f)),
            CallError::Encode(e) => CallError::Encode(e),
            CallError::Decode(e) => CallError::Decode(e),
            CallError::Protocol(e) => CallError::Protocol(e),
            CallError::ConnectionClosed => CallError::ConnectionClosed,
            CallError::DriverGone => CallError::DriverGone,
        }
    }
}

impl<E: std::fmt::Debug> std::fmt::Display for CallError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Roam(e) => write!(f, "roam error: {e:?}"),
            CallError::Encode(e) => write!(f, "encode error: {e}"),
            CallError::Decode(e) => write!(f, "decode error: {e}"),
            CallError::Protocol(e) => write!(f, "protocol error: {e}"),
            CallError::ConnectionClosed => write!(f, "connection closed"),
            CallError::DriverGone => write!(f, "driver task stopped"),
        }
    }
}

impl<E: std::fmt::Debug> std::error::Error for CallError<E> {}

/// Transport-level call error (no user error type).
///
/// Used by the `Caller` trait which operates at the transport level
/// before response decoding.
#[derive(Debug)]
pub enum TransportError {
    /// Failed to encode request payload.
    Encode(facet_postcard::SerializeError),
    /// Connection was closed before response.
    ConnectionClosed,
    /// Driver task is gone.
    DriverGone,
}

impl<E> From<TransportError> for CallError<E> {
    fn from(e: TransportError) -> Self {
        match e {
            TransportError::Encode(e) => CallError::Encode(e),
            TransportError::ConnectionClosed => CallError::ConnectionClosed,
            TransportError::DriverGone => CallError::DriverGone,
        }
    }
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Encode(e) => write!(f, "encode error: {e}"),
            TransportError::ConnectionClosed => write!(f, "connection closed"),
            TransportError::DriverGone => write!(f, "driver task stopped"),
        }
    }
}

impl std::error::Error for TransportError {}

/// Error decoding a response payload.
#[derive(Debug)]
pub enum DecodeError {
    /// Empty response payload.
    EmptyPayload,
    /// Truncated error response.
    TruncatedError,
    /// Unknown RoamError discriminant.
    UnknownRoamErrorDiscriminant(u8),
    /// Invalid Result discriminant.
    InvalidResultDiscriminant(u8),
    /// Postcard deserialization error.
    Postcard(facet_postcard::DeserializeError<facet_postcard::PostcardError>),
    /// Deserialization failed with a message.
    DeserializeFailed(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::EmptyPayload => write!(f, "empty response payload"),
            DecodeError::TruncatedError => write!(f, "truncated error response"),
            DecodeError::UnknownRoamErrorDiscriminant(d) => {
                write!(f, "unknown RoamError discriminant: {d}")
            }
            DecodeError::InvalidResultDiscriminant(d) => {
                write!(f, "invalid Result discriminant: {d}")
            }
            DecodeError::Postcard(e) => write!(f, "postcard: {e}"),
            DecodeError::DeserializeFailed(msg) => write!(f, "deserialize failed: {msg}"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl<E> From<DecodeError> for CallError<E> {
    fn from(e: DecodeError) -> Self {
        match e {
            DecodeError::Postcard(pe) => CallError::Decode(pe),
            other => CallError::Protocol(other),
        }
    }
}

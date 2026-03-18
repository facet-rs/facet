use facet::Facet;

// r[rpc.fallible.roam-error]
/// Protocol-level error wrapper distinguishing application errors from roam infrastructure errors.
///
/// On the caller side, all return types are wrapped as `Result<T, RoamError<E>>`:
///   * Infallible `fn foo() -> T` becomes `Result<T, RoamError>`
///   * Fallible `fn foo() -> Result<T, E>` becomes `Result<T, RoamError<E>>`
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum RoamError<E = ::core::convert::Infallible> {
    /// The handler ran and returned an application error.
    User(E),

    /// No handler recognized the method ID.
    UnknownMethod,

    /// The arguments could not be deserialized.
    InvalidPayload(String),

    /// The call was cancelled before completion (e.g. handler dropped without replying).
    Cancelled,

    /// The underlying connection closed while the call was in flight.
    ConnectionClosed,

    /// The session shut down while the call was in flight.
    SessionShutdown,

    /// The call could not be sent because the transport is dead.
    SendFailed,

    /// The runtime refused to guess after recovery.
    Indeterminate,
}

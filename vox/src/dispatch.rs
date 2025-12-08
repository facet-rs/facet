// src/dispatch.rs

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Control channel method IDs from the DESIGN.md specification.
///
/// These methods operate on channel 0 (the control channel) and handle
/// session-level operations like opening/closing channels, flow control,
/// liveness probes, and introspection.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ControlMethod {
    /// Invalid method ID (reserved)
    Reserved = 0,

    /// Open a new data channel
    OpenChannel = 1,

    /// Close a channel gracefully
    CloseChannel = 2,

    /// Cancel a channel (advisory)
    CancelChannel = 3,

    /// Grant flow control credits
    GrantCredits = 4,

    /// Liveness check
    Ping = 5,

    /// Response to PING
    Pong = 6,

    /// Introspection: list services
    ListServices = 7,

    /// Introspection: get service info
    GetService = 8,

    /// Introspection: get method info
    GetMethod = 9,

    /// Introspection: get schema
    GetSchema = 10,

    /// Change debug level for channel
    SetDebugLevel = 11,

    /// Fault injection control
    InjectFault = 12,
}

impl ControlMethod {
    /// Convert from a u32 wire value.
    /// Returns None if the value doesn't match a known control method.
    pub fn from_u32(val: u32) -> Option<Self> {
        Some(match val {
            0 => ControlMethod::Reserved,
            1 => ControlMethod::OpenChannel,
            2 => ControlMethod::CloseChannel,
            3 => ControlMethod::CancelChannel,
            4 => ControlMethod::GrantCredits,
            5 => ControlMethod::Ping,
            6 => ControlMethod::Pong,
            7 => ControlMethod::ListServices,
            8 => ControlMethod::GetService,
            9 => ControlMethod::GetMethod,
            10 => ControlMethod::GetSchema,
            11 => ControlMethod::SetDebugLevel,
            12 => ControlMethod::InjectFault,
            _ => return None,
        })
    }

    /// Convert to u32 for wire transmission.
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    /// Check if this is a valid control method (not Reserved).
    pub fn is_valid(self) -> bool {
        self != ControlMethod::Reserved
    }

    /// Get a human-readable description of this control method.
    pub fn description(self) -> &'static str {
        match self {
            ControlMethod::Reserved => "reserved (invalid)",
            ControlMethod::OpenChannel => "open a new data channel",
            ControlMethod::CloseChannel => "close a channel gracefully",
            ControlMethod::CancelChannel => "cancel a channel (advisory)",
            ControlMethod::GrantCredits => "grant flow control credits",
            ControlMethod::Ping => "liveness check",
            ControlMethod::Pong => "response to ping",
            ControlMethod::ListServices => "list services",
            ControlMethod::GetService => "get service info",
            ControlMethod::GetMethod => "get method info",
            ControlMethod::GetSchema => "get schema",
            ControlMethod::SetDebugLevel => "set debug level",
            ControlMethod::InjectFault => "fault injection control",
        }
    }
}

impl TryFrom<u32> for ControlMethod {
    type Error = UnknownMethodId;

    fn try_from(val: u32) -> Result<Self, Self::Error> {
        ControlMethod::from_u32(val).ok_or(UnknownMethodId(val))
    }
}

impl From<ControlMethod> for u32 {
    fn from(method: ControlMethod) -> u32 {
        method.as_u32()
    }
}

impl fmt::Display for ControlMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.description(), self.as_u32())
    }
}

/// Error when converting from an unknown method ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownMethodId(pub u32);

impl fmt::Display for UnknownMethodId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown method ID: {}", self.0)
    }
}

impl std::error::Error for UnknownMethodId {}

/// User method IDs start at this offset to avoid conflicts with control methods.
///
/// Control methods use IDs 0-99, user methods start at 1000.
pub const USER_METHOD_ID_START: u32 = 1000;

/// Maximum method ID value (inclusive).
/// This provides a reasonable upper bound for validation.
pub const MAX_METHOD_ID: u32 = u32::MAX;

/// Check if a method ID is in the user method range.
pub fn is_user_method(method_id: u32) -> bool {
    method_id >= USER_METHOD_ID_START
}

/// Check if a method ID is in the control method range.
pub fn is_control_method(method_id: u32) -> bool {
    method_id <= 12
}

/// Handler trait for method dispatch.
///
/// Handlers are registered with a MethodDispatcher and invoked when
/// matching method IDs are received. This is a foundational trait that
/// will be extended with async dispatch capabilities in future phases.
pub trait Handler: Send + Sync {
    /// Get the method ID this handler services.
    fn method_id(&self) -> u32;
}

/// Errors that can occur during method dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchError {
    /// Method ID has no registered handler
    UnknownMethod(u32),

    /// Method ID is already registered
    MethodAlreadyRegistered(u32),

    /// Method ID is in reserved range but not a valid control method
    InvalidControlMethod(u32),

    /// Method ID is out of valid range
    MethodIdOutOfRange(u32),
}

impl fmt::Display for DispatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DispatchError::UnknownMethod(id) => {
                write!(f, "unknown method ID: {}", id)
            }
            DispatchError::MethodAlreadyRegistered(id) => {
                write!(f, "method ID {} is already registered", id)
            }
            DispatchError::InvalidControlMethod(id) => {
                write!(f, "method ID {} is in reserved control range but invalid", id)
            }
            DispatchError::MethodIdOutOfRange(id) => {
                write!(f, "method ID {} is out of valid range", id)
            }
        }
    }
}

impl std::error::Error for DispatchError {}

/// Method dispatcher for routing messages to handlers.
///
/// The dispatcher maintains a registry of handlers keyed by method ID.
/// It supports both control methods (built-in) and user methods (application-defined).
///
/// # Thread Safety
///
/// MethodDispatcher is Send + Sync and can be shared across threads.
/// Handlers are stored as Arc<dyn Handler> to enable safe concurrent access.
///
/// # Example
///
/// ```no_run
/// use rapace::dispatch::{MethodDispatcher, Handler};
///
/// struct MyHandler;
/// impl Handler for MyHandler {
///     fn method_id(&self) -> u32 { 1000 }
/// }
///
/// let mut dispatcher = MethodDispatcher::new();
/// dispatcher.register(MyHandler).unwrap();
///
/// let handler = dispatcher.lookup(1000).unwrap();
/// assert_eq!(handler.method_id(), 1000);
/// ```
pub struct MethodDispatcher {
    handlers: HashMap<u32, Arc<dyn Handler>>,
}

impl MethodDispatcher {
    /// Create a new empty method dispatcher.
    pub fn new() -> Self {
        MethodDispatcher {
            handlers: HashMap::new(),
        }
    }

    /// Register a handler for its associated method ID.
    ///
    /// Returns an error if:
    /// - The method ID is already registered
    /// - The method ID is invalid (e.g., Reserved control method)
    pub fn register(&mut self, handler: impl Handler + 'static) -> Result<(), DispatchError> {
        let method_id = handler.method_id();

        // Validate method ID
        if method_id == ControlMethod::Reserved as u32 {
            return Err(DispatchError::InvalidControlMethod(method_id));
        }

        // Check for conflicts
        if self.handlers.contains_key(&method_id) {
            return Err(DispatchError::MethodAlreadyRegistered(method_id));
        }

        self.handlers.insert(method_id, Arc::new(handler));
        Ok(())
    }

    /// Register a pre-wrapped Arc handler.
    ///
    /// This is useful when you need to share the same handler instance
    /// across multiple dispatchers or keep a reference to it.
    pub fn register_arc(&mut self, handler: Arc<dyn Handler>) -> Result<(), DispatchError> {
        let method_id = handler.method_id();

        if method_id == ControlMethod::Reserved as u32 {
            return Err(DispatchError::InvalidControlMethod(method_id));
        }

        if self.handlers.contains_key(&method_id) {
            return Err(DispatchError::MethodAlreadyRegistered(method_id));
        }

        self.handlers.insert(method_id, handler);
        Ok(())
    }

    /// Look up a handler by method ID.
    ///
    /// Returns None if no handler is registered for this method ID.
    pub fn lookup(&self, method_id: u32) -> Option<&Arc<dyn Handler>> {
        self.handlers.get(&method_id)
    }

    /// Check if a handler is registered for the given method ID.
    pub fn has_handler(&self, method_id: u32) -> bool {
        self.handlers.contains_key(&method_id)
    }

    /// Unregister a handler by method ID.
    ///
    /// Returns true if a handler was removed, false if no handler was registered.
    pub fn unregister(&mut self, method_id: u32) -> bool {
        self.handlers.remove(&method_id).is_some()
    }

    /// Get the number of registered handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// Get an iterator over all registered method IDs.
    pub fn method_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.handlers.keys().copied()
    }

    /// Clear all registered handlers.
    pub fn clear(&mut self) {
        self.handlers.clear();
    }
}

impl Default for MethodDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_method_roundtrip() {
        let methods = [
            ControlMethod::Reserved,
            ControlMethod::OpenChannel,
            ControlMethod::CloseChannel,
            ControlMethod::CancelChannel,
            ControlMethod::GrantCredits,
            ControlMethod::Ping,
            ControlMethod::Pong,
            ControlMethod::ListServices,
            ControlMethod::GetService,
            ControlMethod::GetMethod,
            ControlMethod::GetSchema,
            ControlMethod::SetDebugLevel,
            ControlMethod::InjectFault,
        ];

        for &method in &methods {
            let val = method.as_u32();
            let roundtrip = ControlMethod::from_u32(val).unwrap();
            assert_eq!(method, roundtrip);
        }
    }

    #[test]
    fn control_method_try_from() {
        assert_eq!(ControlMethod::try_from(0).unwrap(), ControlMethod::Reserved);
        assert_eq!(ControlMethod::try_from(1).unwrap(), ControlMethod::OpenChannel);
        assert_eq!(ControlMethod::try_from(5).unwrap(), ControlMethod::Ping);
        assert_eq!(ControlMethod::try_from(12).unwrap(), ControlMethod::InjectFault);

        assert_eq!(ControlMethod::try_from(999), Err(UnknownMethodId(999)));
    }

    #[test]
    fn control_method_values_match_spec() {
        assert_eq!(ControlMethod::Reserved as u32, 0);
        assert_eq!(ControlMethod::OpenChannel as u32, 1);
        assert_eq!(ControlMethod::CloseChannel as u32, 2);
        assert_eq!(ControlMethod::CancelChannel as u32, 3);
        assert_eq!(ControlMethod::GrantCredits as u32, 4);
        assert_eq!(ControlMethod::Ping as u32, 5);
        assert_eq!(ControlMethod::Pong as u32, 6);
        assert_eq!(ControlMethod::ListServices as u32, 7);
        assert_eq!(ControlMethod::GetService as u32, 8);
        assert_eq!(ControlMethod::GetMethod as u32, 9);
        assert_eq!(ControlMethod::GetSchema as u32, 10);
        assert_eq!(ControlMethod::SetDebugLevel as u32, 11);
        assert_eq!(ControlMethod::InjectFault as u32, 12);
    }

    #[test]
    fn control_method_is_valid() {
        assert!(!ControlMethod::Reserved.is_valid());
        assert!(ControlMethod::OpenChannel.is_valid());
        assert!(ControlMethod::Ping.is_valid());
        assert!(ControlMethod::InjectFault.is_valid());
    }

    #[test]
    fn control_method_description() {
        assert_eq!(ControlMethod::OpenChannel.description(), "open a new data channel");
        assert_eq!(ControlMethod::Ping.description(), "liveness check");
        assert_eq!(ControlMethod::Reserved.description(), "reserved (invalid)");
    }

    #[test]
    fn control_method_display() {
        let s = format!("{}", ControlMethod::Ping);
        assert!(s.contains("liveness check"));
        assert!(s.contains("5"));
    }

    #[test]
    fn user_method_range() {
        assert!(!is_user_method(0));
        assert!(!is_user_method(12));
        assert!(!is_user_method(999));
        assert!(is_user_method(USER_METHOD_ID_START));
        assert!(is_user_method(USER_METHOD_ID_START + 1));
        assert!(is_user_method(u32::MAX));
    }

    #[test]
    fn control_method_range() {
        assert!(is_control_method(0));
        assert!(is_control_method(1));
        assert!(is_control_method(12));
        assert!(!is_control_method(13));
        assert!(!is_control_method(100));
        assert!(!is_control_method(USER_METHOD_ID_START));
    }

    #[test]
    fn unknown_method_id_display() {
        let err = UnknownMethodId(999);
        let s = format!("{}", err);
        assert!(s.contains("999"));
    }

    #[test]
    fn dispatch_error_display() {
        let err = DispatchError::UnknownMethod(123);
        let s = format!("{}", err);
        assert!(s.contains("123"));

        let err = DispatchError::MethodAlreadyRegistered(456);
        let s = format!("{}", err);
        assert!(s.contains("456"));
        assert!(s.contains("already registered"));

        let err = DispatchError::InvalidControlMethod(0);
        let s = format!("{}", err);
        assert!(s.contains("reserved control range"));
    }

    // Test handler implementation
    struct TestHandler {
        method_id: u32,
    }

    impl Handler for TestHandler {
        fn method_id(&self) -> u32 {
            self.method_id
        }
    }

    #[test]
    fn dispatcher_new() {
        let dispatcher = MethodDispatcher::new();
        assert_eq!(dispatcher.handler_count(), 0);
    }

    #[test]
    fn dispatcher_register_and_lookup() {
        let mut dispatcher = MethodDispatcher::new();

        let handler = TestHandler { method_id: 1000 };
        dispatcher.register(handler).unwrap();

        assert_eq!(dispatcher.handler_count(), 1);
        assert!(dispatcher.has_handler(1000));

        let found = dispatcher.lookup(1000).unwrap();
        assert_eq!(found.method_id(), 1000);
    }

    #[test]
    fn dispatcher_register_duplicate_fails() {
        let mut dispatcher = MethodDispatcher::new();

        dispatcher.register(TestHandler { method_id: 1000 }).unwrap();
        let result = dispatcher.register(TestHandler { method_id: 1000 });

        assert_eq!(result, Err(DispatchError::MethodAlreadyRegistered(1000)));
    }

    #[test]
    fn dispatcher_register_reserved_fails() {
        let mut dispatcher = MethodDispatcher::new();

        let result = dispatcher.register(TestHandler { method_id: 0 });
        assert_eq!(result, Err(DispatchError::InvalidControlMethod(0)));
    }

    #[test]
    fn dispatcher_lookup_missing() {
        let dispatcher = MethodDispatcher::new();
        assert!(dispatcher.lookup(999).is_none());
        assert!(!dispatcher.has_handler(999));
    }

    #[test]
    fn dispatcher_unregister() {
        let mut dispatcher = MethodDispatcher::new();

        dispatcher.register(TestHandler { method_id: 1000 }).unwrap();
        assert!(dispatcher.has_handler(1000));

        assert!(dispatcher.unregister(1000));
        assert!(!dispatcher.has_handler(1000));
        assert_eq!(dispatcher.handler_count(), 0);

        // Second unregister returns false
        assert!(!dispatcher.unregister(1000));
    }

    #[test]
    fn dispatcher_method_ids() {
        let mut dispatcher = MethodDispatcher::new();

        dispatcher.register(TestHandler { method_id: 1000 }).unwrap();
        dispatcher.register(TestHandler { method_id: 1001 }).unwrap();
        dispatcher.register(TestHandler { method_id: 1002 }).unwrap();

        let mut ids: Vec<u32> = dispatcher.method_ids().collect();
        ids.sort();

        assert_eq!(ids, vec![1000, 1001, 1002]);
    }

    #[test]
    fn dispatcher_clear() {
        let mut dispatcher = MethodDispatcher::new();

        dispatcher.register(TestHandler { method_id: 1000 }).unwrap();
        dispatcher.register(TestHandler { method_id: 1001 }).unwrap();

        assert_eq!(dispatcher.handler_count(), 2);

        dispatcher.clear();
        assert_eq!(dispatcher.handler_count(), 0);
        assert!(!dispatcher.has_handler(1000));
        assert!(!dispatcher.has_handler(1001));
    }

    #[test]
    fn dispatcher_register_arc() {
        let mut dispatcher = MethodDispatcher::new();

        let handler: Arc<dyn Handler> = Arc::new(TestHandler { method_id: 1000 });
        dispatcher.register_arc(handler.clone()).unwrap();

        assert!(dispatcher.has_handler(1000));

        let found = dispatcher.lookup(1000).unwrap();
        assert_eq!(found.method_id(), 1000);
    }

    #[test]
    fn dispatcher_multiple_handlers() {
        let mut dispatcher = MethodDispatcher::new();

        for i in 1000..1100 {
            dispatcher.register(TestHandler { method_id: i }).unwrap();
        }

        assert_eq!(dispatcher.handler_count(), 100);

        for i in 1000..1100 {
            assert!(dispatcher.has_handler(i));
            let handler = dispatcher.lookup(i).unwrap();
            assert_eq!(handler.method_id(), i);
        }
    }

    #[test]
    fn dispatcher_default() {
        let dispatcher = MethodDispatcher::default();
        assert_eq!(dispatcher.handler_count(), 0);
    }
}

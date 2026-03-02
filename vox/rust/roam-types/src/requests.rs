use facet::Facet;

// r[impl connection]
// r[impl connection.root]
declare_id!(
    /// Connection ID identifying a virtual connection on a link.
    ///
    /// Connection 0 is the root connection, established implicitly when the link is created.
    /// Additional connections are opened via Connect/Accept messages.
    ConnectionId, u64
);

impl ConnectionId {
    /// The root connection (always exists on a link).
    pub const ROOT: Self = Self(0);

    /// Check if this is the root connection.
    pub const fn is_root(self) -> bool {
        self.0 == Self::ROOT.0
    }
}

declare_id!(
    /// Request ID identifying an in-flight RPC request.
    ///
    /// Request IDs are unique within a connection and monotonically increasing.
    RequestId, u64
);

declare_id!(
    /// ID of a channel between two peers.
    ChannelId, u64
);

impl ChannelId {
    /// Reserved channel ID (not usable).
    pub const RESERVED: Self = Self(0);

    /// Create a new channel ID, returning None if the value is 0 (reserved).
    #[inline]
    pub fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }
}

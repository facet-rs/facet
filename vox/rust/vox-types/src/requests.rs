use facet::Facet;

// r[impl connection]
// r[impl connection.root]
declare_id!(
    /// Lane ID identifying a service lane on a Vox connection.
    ///
    /// Lane 0 is reserved for connection control. Application services use
    /// lanes opened with `LaneOpen` / `LaneAccept`.
    LaneId, u64
);

impl LaneId {
    /// The reserved connection-control lane.
    pub const ROOT: Self = Self(0);

    /// Check if this is the reserved connection-control lane.
    pub const fn is_root(self) -> bool {
        self.0 == Self::ROOT.0
    }
}

declare_id!(
    /// Request ID identifying an in-flight RPC request.
    ///
    /// Request IDs are unique within a lane and monotonically increasing.
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

//! Structured diagnostic snapshots for JSON serialization.
//!
//! The old inventory-based DiagnosticsSource approach has been replaced
//! by direct registration of nodes/edges in the moire registry.

use moire_types::{ChannelDir, Direction};

use crate::diagnostic::{ChannelDirection, RequestDirection};

impl From<RequestDirection> for Direction {
    fn from(d: RequestDirection) -> Self {
        match d {
            RequestDirection::Outgoing => Direction::Outgoing,
            RequestDirection::Incoming => Direction::Incoming,
        }
    }
}

impl From<ChannelDirection> for ChannelDir {
    fn from(d: ChannelDirection) -> Self {
        match d {
            ChannelDirection::Tx => ChannelDir::Tx,
            ChannelDirection::Rx => ChannelDir::Rx,
        }
    }
}

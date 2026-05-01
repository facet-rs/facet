use vox_types::{Link, MsgFamily};

use crate::BareConduit;

/// Converts a value into a [`vox_types::Conduit`].
///
/// Implemented for:
/// - [`BareConduit`] → identity (pass-through)
///
/// This allows [`crate::Session`] connection handling methods to accept
/// already-constructed conduits without additional wrapping.
pub trait IntoConduit {
    /// The conduit type produced by this conversion.
    type Conduit;

    /// Convert into a conduit.
    fn into_conduit(self) -> Self::Conduit;
}

/// [`BareConduit`] passes through unchanged.
impl<F: MsgFamily, L: Link> IntoConduit for BareConduit<F, L> {
    type Conduit = BareConduit<F, L>;

    fn into_conduit(self) -> Self::Conduit {
        self
    }
}

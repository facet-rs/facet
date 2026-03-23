use vox_types::{Link, MessageFamily, MsgFamily};

use crate::BareConduit;

/// Converts a value into a [`vox_types::Conduit`].
///
/// Implemented for:
/// - Any [`Link`] → wraps it in a [`BareConduit`] automatically
/// - [`BareConduit`] → identity (pass-through)
/// - [`crate::StableConduit`] → identity (pass-through)
///
/// This allows [`crate::Session`] connection handling methods
/// to accept raw links directly, without requiring callers to wrap them in
/// `BareConduit::new()` manually.
pub trait IntoConduit {
    /// The conduit type produced by this conversion.
    type Conduit;

    /// Convert into a conduit.
    fn into_conduit(self) -> Self::Conduit;
}

/// Any [`Link`] becomes a [`BareConduit`] over [`MessageFamily`].
impl<L: Link> IntoConduit for L {
    type Conduit = BareConduit<MessageFamily, L>;

    fn into_conduit(self) -> Self::Conduit {
        BareConduit::new(self)
    }
}

/// [`BareConduit`] passes through unchanged.
impl<F: MsgFamily, L: Link> IntoConduit for BareConduit<F, L> {
    type Conduit = BareConduit<F, L>;

    fn into_conduit(self) -> Self::Conduit {
        self
    }
}

/// [`crate::StableConduit`] passes through unchanged.
#[cfg(not(target_arch = "wasm32"))]
impl<F: MsgFamily, LS: crate::LinkSource> IntoConduit for crate::StableConduit<F, LS> {
    type Conduit = crate::StableConduit<F, LS>;

    fn into_conduit(self) -> Self::Conduit {
        self
    }
}

//! facet-hash native stencils using explicit tail calls.

#![feature(explicit_tail_calls)]
#![allow(clippy::missing_safety_doc)]

macro_rules! continue_to {
    ($target:ident, $cx:expr) => {
        become $target($cx)
    };
}

include!("common.rs");

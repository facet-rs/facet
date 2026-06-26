//! facet-hash native stencils.
//!
//! These are compiled to an object by `build.rs`, then copied and patched into a
//! native chain at runtime. Per-op immediates ride in `Ctx.prog`; scalar hashing
//! itself stays in monomorphized host calls so the stencil ABI is independent of
//! the concrete `Hasher`.

#![allow(clippy::missing_safety_doc)]

macro_rules! continue_to {
    ($target:ident, $cx:expr) => {
        $target($cx)
    };
}

include!("common.rs");

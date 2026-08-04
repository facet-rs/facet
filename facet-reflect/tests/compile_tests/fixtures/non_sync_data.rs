//! Soundness test for GitHub issue #1573 — the `Attr::new` path.
//!
//! `Attr` is `Send + Sync` by a hand-written `unsafe impl`, and its payload is a
//! type-erased `OxRef`, so nothing but the construction site can check that the
//! payload is `Sync`. An `Attr` over an `&'static Rc<i32>` lets many threads
//! race the `Rc`'s non-atomic refcount from safe code.
//!
//! `1524e017a` closed this entry point with a `T: Sync` bound. See
//! `non_sync_attr_literal.rs` for the struct-literal path, which it missed.
//!
//! The `Box::leak` is deliberate: an earlier version of this fixture used
//! `static RC: LazyLock<Rc<i32>>`, which is itself rejected with
//! "`Rc<i32>` cannot be shared between threads safely" because statics must be
//! `Sync` — so the test passed on an error that had nothing to do with `Attr`,
//! and would have kept passing with the bound removed.

#![forbid(unsafe_code)]

use std::rc::Rc;

use facet::Attr;

fn main() {
    let rc: &'static Rc<i32> = Box::leak(Box::new(Rc::new(0)));

    // Rc is not Sync, so `Attr::new`'s bound must reject this.
    let _attr = Attr::new(None, "", rc);
}

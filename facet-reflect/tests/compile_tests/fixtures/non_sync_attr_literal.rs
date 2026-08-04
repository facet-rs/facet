//! Soundness test for GitHub issue #1573 — the struct-literal path.
//!
//! This is the half that `1524e017a` missed. It added `T: Sync` to `Attr::new`,
//! but all three of `Attr`'s fields were already `pub` on that day, so a struct
//! literal skipped the constructor and the bound with it. `OxRef::from_ref` is
//! safe and carries no `Sync` bound of its own (correctly — `OxRef` is neither
//! `Send` nor `Sync`, so it needs none), which makes the line below a complete,
//! `#![forbid(unsafe_code)]`-clean reproduction of dtolnay's original:
//!
//! ```text
//! error: Undefined Behavior: Data race detected between (1) non-atomic write
//! on thread `unnamed-1` and (2) non-atomic read on thread `unnamed-2`
//! ```
//!
//! Natively it corrupts the refcount or aborts with "unsafe precondition(s)
//! violated: hint::assert_unchecked". The fields are private now, so this must
//! not compile at all.
//!
//! Kept as a separate fixture from `non_sync_data.rs` on purpose: rustc runs the
//! privacy pass *after* type checking and bails if type checking failed, so a
//! trait-bound error anywhere in the same crate would suppress the E0451 this
//! test is looking for.

#![forbid(unsafe_code)]

use std::rc::Rc;

use facet::{Attr, OxRef};

fn main() {
    let rc: &'static Rc<i32> = Box::leak(Box::new(Rc::new(0)));

    // Must not be a way around `Attr::new`'s `T: Sync` bound.
    let _attr = Attr {
        ns: None,
        key: "",
        data: OxRef::from_ref(rc),
    };
}

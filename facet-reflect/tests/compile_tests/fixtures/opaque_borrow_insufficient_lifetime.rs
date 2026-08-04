//! Regression test for GitHub issue #1563, second door.
//!
//! #1563 was reported against `Opaque<&mut String>` and fixed in bdf1dcfa6 by
//! requiring `T: 'static` on `Opaque`'s `Facet` impl — see the sibling fixture
//! `opaque_insufficient_lifetime.rs`. cd254d928 (#2087) then added
//! `OpaqueBorrow<'facet, T: 'facet>` so that `#[facet(opaque)]` fields could be
//! borrowed again, which reopened the hole from a type the old fixture never named.
//!
//! `OpaqueBorrow<'x, T>` implements `Facet<'facet>` only for `'facet == 'x`. That
//! is the whole safety argument, because `ConstTypeId` erases lifetimes and cannot
//! tell `OpaqueBorrow<'a, _>` from `OpaqueBorrow<'static, _>` on its own. So asking
//! a `Poke<'_, 'a>` for the `'static` flavour must be rejected by the *borrow
//! checker*, not by a runtime shape comparison.
//!
//! (The mirror case — asking the same `Poke` for `Opaque<&'static mut String>`,
//! which *does* compile because `Opaque<T>` is `Facet<'facet>` for every `'facet` —
//! is guarded at runtime by `poke::opaque_lifetime_laundering`.)

use facet::OpaqueBorrow;
use facet_reflect::Poke;

fn steal<'a>(borrowed: &'a mut String) -> &'static mut String {
    let mut wrapper: OpaqueBorrow<'a, &'a mut String> = OpaqueBorrow::new(borrowed);
    let mut poke = Poke::new(&mut wrapper);
    let laundered = poke
        .get_mut::<OpaqueBorrow<'static, &'static mut String>>()
        .unwrap();
    core::mem::replace(&mut laundered.0, Box::leak(Box::new(String::new())))
}

fn main() {
    let stolen: &'static mut String = {
        let mut victim = String::from("i am a stack local");
        steal(&mut victim)
    }; // `victim` is dropped here -- its heap buffer is freed
    stolen.push_str("!!!");
    println!("{stolen}");
}

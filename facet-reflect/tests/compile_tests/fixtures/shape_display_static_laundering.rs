// Soundness test: safe code must not be able to obtain a `&'static Shape` that
// outlives the `Shape` it points at.
//
// `impl Display for Shape` used to `transmute` its `&self` to `&'static Shape`,
// on the claim that "all Shape instances are guaranteed to be 'static because
// they're always references to const statics". That is a convention of the derive
// macro, not an enforced invariant — `ShapeBuilder::build()` is safe, public, and
// returns an owned `Shape` by value.
//
// Chained with `ShapeBuilder::type_name()` (safe) and a `TypeNameFn` whose first
// parameter was `&'static Shape`, the five lines below stashed a `&'static Shape`
// pointing into a stack frame that then returned — native SIGSEGV, and under Miri
// "constructing invalid value: encountered a dangling reference (use-after-free)".
// Note the `#![forbid(unsafe_code)]`: no unsafe was needed anywhere.
//
// After the fix, `TypeNameFn` takes `&Shape`, so `evil` — which insists on
// `&'static Shape` — is no longer a valid `TypeNameFn` and this must not compile.
#![forbid(unsafe_code)]

use facet::{Shape, ShapeBuilder, TypeNameOpts};
use std::fmt;
use std::sync::OnceLock;

static STASH: OnceLock<&'static Shape> = OnceLock::new();

fn evil(shape: &'static Shape, _f: &mut fmt::Formatter<'_>, _o: TypeNameOpts) -> fmt::Result {
    let _ = STASH.set(shape);
    Ok(())
}

fn launder() {
    // An owned, stack-resident Shape. Entirely safe, entirely public API.
    let s: Shape = ShapeBuilder::for_sized::<u8>("Victim").type_name(evil).build();
    // This should fail to compile: `evil` requires `&'static Shape`, but
    // `TypeNameFn` is now higher-ranked over the shape's lifetime.
    let _ = format!("{}", s);
}

fn main() {
    launder();
    // `s` is dead. Before the fix, STASH held a reference into its frame.
    println!("{:?}", STASH.get().unwrap().type_identifier);
}

//! Regression test for GitHub issue #1563
//!
//! This code was unsound before the fix because Opaque's Facet implementation
//! was covariant, allowing unsafe lifetime coercion. The code creates an
//! Opaque<&mut String> with a short lifetime, then uses Poke::get_mut to
//! get an Opaque<&mut String> with a longer lifetime (by inference), and
//! assigns a short-lived reference to it.
//!
//! After the fix, Opaque<T> requires T: 'static, preventing this code from compiling.

use facet::Opaque;
use facet_reflect::Poke;

fn main() {
    let mut s = String::new();
    let mut t = Opaque(&mut s);
    Poke::new(&mut t)
        .get_mut::<Opaque<&mut String>>()
        .unwrap()
        .0 = &mut ".".repeat(200);
    println!("{}", t.0);
}

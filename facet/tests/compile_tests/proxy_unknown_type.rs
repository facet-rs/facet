//! Test that proxy attribute errors point to the correct span.
//!
//! The error should point to `NonExistentProxyType`, not the macro expansion site.

use facet::Facet;

#[derive(Facet)]
struct TestProxy {
    #[facet(proxy = NonExistentProxyType)]
    value: i32,
}

fn main() {}

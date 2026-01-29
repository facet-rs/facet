//! Regression test for GitHub issue #1555
//!
//! This code was unsound before the fix because OxRef::new was safe
//! but accepted arbitrary pointers, allowing use-after-free through
//! safe code like PartialEq.
//!
//! After the fix, OxRef::new is unsafe, so this code should fail to compile.

use facet::{Facet, OxRef, PtrConst, Shape, ShapeBuilder, VTableDirect};

#[derive(PartialEq, Facet)]
struct MyType(String);

fn main() {
    let shape = MyType::SHAPE;
    // These are invalid pointers
    let a = PtrConst::new(0x1111111 as *const MyType);
    let b = PtrConst::new(0x2222222 as *const MyType);

    // This should fail to compile because OxRef::new is now unsafe
    let ox_a = OxRef::new(a, shape);
    let ox_b = OxRef::new(b, shape);

    // This would have crashed before (dereferencing invalid pointers)
    let _ = ox_a == ox_b;
}

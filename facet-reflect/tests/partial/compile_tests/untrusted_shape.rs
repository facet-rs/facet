// Compile test for GitHub issue #1663
// https://github.com/facet-rs/facet/issues/1663
//
// Before the fix, creating a Partial with an untrusted shape was safe,
// allowing soundness bugs where the shape doesn't match the actual type.
//
// After the fix, Partial::alloc_shape() is unsafe, so this code should
// fail to compile without an unsafe block.

use facet::{Def, Facet, Shape, StructKind, Type};
use facet_reflect::Partial;

#[derive(Facet)]
struct Thing {
    x: String,
}

fn main() {
    // Create a malicious shape: claims to be Thing, but describes a unit struct
    let bad_shape = const {
        &Shape::builder_for_sized::<Thing>("Thing")
            .ty(Type::struct_builder(StructKind::Unit, &[]).build())
            .def(Def::Undefined)
            .build()
    };

    // This should fail to compile - alloc_shape is unsafe
    let _partial = Partial::alloc_shape(bad_shape);
}

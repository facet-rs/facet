use crate::{
    Def, Facet, HashProxy, OxPtrConst, OxPtrMut, Repr, Shape, ShapeBuilder, StructKind, StructType,
    Type, TypeOpsIndirect, UserType, VTableIndirect,
};

unsafe fn infallible_drop(_ptr: OxPtrMut) {
    // Infallible is zero-sized, nothing to drop
}

// Infallible vtable - implementations that can never be called since Infallible cannot be instantiated
const INFALLIBLE_VTABLE: VTableIndirect = VTableIndirect {
    display: None,
    debug: Some(infallible_debug),
    hash: Some(infallible_hash),
    invariants: None,
    parse: None,
    parse_bytes: None,
    try_from: None,
    try_into_inner: None,
    try_borrow_inner: None,
    partial_eq: Some(infallible_partial_eq),
    partial_cmp: Some(infallible_partial_cmp),
    cmp: Some(infallible_cmp),
};

// Type operations for Infallible
static INFALLIBLE_TYPE_OPS: TypeOpsIndirect = TypeOpsIndirect {
    drop_in_place: infallible_drop,
    default_in_place: None, // Infallible cannot be constructed
    clone_into: None,
    is_truthy: None,
};

unsafe fn infallible_debug(
    _ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    // This can never be called since Infallible cannot be instantiated
    Some(f.write_str("Infallible"))
}

unsafe fn infallible_hash(_ox: OxPtrConst, _hasher: &mut HashProxy<'_>) -> Option<()> {
    // This can never be called since Infallible cannot be instantiated
    Some(())
}

unsafe fn infallible_partial_eq(_a: OxPtrConst, _b: OxPtrConst) -> Option<bool> {
    // This can never be called since Infallible cannot be instantiated
    Some(true)
}

unsafe fn infallible_partial_cmp(
    _a: OxPtrConst,
    _b: OxPtrConst,
) -> Option<Option<core::cmp::Ordering>> {
    // This can never be called since Infallible cannot be instantiated
    Some(Some(core::cmp::Ordering::Equal))
}

unsafe fn infallible_cmp(_a: OxPtrConst, _b: OxPtrConst) -> Option<core::cmp::Ordering> {
    // This can never be called since Infallible cannot be instantiated
    Some(core::cmp::Ordering::Equal)
}

unsafe impl Facet<'_> for core::convert::Infallible {
    const SHAPE: &'static Shape = &const {
        // Infallible implements Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash
        // but NOT Default (cannot be constructed) or Display

        ShapeBuilder::for_sized::<core::convert::Infallible>("Infallible")
            .ty(Type::User(UserType::Struct(StructType {
                repr: Repr::default(),
                kind: StructKind::Unit,
                fields: &[],
            })))
            .def(Def::Scalar)
            .vtable_indirect(&INFALLIBLE_VTABLE)
            .type_ops_indirect(&INFALLIBLE_TYPE_OPS)
            .eq()
            .copy()
            .send()
            .sync()
            .build()
    };
}

// Note: The never type (!) implementation is currently not included because
// it requires the unstable `never_type` feature. Once the feature is stabilized,
// we can add the implementation here following the same pattern as Infallible.

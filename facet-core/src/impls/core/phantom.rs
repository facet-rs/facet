use crate::{
    Def, Facet, HashProxy, OxPtrConst, OxPtrMut, OxPtrUninit, Repr, Shape, ShapeBuilder,
    StructKind, StructType, Type, TypeOpsIndirect, UserType, VTableIndirect,
};

const unsafe fn phantom_drop(_ptr: OxPtrMut) {
    // PhantomData is zero-sized, nothing to drop
}

const unsafe fn phantom_default(_dst: OxPtrUninit) -> bool {
    // PhantomData is zero-sized, nothing to write
    true
}

// Shared vtable for all PhantomData<T> - the implementations don't depend on T
const PHANTOM_VTABLE: VTableIndirect = VTableIndirect {
    display: None,
    debug: Some(phantom_debug),
    hash: Some(phantom_hash),
    invariants: None,
    parse: None,
    parse_bytes: None,
    try_from: None,
    try_into_inner: None,
    try_borrow_inner: None,
    partial_eq: Some(phantom_partial_eq),
    partial_cmp: Some(phantom_partial_cmp),
    cmp: Some(phantom_cmp),
};

// Type operations for all PhantomData<T>
static PHANTOM_TYPE_OPS: TypeOpsIndirect = TypeOpsIndirect {
    drop_in_place: phantom_drop,
    default_in_place: Some(phantom_default),
    clone_into: None,
    is_truthy: None,
};

unsafe fn phantom_debug(
    _ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    Some(f.write_str("PhantomData"))
}

const unsafe fn phantom_hash(_ox: OxPtrConst, _hasher: &mut HashProxy<'_>) -> Option<()> {
    // PhantomData hashes to nothing
    Some(())
}

const unsafe fn phantom_partial_eq(_a: OxPtrConst, _b: OxPtrConst) -> Option<bool> {
    // All PhantomData are equal
    Some(true)
}

const unsafe fn phantom_partial_cmp(
    _a: OxPtrConst,
    _b: OxPtrConst,
) -> Option<Option<core::cmp::Ordering>> {
    Some(Some(core::cmp::Ordering::Equal))
}

const unsafe fn phantom_cmp(_a: OxPtrConst, _b: OxPtrConst) -> Option<core::cmp::Ordering> {
    Some(core::cmp::Ordering::Equal)
}

unsafe impl<'a, T: ?Sized + 'a> Facet<'a> for core::marker::PhantomData<T> {
    const SHAPE: &'static Shape = &const {
        // PhantomData<T> implements Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash
        // unconditionally (not depending on T) - but NOT Display

        ShapeBuilder::for_sized::<core::marker::PhantomData<T>>("PhantomData")
            .module_path("core::marker")
            .ty(Type::User(UserType::Struct(StructType {
                repr: Repr::default(),
                kind: StructKind::Unit,
                fields: &[],
            })))
            .def(Def::Scalar)
            .vtable_indirect(&PHANTOM_VTABLE)
            .type_ops_indirect(&PHANTOM_TYPE_OPS)
            .eq()
            .copy()
            .send()
            .sync()
            .build()
    };
}

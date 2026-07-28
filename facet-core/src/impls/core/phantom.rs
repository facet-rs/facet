use crate::{
    Def, Facet, HashProxy, OxPtrConst, OxPtrMut, OxPtrUninit, Repr, Shape, ShapeBuilder,
    StructKind, StructType, Type, TypeOpsIndirect, UserType, VTableIndirect, VarianceDesc,
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
            // `PhantomData<T>` has the variance of `T`, but this impl is
            // `T: ?Sized + 'a` with **no `T: Facet<'a>` bound**, so there is no
            // `T::SHAPE` to take a `VarianceDep` on. Our only options are the
            // constants, and `Invariant` is the sole sound one:
            //
            // - `Bivariant` (the old `ShapeBuilder` default) claims "no lifetime
            //   parameters at all". That is a lie for `PhantomData<&'a str>`, and it
            //   is exactly how the canonical hand-rolled-borrow pattern
            //   `struct S<'a> { ptr: *const u8, len: usize, _m: PhantomData<&'a [u8]> }`
            //   ends up reported as `Bivariant` — every field claims to carry no
            //   lifetime — so `grow_lifetime` hands out an `S<'static>` whose safe
            //   accessors then return dangling references.
            // - `Covariant` would be right for the common cases (`&'a T`, `Box<T>`,
            //   `[T]`) but wrong for `PhantomData<&'a mut T>` and
            //   `PhantomData<fn(&'a T)>`, which are invariant/contravariant. Those
            //   are not exotic — `PhantomData<&'a mut T>` is the standard `IterMut`
            //   marker.
            //
            // The cost is real: `struct S<'a> { s: &'a str, _m: PhantomData<&'a str> }`
            // is downgraded from `Covariant` to `Invariant`, losing `shrink_lifetime`.
            // We take the false negative over the unsoundness.
            //
            // TODO: this is a symptom of `Variance::Bivariant` doing double duty as
            // both the lattice identity and the "nobody declared this" placeholder.
            // The systemic fix is a `Variance::Unknown` variant (breaking: needs
            // `#[non_exhaustive]` on `Variance` plus ~109 declarations across
            // facet-core), planned for 1.0. Once `PhantomData` can express "same
            // variance as T" — either via `Unknown` or by gaining a `T: Facet<'a>`
            // bound — this should be relaxed back to propagating T.
            .variance(VarianceDesc::INVARIANT)
            .eq()
            .copy()
            .send()
            .sync()
            .build()
    };
}

//! Facet implementation for `Option<T>`

use core::cmp::Ordering;

use crate::{
    Def, EnumRepr, EnumType, Facet, FieldBuilder, HashProxy, OptionDef, OptionVTable, OxPtrConst,
    OxPtrMut, OxRef, PtrConst, Repr, Shape, ShapeBuilder, Type, TypeOpsIndirect, TypeParam,
    UserType, VTableIndirect, VariantBuilder,
};

/// Extract the OptionDef from a shape, returns None if not an Option
#[inline]
fn get_option_def(shape: &'static Shape) -> Option<&'static OptionDef> {
    match shape.def {
        Def::Option(ref def) => Some(def),
        _ => None,
    }
}

/// Display for `Option<T>` - delegates to inner T's display if available
unsafe fn option_display(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let shape = ox.shape();
    let def = get_option_def(shape)?;
    let ptr = ox.ptr();

    if unsafe { (def.vtable.is_some)(ptr) } {
        // Get the inner value using the vtable
        let inner_ptr = unsafe { (def.vtable.get_value)(ptr)? };
        // Delegate to inner type's display
        unsafe { def.t.call_display(inner_ptr, f) }
    } else {
        Some(f.write_str("None"))
    }
}

/// Debug for `Option<T>` - delegates to inner T's debug if available
unsafe fn option_debug(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let shape = ox.shape();
    let def = get_option_def(shape)?;
    let ptr = ox.ptr();

    if unsafe { (def.vtable.is_some)(ptr) } {
        // Get the inner value using the vtable
        // SAFETY: is_some returned true, so get_value returns a valid pointer.
        // The caller guarantees the OxPtrConst points to a valid Option.
        let inner_ptr = unsafe { (def.vtable.get_value)(ptr)? };
        let inner_ox = unsafe { OxRef::new(inner_ptr, def.t) };
        Some(f.debug_tuple("Some").field(&inner_ox).finish())
    } else {
        Some(f.write_str("None"))
    }
}

/// Hash for `Option<T>` - delegates to inner T's hash if available
unsafe fn option_hash(ox: OxPtrConst, hasher: &mut HashProxy<'_>) -> Option<()> {
    let shape = ox.shape();
    let def = get_option_def(shape)?;
    let ptr = ox.ptr();

    use core::hash::Hash;
    if unsafe { (def.vtable.is_some)(ptr) } {
        1u8.hash(hasher);
        let inner_ptr = unsafe { (def.vtable.get_value)(ptr)? };
        unsafe { def.t.call_hash(inner_ptr, hasher)? };
    } else {
        0u8.hash(hasher);
    }
    Some(())
}

/// PartialEq for `Option<T>`
unsafe fn option_partial_eq(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    let shape = a.shape();
    let def = get_option_def(shape)?;

    let a_ptr = a.ptr();
    let b_ptr = b.ptr();
    let a_is_some = unsafe { (def.vtable.is_some)(a_ptr) };
    let b_is_some = unsafe { (def.vtable.is_some)(b_ptr) };

    Some(match (a_is_some, b_is_some) {
        (false, false) => true,
        (true, true) => {
            let a_inner = unsafe { (def.vtable.get_value)(a_ptr)? };
            let b_inner = unsafe { (def.vtable.get_value)(b_ptr)? };
            unsafe { def.t.call_partial_eq(a_inner, b_inner)? }
        }
        _ => false,
    })
}

/// PartialOrd for `Option<T>`
unsafe fn option_partial_cmp(a: OxPtrConst, b: OxPtrConst) -> Option<Option<Ordering>> {
    let shape = a.shape();
    let def = get_option_def(shape)?;

    let a_ptr = a.ptr();
    let b_ptr = b.ptr();
    let a_is_some = unsafe { (def.vtable.is_some)(a_ptr) };
    let b_is_some = unsafe { (def.vtable.is_some)(b_ptr) };

    Some(match (a_is_some, b_is_some) {
        (false, false) => Some(Ordering::Equal),
        (false, true) => Some(Ordering::Less),
        (true, false) => Some(Ordering::Greater),
        (true, true) => {
            let a_inner = unsafe { (def.vtable.get_value)(a_ptr)? };
            let b_inner = unsafe { (def.vtable.get_value)(b_ptr)? };
            unsafe { def.t.call_partial_cmp(a_inner, b_inner)? }
        }
    })
}

/// Ord for `Option<T>`
unsafe fn option_cmp(a: OxPtrConst, b: OxPtrConst) -> Option<Ordering> {
    let shape = a.shape();
    let def = get_option_def(shape)?;

    let a_ptr = a.ptr();
    let b_ptr = b.ptr();
    let a_is_some = unsafe { (def.vtable.is_some)(a_ptr) };
    let b_is_some = unsafe { (def.vtable.is_some)(b_ptr) };

    Some(match (a_is_some, b_is_some) {
        (false, false) => Ordering::Equal,
        (false, true) => Ordering::Less,
        (true, false) => Ordering::Greater,
        (true, true) => {
            let a_inner = unsafe { (def.vtable.get_value)(a_ptr)? };
            let b_inner = unsafe { (def.vtable.get_value)(b_ptr)? };
            unsafe { def.t.call_cmp(a_inner, b_inner)? }
        }
    })
}

/// Drop for `Option<T>`
unsafe fn option_drop(ox: OxPtrMut) {
    let shape = ox.shape();
    let Some(def) = get_option_def(shape) else {
        return;
    };
    let ptr = ox.ptr();

    if unsafe { (def.vtable.is_some)(ptr.as_const()) } {
        // Get a mutable pointer to the inner value directly.
        // We can't use get_value() because it creates a shared reference via as_ref(),
        // which would violate Stacked Borrows when we try to drop through a mutable pointer.
        // Instead, call the typed drop function which takes a mutable pointer.
        unsafe { option_drop_inner(ptr, def) };
    }
}

/// Helper to drop the inner value of a Some option with proper mutable access
unsafe fn option_drop_inner(ptr: crate::PtrMut, def: &OptionDef) {
    // Use the replace_with vtable function to replace Some with None.
    // This properly handles the drop of the inner value.
    unsafe { (def.vtable.replace_with)(ptr, None) };
}

/// Default for `Option<T>` - always None (no `T::Default` requirement)
unsafe fn option_default<T>(ox: OxPtrMut) {
    let ptr = ox.ptr();
    unsafe { ptr.as_uninit().put(Option::<T>::None) };
}

/// Check if `Option<T>` is Some
unsafe fn option_is_some<T>(option: PtrConst) -> bool {
    unsafe { option.get::<Option<T>>().is_some() }
}

/// Get the value from `Option<T>` if present
unsafe fn option_get_value<T>(option: PtrConst) -> Option<PtrConst> {
    unsafe {
        option
            .get::<Option<T>>()
            .as_ref()
            .map(|t| PtrConst::new(t as *const T))
    }
}

/// Initialize `Option<T>` with Some(value)
unsafe fn option_init_some<T>(option: crate::PtrUninit, value: PtrConst) -> crate::PtrMut {
    unsafe { option.put(Option::Some(value.read::<T>())) }
}

/// Initialize `Option<T>` with None
unsafe fn option_init_none<T>(option: crate::PtrUninit) -> crate::PtrMut {
    unsafe { option.put(<Option<T>>::None) }
}

/// Replace `Option<T>` with a new value
unsafe fn option_replace_with<T>(option: crate::PtrMut, value: Option<PtrConst>) {
    unsafe {
        let option = option.as_mut::<Option<T>>();
        match value {
            Some(value) => {
                option.replace(value.read::<T>());
            }
            None => {
                option.take();
            }
        };
    }
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for Option<T> {
    const SHAPE: &'static Shape = &const {
        const fn build_option_vtable<T>() -> OptionVTable {
            OptionVTable::builder()
                .is_some(option_is_some::<T>)
                .get_value(option_get_value::<T>)
                .init_some(option_init_some::<T>)
                .init_none(option_init_none::<T>)
                .replace_with(option_replace_with::<T>)
                .build()
        }

        const fn build_vtable() -> VTableIndirect {
            VTableIndirect {
                display: Some(option_display),
                debug: Some(option_debug),
                hash: Some(option_hash),
                invariants: None,
                parse: None,
                parse_bytes: None,
                try_from: None,
                try_into_inner: None,
                try_borrow_inner: None,
                partial_eq: Some(option_partial_eq),
                partial_cmp: Some(option_partial_cmp),
                cmp: Some(option_cmp),
            }
        }

        const fn build_type_ops<T>() -> TypeOpsIndirect {
            TypeOpsIndirect {
                drop_in_place: option_drop,
                default_in_place: Some(option_default::<T>),
                clone_into: None,
                is_truthy: Some(option_is_some::<T>),
            }
        }

        ShapeBuilder::for_sized::<Option<T>>("Option")
            .ty(Type::User(
                // Null-Pointer-Optimization check
                if core::mem::size_of::<T>() == core::mem::size_of::<Option<T>>()
                    && core::mem::size_of::<T>() <= core::mem::size_of::<usize>()
                {
                    UserType::Enum(EnumType {
                        repr: Repr::default(),
                        enum_repr: EnumRepr::RustNPO,
                        variants: &const {
                            [
                                VariantBuilder::unit("None").discriminant(0).build(),
                                VariantBuilder::tuple(
                                    "Some",
                                    &const { [FieldBuilder::new("0", crate::shape_of::<T>, 0).build()] },
                                )
                                .discriminant(0)
                                .build(),
                            ]
                        },
                    })
                } else {
                    UserType::Opaque
                },
            ))
            .def(Def::Option(OptionDef::new(
                &const { build_option_vtable::<T>() },
                T::SHAPE,
            )))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            .vtable_indirect(&const { build_vtable() })
            .type_ops_indirect(&const { build_type_ops::<T>() })
            // Option<T> propagates T's variance
            .variance(Shape::computed_variance)
            .build()
    };
}

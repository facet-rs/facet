use crate::{
    Def, Facet, KnownPointer, OxPtrConst, OxPtrMut, PointerDef, PointerFlags, PointerVTable,
    PtrConst, Shape, ShapeBuilder, Type, TypeNameFn, TypeNameOpts, TypeOpsIndirect, TypeParam,
    UserType, VTableIndirect,
};
use alloc::borrow::Cow;
use alloc::borrow::ToOwned;

/// Debug for `Cow<T>` - delegates to inner T's debug
///
/// # Safety
/// The pointer must point to a valid Cow<'_, T> value
unsafe fn cow_debug<T: ?Sized + ToOwned + 'static>(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result>
where
    T::Owned: 'static,
{
    let cow_ref: &Cow<'_, T> = unsafe { ox.get::<Cow<'static, T>>() };

    // Get T's shape from the Cow's shape
    let cow_shape = ox.shape();
    let t_shape = cow_shape.inner?;

    let inner_ref: &T = cow_ref.as_ref();

    let inner_ptr = PtrConst::new(inner_ref as *const T);
    unsafe { t_shape.call_debug(inner_ptr, f) }
}

/// Display for `Cow<T>` - delegates to inner T's display if available
///
/// # Safety
/// The pointer must point to a valid Cow<'_, T> value
unsafe fn cow_display<T: ?Sized + ToOwned + 'static>(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result>
where
    T::Owned: 'static,
{
    let cow_ref: &Cow<'_, T> = unsafe { ox.get::<Cow<'static, T>>() };

    // Get T's shape from the Cow's shape
    let cow_shape = ox.shape();
    let t_shape = cow_shape.inner?;

    if !t_shape.vtable.has_display() {
        return None;
    }

    let inner_ref: &T = cow_ref.as_ref();
    let inner_ptr = PtrConst::new(inner_ref as *const T);

    unsafe { t_shape.call_display(inner_ptr, f) }
}

/// PartialEq for `Cow<T>`
///
/// # Safety
/// Both pointers must point to valid Cow<'_, T> values
unsafe fn cow_partial_eq<T: ?Sized + ToOwned + 'static>(
    a: OxPtrConst,
    b: OxPtrConst,
) -> Option<bool>
where
    T::Owned: 'static,
{
    let a_cow_ref: &Cow<'_, T> = unsafe { a.get::<Cow<'static, T>>() };
    let b_cow_ref: &Cow<'_, T> = unsafe { b.get::<Cow<'static, T>>() };

    let cow_shape = a.shape();
    let t_shape = cow_shape.inner?;

    let a_inner = PtrConst::new(a_cow_ref.as_ref() as *const T);
    let b_inner = PtrConst::new(b_cow_ref.as_ref() as *const T);

    unsafe { t_shape.call_partial_eq(a_inner, b_inner) }
}

/// PartialOrd for `Cow<T>`
///
/// # Safety
/// Both pointers must point to valid Cow<'_, T> values
unsafe fn cow_partial_cmp<T: ?Sized + ToOwned + 'static>(
    a: OxPtrConst,
    b: OxPtrConst,
) -> Option<Option<core::cmp::Ordering>>
where
    T::Owned: 'static,
{
    let a_cow_ref: &Cow<'_, T> = unsafe { a.get::<Cow<'static, T>>() };
    let b_cow_ref: &Cow<'_, T> = unsafe { b.get::<Cow<'static, T>>() };

    let cow_shape = a.shape();
    let t_shape = cow_shape.inner?;

    let a_inner = PtrConst::new(a_cow_ref.as_ref() as *const T);
    let b_inner = PtrConst::new(b_cow_ref.as_ref() as *const T);

    unsafe { t_shape.call_partial_cmp(a_inner, b_inner) }
}

/// Ord for `Cow<T>`
///
/// # Safety
/// Both pointers must point to valid Cow<'_, T> values
unsafe fn cow_cmp<T: ?Sized + ToOwned + 'static>(
    a: OxPtrConst,
    b: OxPtrConst,
) -> Option<core::cmp::Ordering>
where
    T::Owned: 'static,
{
    let a_cow_ref: &Cow<'_, T> = unsafe { a.get::<Cow<'static, T>>() };
    let b_cow_ref: &Cow<'_, T> = unsafe { b.get::<Cow<'static, T>>() };

    let cow_shape = a.shape();
    let t_shape = cow_shape.inner?;

    let a_inner = PtrConst::new(a_cow_ref.as_ref() as *const T);
    let b_inner = PtrConst::new(b_cow_ref.as_ref() as *const T);

    unsafe { t_shape.call_cmp(a_inner, b_inner) }
}

/// Borrow the inner value from `Cow<T>`
///
/// # Safety
/// `this` must point to a valid Cow<'_, T> value
unsafe fn cow_borrow<T: ?Sized + ToOwned + 'static>(this: PtrConst) -> PtrConst
where
    T::Owned: 'static,
{
    // SAFETY: Same layout reasoning as cow_debug
    let cow_ref: &Cow<'_, T> =
        unsafe { &*(this.as_byte_ptr() as *const alloc::borrow::Cow<'_, T>) };
    let inner_ref: &T = cow_ref.as_ref();
    PtrConst::new(inner_ref as *const T)
}

unsafe impl<'a, T> Facet<'a> for Cow<'a, T>
where
    T: 'a + ?Sized + ToOwned + 'static,
    T: Facet<'a>,
    T::Owned: Facet<'static>,
{
    const SHAPE: &'static Shape = &const {
        const fn build_cow_vtable<T: ?Sized + ToOwned + 'static>() -> VTableIndirect
        where
            T::Owned: Facet<'static> + 'static,
        {
            VTableIndirect {
                debug: Some(cow_debug::<T>),
                display: Some(cow_display::<T>),
                partial_eq: Some(cow_partial_eq::<T>),
                partial_cmp: Some(cow_partial_cmp::<T>),
                cmp: Some(cow_cmp::<T>),
                ..VTableIndirect::EMPTY
            }
        }

        const fn build_cow_type_ops<'facet, T>() -> TypeOpsIndirect
        where
            T: ?Sized + ToOwned + 'static + Facet<'facet>,
            T::Owned: Facet<'static> + 'static,
        {
            unsafe fn drop_in_place<T: ?Sized + ToOwned + 'static>(ox: OxPtrMut)
            where
                T::Owned: 'static,
            {
                unsafe {
                    core::ptr::drop_in_place(
                        ox.ptr().as_ptr::<Cow<'static, T>>() as *mut Cow<'static, T>
                    )
                };
            }

            unsafe fn clone_into<T: ?Sized + ToOwned + 'static>(src: OxPtrConst, dst: OxPtrMut)
            where
                T::Owned: 'static,
            {
                let src_cow_ref: &Cow<'_, T> = unsafe { src.get::<Cow<'static, T>>() };
                let cloned = src_cow_ref.clone();
                let dst_cow_ref: &mut Cow<'_, T> = unsafe { dst.as_mut::<Cow<'static, T>>() };
                *dst_cow_ref = cloned;
            }

            /// Default for `Cow<T>` - creates `Cow::Owned(T::Owned::default())`
            /// by checking if T::Owned supports default at runtime via its shape.
            ///
            /// # Safety
            /// dst must be valid for writes
            unsafe fn default_in_place<T: ?Sized + ToOwned + 'static>(dst: OxPtrMut)
            where
                T::Owned: Facet<'static> + 'static,
            {
                // Get the Owned type's shape from the second type param
                let cow_shape = dst.shape();
                let type_params = cow_shape.type_params;
                if type_params.len() < 2 {
                    return;
                }

                let owned_shape = type_params[1].shape;

                // Allocate space for T::Owned and call default_in_place
                let owned_layout = match owned_shape.layout.sized_layout() {
                    Ok(layout) => layout,
                    Err(_) => return,
                };

                let owned_ptr = unsafe { alloc::alloc::alloc(owned_layout) };
                if owned_ptr.is_null() {
                    return;
                }

                let owned_uninit = crate::PtrMut::new(owned_ptr);
                if unsafe { owned_shape.call_default_in_place(owned_uninit) }.is_none() {
                    // Default not supported, deallocate and return
                    unsafe { alloc::alloc::dealloc(owned_ptr, owned_layout) };
                    return;
                }

                // Move the constructed T::Owned out of the temporary allocation.
                // This leaves `owned_ptr` uninitialized, so we must deallocate the backing storage.
                let owned_value: T::Owned =
                    unsafe { core::ptr::read(owned_ptr as *const T::Owned) };
                unsafe { alloc::alloc::dealloc(owned_ptr, owned_layout) };

                // IMPORTANT: `default_in_place` must be valid for writes to potentially-uninitialized
                // destination memory. Do not create `&mut Cow` here (that would assume initialization).
                let out: *mut Cow<'static, T> =
                    unsafe { dst.ptr().as_ptr::<Cow<'static, T>>() as *mut Cow<'static, T> };
                unsafe { core::ptr::write(out, Cow::Owned(owned_value)) };
            }

            unsafe fn truthy<'facet, T>(ptr: PtrConst) -> bool
            where
                T: ?Sized + ToOwned + 'static + Facet<'facet>,
                T::Owned: Facet<'static> + 'static,
            {
                let cow_ref: &Cow<'_, T> = unsafe { ptr.get::<Cow<'static, T>>() };
                let inner_shape = <T as Facet<'facet>>::SHAPE;
                if let Some(truthy) = inner_shape.truthiness_fn() {
                    let inner: &T = cow_ref.as_ref();
                    unsafe { truthy(PtrConst::new(inner as *const T)) }
                } else {
                    false
                }
            }

            TypeOpsIndirect {
                drop_in_place: drop_in_place::<T>,
                default_in_place: Some(default_in_place::<T>),
                clone_into: Some(clone_into::<T>),
                is_truthy: Some(truthy::<'facet, T>),
            }
        }

        const fn build_type_name<'a, T: Facet<'a> + ?Sized + ToOwned>() -> TypeNameFn {
            fn type_name_impl<'a, T: Facet<'a> + ?Sized + ToOwned>(
                _shape: &'static Shape,
                f: &mut core::fmt::Formatter<'_>,
                opts: TypeNameOpts,
            ) -> core::fmt::Result {
                write!(f, "Cow")?;
                if let Some(opts) = opts.for_children() {
                    write!(f, "<")?;
                    T::SHAPE.write_type_name(f, opts)?;
                    write!(f, ">")?;
                } else {
                    write!(f, "<…>")?;
                }
                Ok(())
            }
            type_name_impl::<T>
        }

        ShapeBuilder::for_sized::<Cow<'a, T>>("Cow")
            .type_name(build_type_name::<T>())
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        borrow_fn: Some(cow_borrow::<T>),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(T::SHAPE),
                weak: None,
                strong: None,
                flags: PointerFlags::EMPTY,
                known: Some(KnownPointer::Cow),
            }))
            .type_params(&[
                TypeParam {
                    name: "T",
                    shape: T::SHAPE,
                },
                TypeParam {
                    name: "Owned",
                    shape: <T::Owned>::SHAPE,
                },
            ])
            .inner(T::SHAPE)
            .vtable_indirect(&const { build_cow_vtable::<T>() })
            .type_ops_indirect(&const { build_cow_type_ops::<'a, T>() })
            .build()
    };
}

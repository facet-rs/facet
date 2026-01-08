use core::{cmp::Ordering, fmt, mem};

use crate::{
    Def, Facet, FieldBuilder, HashProxy, OxPtrConst, OxPtrMut, OxRef, PtrConst, PtrMut, Repr,
    Shape, ShapeBuilder, StructKind, StructType, Type, TypeNameOpts, TypeOpsIndirect, UserType,
    VTableIndirect,
};

/// Debug for tuples - formats as tuple literal
unsafe fn tuple_debug(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let shape = ox.shape();
    let ty = match shape.ty {
        Type::User(UserType::Struct(ref st)) => st,
        _ => return None,
    };

    let ptr = ox.ptr();
    let mut tuple = f.debug_tuple("");

    for field in ty.fields {
        // SAFETY: Field offset is valid, and the caller guarantees the OxPtrConst
        // points to a valid tuple.
        let field_ptr = unsafe { PtrConst::new(ptr.as_byte_ptr().add(field.offset)) };
        let field_ox = unsafe { OxRef::new(field_ptr, field.shape.get()) };
        tuple.field(&field_ox);
    }

    Some(tuple.finish())
}

/// Hash for tuples - hashes each element
unsafe fn tuple_hash(ox: OxPtrConst, hasher: &mut HashProxy<'_>) -> Option<()> {
    let shape = ox.shape();
    let ty = match shape.ty {
        Type::User(UserType::Struct(ref st)) => st,
        _ => return None,
    };

    let ptr = ox.ptr();

    for field in ty.fields {
        let field_ptr = unsafe { PtrConst::new(ptr.as_byte_ptr().add(field.offset)) };
        let field_shape = field.shape.get();
        unsafe { field_shape.call_hash(field_ptr, hasher)? };
    }

    Some(())
}

/// PartialEq for tuples
unsafe fn tuple_partial_eq(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    let shape = a.shape();
    let ty = match shape.ty {
        Type::User(UserType::Struct(ref st)) => st,
        _ => return None,
    };

    let a_ptr = a.ptr();
    let b_ptr = b.ptr();

    for field in ty.fields {
        let a_field = unsafe { PtrConst::new(a_ptr.as_byte_ptr().add(field.offset)) };
        let b_field = unsafe { PtrConst::new(b_ptr.as_byte_ptr().add(field.offset)) };
        let field_shape = field.shape.get();
        if !unsafe { field_shape.call_partial_eq(a_field, b_field)? } {
            return Some(false);
        }
    }

    Some(true)
}

/// PartialOrd for tuples
unsafe fn tuple_partial_cmp(a: OxPtrConst, b: OxPtrConst) -> Option<Option<Ordering>> {
    let shape = a.shape();
    let ty = match shape.ty {
        Type::User(UserType::Struct(ref st)) => st,
        _ => return None,
    };

    let a_ptr = a.ptr();
    let b_ptr = b.ptr();

    for field in ty.fields {
        let a_field = unsafe { PtrConst::new(a_ptr.as_byte_ptr().add(field.offset)) };
        let b_field = unsafe { PtrConst::new(b_ptr.as_byte_ptr().add(field.offset)) };
        let field_shape = field.shape.get();
        match unsafe { field_shape.call_partial_cmp(a_field, b_field)? } {
            Some(Ordering::Equal) => continue,
            other => return Some(other),
        }
    }

    Some(Some(Ordering::Equal))
}

/// Ord for tuples
unsafe fn tuple_cmp(a: OxPtrConst, b: OxPtrConst) -> Option<Ordering> {
    let shape = a.shape();
    let ty = match shape.ty {
        Type::User(UserType::Struct(ref st)) => st,
        _ => return None,
    };

    let a_ptr = a.ptr();
    let b_ptr = b.ptr();

    for field in ty.fields {
        let a_field = unsafe { PtrConst::new(a_ptr.as_byte_ptr().add(field.offset)) };
        let b_field = unsafe { PtrConst::new(b_ptr.as_byte_ptr().add(field.offset)) };
        let field_shape = field.shape.get();
        match unsafe { field_shape.call_cmp(a_field, b_field)? } {
            Ordering::Equal => continue,
            other => return Some(other),
        }
    }

    Some(Ordering::Equal)
}

/// Drop for tuples
unsafe fn tuple_drop(ox: OxPtrMut) {
    let shape = ox.shape();
    let ty = match shape.ty {
        Type::User(UserType::Struct(ref st)) => st,
        _ => return,
    };

    let ptr = ox.ptr();

    for field in ty.fields {
        let field_ptr = unsafe { PtrMut::new((ptr.as_byte_ptr() as *mut u8).add(field.offset)) };
        let field_shape = field.shape.get();
        unsafe { field_shape.call_drop_in_place(field_ptr) };
    }
}

// Shared vtable for all tuples
const TUPLE_VTABLE: VTableIndirect = VTableIndirect {
    display: None,
    debug: Some(tuple_debug),
    hash: Some(tuple_hash),
    invariants: None,
    parse: None,
    parse_bytes: None,
    try_from: None,
    try_into_inner: None,
    try_borrow_inner: None,
    partial_eq: Some(tuple_partial_eq),
    partial_cmp: Some(tuple_partial_cmp),
    cmp: Some(tuple_cmp),
};

// Type operations for all tuples
static TUPLE_TYPE_OPS: TypeOpsIndirect = TypeOpsIndirect {
    drop_in_place: tuple_drop,
    default_in_place: None,
    clone_into: None,
    is_truthy: None,
};

/// Type-erased type_name for tuples - reads field types from the shape
fn tuple_type_name(
    shape: &'static Shape,
    f: &mut fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> fmt::Result {
    let st = match &shape.ty {
        Type::User(UserType::Struct(st)) if st.kind == StructKind::Tuple => st,
        _ => return write!(f, "(?)"),
    };

    write!(f, "(")?;
    // Use for_children() for the element types since they're nested inside the tuple
    let child_opts = opts.for_children();
    for (i, field) in st.fields.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        if let Some(opts) = child_opts {
            field.shape.get().write_type_name(f, opts)?;
        } else {
            write!(f, "…")?;
        }
    }
    // Trailing comma for single-element tuples
    if st.fields.len() == 1 {
        write!(f, ",")?;
    }
    write!(f, ")")?;
    Ok(())
}

macro_rules! impl_facet_for_tuple {
    // Used to implement the next bigger tuple type, by taking the next typename & associated index
    // out of `remaining`, if it exists.
    {
        continue from ($($elems:ident.$idx:tt,)+),
        remaining ()
    } => {};
    {
        continue from ($($elems:ident.$idx:tt,)+),
        remaining ($next:ident.$nextidx:tt, $($remaining:ident.$remainingidx:tt,)*)
    } => {
        impl_facet_for_tuple! {
            impl ($($elems.$idx,)+ $next.$nextidx,),
            remaining ($($remaining.$remainingidx,)*)
        }
    };
    // Actually generate the trait implementation, and keep the remaining possible elements around
    {
        impl ($($elems:ident.$idx:tt,)+),
        remaining ($($remaining:ident.$remainingidx:tt,)*)
    } => {
        unsafe impl<'a $(, $elems)+> Facet<'a> for ($($elems,)+)
        where
            $($elems: Facet<'a>,)+
        {
            const SHAPE: &'static Shape = &const {
                ShapeBuilder::for_sized::<Self>(
                    if 1 == [$($elems::SHAPE),+].len() {
                        "(_,)"
                    } else {
                        "(…)"
                    }
                )
                .decl_id_prim()
                .type_name(tuple_type_name)
                .ty(Type::User(UserType::Struct(StructType {
                    repr: Repr::default(),
                    kind: StructKind::Tuple,
                    fields: &const {[
                        $(FieldBuilder::new(stringify!($idx), crate::shape_of::<$elems>, mem::offset_of!(Self, $idx)).build(),)+
                    ]}
                })))
                .def(Def::Undefined)
                .vtable_indirect(&TUPLE_VTABLE)
                .type_ops_indirect(&TUPLE_TYPE_OPS)
                .build()
            };
        }

        impl_facet_for_tuple! {
            continue from ($($elems.$idx,)+),
            remaining ($($remaining.$remainingidx,)*)
        }
    };
    // The entry point into this macro, all smaller tuple types get implemented as well.
    { ($first:ident.$firstidx:tt $(, $remaining:ident.$remainingidx:tt)* $(,)?) } => {
        impl_facet_for_tuple! {
            impl ($first.$firstidx,),
            remaining ($($remaining.$remainingidx,)*)
        }
    };
}

#[cfg(feature = "tuples-12")]
impl_facet_for_tuple! {
    (T0.0, T1.1, T2.2, T3.3, T4.4, T5.5, T6.6, T7.7, T8.8, T9.9, T10.10, T11.11)
}

#[cfg(not(feature = "tuples-12"))]
impl_facet_for_tuple! {
    (T0.0, T1.1, T2.2, T3.3)
}

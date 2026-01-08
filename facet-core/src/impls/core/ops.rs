use crate::{
    Def, Facet, FieldBuilder, OxPtrConst, OxPtrMut, Shape, ShapeBuilder, StructType, Type,
    TypeOpsIndirect, TypeParam, UserType, VTableIndirect,
};

/// Debug for `Range<Idx>`
unsafe fn range_debug<Idx>(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let this = unsafe { ox.get::<core::ops::Range<Idx>>() };

    Some((|| {
        let fields = match &ox.shape().ty {
            Type::User(UserType::Struct(s)) => &s.fields,
            _ => return Err(core::fmt::Error),
        };

        let start_shape = fields[0].shape.get();
        let end_shape = fields[1].shape.get();

        let start_ptr = crate::PtrConst::new(&this.start);
        if let Some(result) = unsafe { start_shape.call_debug(start_ptr, f) } {
            result?;
        } else {
            return Err(core::fmt::Error);
        }

        write!(f, "..")?;

        let end_ptr = crate::PtrConst::new(&this.end);
        if let Some(result) = unsafe { end_shape.call_debug(end_ptr, f) } {
            result?;
        } else {
            return Err(core::fmt::Error);
        }

        Ok(())
    })())
}

/// Drop for `Range<Idx>`
unsafe fn range_drop<Idx>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_byte_ptr() as *mut core::ops::Range<Idx>);
    }
}

unsafe impl<'a, Idx: Facet<'a>> Facet<'a> for core::ops::Range<Idx> {
    const SHAPE: &'static Shape = &const {
        const fn build_vtable<Idx>() -> VTableIndirect {
            VTableIndirect {
                display: None,
                debug: Some(range_debug::<Idx>),
                hash: None,
                invariants: None,
                parse: None,
                parse_bytes: None,
                try_from: None,
                try_into_inner: None,
                try_borrow_inner: None,
                partial_eq: None,
                partial_cmp: None,
                cmp: None,
            }
        }

        const fn build_type_ops<Idx>() -> TypeOpsIndirect {
            TypeOpsIndirect {
                drop_in_place: range_drop::<Idx>,
                default_in_place: None,
                clone_into: None,
                is_truthy: None,
            }
        }

        ShapeBuilder::for_sized::<core::ops::Range<Idx>>("Range")
            .decl_id(crate::DeclId::new(crate::decl_id_hash("Range")))
            .ty(Type::User(UserType::Struct(StructType {
                kind: crate::StructKind::Struct,
                repr: crate::Repr::default(),
                fields: &const {
                    [
                        FieldBuilder::new(
                            "start",
                            crate::shape_of::<Idx>,
                            core::mem::offset_of!(core::ops::Range<Idx>, start),
                        )
                        .build(),
                        FieldBuilder::new(
                            "end",
                            crate::shape_of::<Idx>,
                            core::mem::offset_of!(core::ops::Range<Idx>, end),
                        )
                        .build(),
                    ]
                },
            })))
            .def(Def::Scalar)
            .type_params(&[TypeParam {
                name: "Idx",
                shape: Idx::SHAPE,
            }])
            .vtable_indirect(&const { build_vtable::<Idx>() })
            .type_ops_indirect(&const { build_type_ops::<Idx>() })
            .build()
    };
}

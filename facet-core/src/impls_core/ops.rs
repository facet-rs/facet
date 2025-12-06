use crate::{
    Def, Facet, Field, FieldFlags, Shape, ShapeRef, StructType, Type, VTableView, ValueVTable,
};
use core::mem;

unsafe impl<'a, Idx: Facet<'a>> Facet<'a> for core::ops::Range<Idx> {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: ValueVTable::builder(|f, opts| {
                write!(f, "{}", Self::SHAPE.type_identifier)?;
                if let Some(opts) = opts.for_children() {
                    write!(f, "<")?;
                    Idx::SHAPE.vtable.type_name()(f, opts)?;
                    write!(f, ">")?;
                } else {
                    write!(f, "<…>")?;
                }
                Ok(())
            })
            .drop_in_place(ValueVTable::drop_in_place_for::<Self>())
            .debug_opt({
                if Idx::SHAPE.vtable.has_debug() {
                    Some(|this, f| {
                        let this = unsafe { this.get::<core::ops::Range<Idx>>() };
                        (<VTableView<Idx>>::of().debug().unwrap())((&this.start).into(), f)?;
                        write!(f, "..")?;
                        (<VTableView<Idx>>::of().debug().unwrap())((&this.end).into(), f)?;
                        Ok(())
                    })
                } else {
                    None
                }
            })
            .build(),
            ty: Type::User(crate::UserType::Struct(StructType {
                kind: crate::StructKind::Struct,
                repr: crate::Repr::default(),
                fields: &const {
                    [
                        Field {
                            name: "start",
                            shape: ShapeRef::Static(Idx::SHAPE),
                            offset: mem::offset_of!(core::ops::Range<Idx>, start),
                            flags: FieldFlags::empty(),
                            rename: None,
                            alias: None,
                            attributes: &[],
                            doc: &[],
                        },
                        Field {
                            name: "end",
                            shape: ShapeRef::Static(Idx::SHAPE),
                            offset: mem::offset_of!(core::ops::Range<Idx>, end),
                            flags: FieldFlags::empty(),
                            rename: None,
                            alias: None,
                            attributes: &[],
                            doc: &[],
                        },
                    ]
                },
            })),
            def: Def::Scalar,
            type_identifier: "Range",
            type_params: &[crate::TypeParam {
                name: "Idx",
                shape: Idx::SHAPE,
            }],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
        }
    };
}

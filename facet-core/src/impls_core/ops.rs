use crate::{ConstTypeId, Facet, Field, Shape, StructType, Type, VTableView, ValueVTable};
use core::{alloc::Layout, mem};

unsafe impl<'a, Idx: Facet<'a>> Facet<'a> for core::ops::Range<Idx> {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable(
                ValueVTable::builder::<Self>()
                    .type_name(|f, opts| {
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
                    .debug({
                        if Idx::SHAPE.vtable.has_debug() {
                            Some(|this, f| {
                                let this = this.get();
                                (<VTableView<Idx>>::of().debug().unwrap())(
                                    (&this.start).into(),
                                    f,
                                )?;
                                write!(f, "..")?;
                                (<VTableView<Idx>>::of().debug().unwrap())((&this.end).into(), f)?;
                                Ok(())
                            })
                        } else {
                            None
                        }
                    })
                    .build(),
            )
            .type_identifier("Range")
            .type_params(&[crate::TypeParam {
                name: "Idx",
                shape: Idx::SHAPE,
            }])
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .ty(Type::User(crate::UserType::Struct(
                StructType::builder()
                    .kind(crate::StructKind::Struct)
                    .repr(crate::Repr::default())
                    .fields(
                        &const {
                            [
                                Field::builder()
                                    .name("start")
                                    .shape(|| Idx::SHAPE)
                                    .offset(mem::offset_of!(core::ops::Range<Idx>, start))
                                    .build(),
                                Field::builder()
                                    .name("end")
                                    .shape(|| Idx::SHAPE)
                                    .offset(mem::offset_of!(core::ops::Range<Idx>, end))
                                    .build(),
                            ]
                        },
                    )
                    .build(),
            )))
            .build()
    };
}

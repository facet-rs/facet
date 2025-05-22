use crate::{ConstTypeId, Facet, Field, Shape, StructType, Type, VTableView, ValueVTable};
use core::{alloc::Layout, mem};

unsafe impl<'a, Idx: Facet<'a>> Facet<'a> for core::ops::Range<Idx> {
    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_params(&[crate::TypeParam {
                name: "Idx",
                shape: || Idx::SHAPE,
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
                                    .shape(Idx::SHAPE)
                                    .offset(mem::offset_of!(core::ops::Range<Idx>, start))
                                    .build(),
                                Field::builder()
                                    .name("end")
                                    .shape(Idx::SHAPE)
                                    .offset(mem::offset_of!(core::ops::Range<Idx>, end))
                                    .build(),
                            ]
                        },
                    )
                    .build(),
            )))
            .build()
    };

    const VTABLE: &'static ValueVTable = &const {
        ValueVTable::builder::<Self>()
            .type_name(|f, opts| {
                write!(f, "Range")?;
                if let Some(opts) = opts.for_children() {
                    write!(f, "<")?;
                    (Idx::SHAPE.vtable.type_name)(f, opts)?;
                    write!(f, ">")?;
                } else {
                    write!(f, "<…>")?;
                }
                Ok(())
            })
            .debug(|| {
                if Idx::SHAPE.is_debug() {
                    Some(|this, f| {
                        (<VTableView<Idx>>::of().debug().unwrap())(&this.start, f)?;
                        write!(f, "..")?;
                        (<VTableView<Idx>>::of().debug().unwrap())(&this.end, f)?;
                        Ok(())
                    })
                } else {
                    None
                }
            })
            .build()
    };
}

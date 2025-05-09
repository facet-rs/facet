use crate::{
    ConstTypeId, Def, Facet, Field, Shape, StructDef, StructKind, VTableView, ValueVTable,
};
use core::{alloc::Layout, mem};

unsafe impl<'a, Idx: Facet<'a>> Facet<'a> for core::ops::Range<Idx> {
    const SHAPE: &'static crate::Shape = &const {
        Shape::builder_for_sized::<Self>()
            .type_params(&[crate::TypeParam {
                name: "Idx",
                shape: || Idx::SHAPE,
            }])
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .vtable(
                &const {
                    let mut builder = ValueVTable::builder::<Self>().type_name(|f, opts| {
                        write!(f, "Range")?;
                        if let Some(opts) = opts.for_children() {
                            write!(f, "<")?;
                            (Idx::SHAPE.vtable.type_name)(f, opts)?;
                            write!(f, ">")?;
                        } else {
                            write!(f, "<…>")?;
                        }
                        Ok(())
                    });

                    if Idx::SHAPE.is_debug() {
                        builder = builder.debug(|this, f| {
                            (<VTableView<Idx>>::of().debug().unwrap())(&this.start, f)?;
                            write!(f, "..")?;
                            (<VTableView<Idx>>::of().debug().unwrap())(&this.end, f)?;
                            Ok(())
                        });
                    }

                    builder.build()
                },
            )
            .def(Def::Struct(
                StructDef::builder()
                    .kind(StructKind::Struct)
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
                                    .offset(mem::offset_of!(core::ops::Range<Idx>, start))
                                    .build(),
                            ]
                        },
                    )
                    .build(),
            ))
            .build()
    };
}

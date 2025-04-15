use core::alloc::Layout;

use crate::{
    ConstTypeId, Def, Facet, OptionDef, OptionVTable, PtrConst, Shape, value_vtable_inner,
};

unsafe impl<T: Facet> Facet for Option<T> {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Option(
                OptionDef::builder()
                    .t(T::SHAPE)
                    .vtable(
                        const {
                            &OptionVTable {
                                is_some_fn: |option| unsafe { option.get::<Option<T>>().is_some() },
                                get_value_fn: |option| unsafe {
                                    option
                                        .get::<Option<T>>()
                                        .as_ref()
                                        .map(|t| PtrConst::new(t as *const T))
                                },
                                init_some_fn: |option, value| unsafe {
                                    option.put(Option::Some(value.read::<T>()))
                                },
                                init_none_fn: |option| unsafe { option.put(<Option<T>>::None) },
                                replace_with_fn: |option, value| unsafe {
                                    let option = option.as_mut::<Option<T>>();
                                    match value {
                                        Some(value) => option.replace(value.read::<T>()),
                                        None => option.take(),
                                    };
                                },
                            }
                        },
                    )
                    .build(),
            ))
            .vtable(
                &const {
                    let mut vtable = value_vtable_inner!(core::option::Option<T>, |f, opts| {
                        write!(f, "Option")?;
                        if let Some(opts) = opts.for_children() {
                            write!(f, "<")?;
                            (T::SHAPE.vtable.type_name)(f, opts)?;
                            write!(f, ">")?;
                        } else {
                            write!(f, "<…>")?;
                        }
                        Ok(())
                    });

                    if T::SHAPE.is_debug() {
                        vtable.debug = Some(|this, f| {
                            let this = unsafe { this.get::<Self>() };
                            if let Some(value) = &this {
                                write!(f, "Some(")?;
                                unsafe {
                                    (T::SHAPE.vtable.debug.unwrap_unchecked())(
                                        PtrConst::new(value),
                                        f,
                                    )?;
                                }
                                write!(f, ")")?;
                            } else {
                                write!(f, "None")?;
                            }
                            Ok(())
                        });
                    }

                    vtable
                },
            )
            .build()
    };
}

use core::{alloc::Layout, mem::MaybeUninit};

use crate::{
    ConstTypeId, Def, Facet, OptionDef, OptionVTable, PtrConst, PtrUninit, Shape,
    value_vtable_inner,
};

unsafe impl<'a, T: Facet<'a>> Facet<'a> for Option<T> {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .def(Def::Option(
                OptionDef::builder()
                    .t(|| T::SHAPE)
                    .vtable(
                        const {
                            &OptionVTable::builder()
                                .is_some(|option| unsafe { option.get::<Option<T>>().is_some() })
                                .get_value(|option| unsafe {
                                    option
                                        .get::<Option<T>>()
                                        .as_ref()
                                        .map(|t| PtrConst::new(t as *const T))
                                })
                                .init_some(|option, value| unsafe {
                                    option.put(Option::Some(value.read::<T>()))
                                })
                                .init_none(|option| unsafe { option.put(<Option<T>>::None) })
                                .replace_with(|option, value| unsafe {
                                    let option = option.as_mut::<Option<T>>();
                                    match value {
                                        Some(value) => option.replace(value.read::<T>()),
                                        None => option.take(),
                                    };
                                })
                                .build()
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

                    if T::SHAPE.is_from_str() {
                        vtable.parse = Some(|str, target| {
                            let mut t = MaybeUninit::<T>::uninit();
                            let parse = unsafe { T::SHAPE.vtable.parse.unwrap_unchecked() };
                            let _res = unsafe { (parse)(str, PtrUninit::new(t.as_mut_ptr()))? };
                            // res points to t so we can't drop it yet. the option is not initialized though
                            unsafe {
                                target.put(Some(t.assume_init()));
                                Ok(target.assume_init())
                            }
                        });
                    }

                    vtable
                },
            )
            .build()
    };
}

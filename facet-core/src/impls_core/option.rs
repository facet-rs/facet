use core::alloc::Layout;

use crate::{ConstTypeId, Def, Facet, OpaqueConst, OptionDef, OptionVTable, Shape, value_vtable};

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
                                is_some_fn: |option| unsafe {
                                    option.as_ref::<Option<T>>().is_some()
                                },
                                get_value_fn: |option| unsafe {
                                    option
                                        .as_ref::<Option<T>>()
                                        .as_ref()
                                        .map(|t| OpaqueConst::new(t as *const T))
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
            .vtable(value_vtable!(core::option::Option<T>, |f, _opts| write!(
                f,
                "Option"
            )))
            .build()
    };
}

use core::{fmt, mem};

use crate::{
    Characteristic, Def, Facet, Repr, Shape, StructKind, StructType, Type, TypeNameOpts, UserType,
    ValueVTable, types::field_in_type,
};

#[inline(always)]
pub fn write_type_name_list(
    f: &mut fmt::Formatter<'_>,
    opts: TypeNameOpts,
    open: &'static str,
    delimiter: &'static str,
    close: &'static str,
    shapes: &'static [&'static Shape],
) -> fmt::Result {
    f.pad(open)?;
    if let Some(opts) = opts.for_children() {
        for (index, shape) in shapes.iter().enumerate() {
            if index > 0 {
                f.pad(delimiter)?;
            }
            shape.write_type_name(f, opts)?;
        }
    } else {
        write!(f, "…")?;
    }
    f.pad(close)?;
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
    // Handle commas correctly for the debug implementation
    { debug on $f:ident { $first:stmt; } } => {
        write!($f, "(")?;
        $first
        write!($f, ",)")
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
                Shape {
                    id: Shape::id_of::<Self>(),
                    layout: Shape::layout_of::<Self>(),
                    vtable: ValueVTable {
                        type_name: |f, opts| {
                            write_type_name_list(f, opts, "(", ", ", ")", &[$($elems::SHAPE),+])
                        },
                        drop_in_place: ValueVTable::drop_in_place_for::<Self>(),
                        default_in_place: {
                            let elem_shapes = const { &[$($elems::SHAPE),+] };
                            if Characteristic::all_default(elem_shapes) {
                                Some(|dst| {
                                    $(
                                        unsafe {
                                            ($elems::SHAPE.vtable.default_in_place.unwrap())(
                                                dst.field_uninit_at(mem::offset_of!(Self, $idx))
                                            );
                                        }
                                    )+

                                    unsafe { dst.assume_init().into() }
                                })
                            } else {
                                None
                            }
                        },
                        ..ValueVTable::new(|_, _| Ok(()))
                    },
                    ty: Type::User(UserType::Struct(StructType {
                        repr: Repr::default(),
                        kind: StructKind::Tuple,
                        fields: &const {[
                            $(field_in_type!(Self, $idx, $elems),)+
                        ]}
                    })),
                    def: Def::Undefined,
                    type_identifier: const {
                        let fields = [
                            $(field_in_type!(Self, $idx, $elems),)+
                        ];
                        if fields.len() == 1 {
                            "(_)"
                        } else {
                            "(…)"
                        }
                    },
                    type_params: &[],
                    doc: &[],
                    attributes: &[],
                    type_tag: None,
                    inner: None,
                }
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

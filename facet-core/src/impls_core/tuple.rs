use core::{cmp::Ordering, fmt, mem};

use crate::{
    Characteristic, Facet, MarkerTraits, SequenceType, Shape, TupleType, Type, TypeNameOpts,
    VTableView, ValueVTable, types::field_in_type,
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
        write!(f, "⋯")?;
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
    { debug on $f:ident { $first:stmt; $($stmt:stmt;)+ } } => {
        write!($f, "(")?;
        $first
        $(
            write!($f, ", ")?;
            $stmt
        )+
        write!($f, ")")
    };
    // Common structure of eq, partial_ord & ord
    { ord on ($($elems:ident.$idx:tt,)+), $cmp:ident($a:ident, $b:ident), eq = $eq:expr } => {{
        $(
            unsafe {
                let ordering = (<VTableView<$elems>>::of().$cmp().unwrap_unchecked())(
                    &$a.$idx,
                    &$b.$idx,
                );

                if ordering != $eq {
                    return ordering;
                }
            }
        )+

        $eq
    }};
    // Actually generate the trait implementation, and keep the remaining possible elements around
    {
        impl ($($elems:ident.$idx:tt,)+),
        remaining ($($remaining:ident.$remainingidx:tt,)*)
    } => {
        unsafe impl<'a $(, $elems)+> Facet<'a> for ($($elems,)+)
        where
            $($elems: Facet<'a>,)+
        {
            const VTABLE: &'static ValueVTable = &const {
                let mut builder = ValueVTable::builder::<Self>()
                    .type_name(|f, opts| {
                        write_type_name_list(f, opts, "(", ", ", ")", &[$($elems::SHAPE),+])
                    })
                    .drop_in_place(|data| unsafe { data.drop_in_place::<Self>() })
                    .marker_traits(
                        MarkerTraits::all()
                            $(.intersection($elems::SHAPE.vtable.marker_traits))+
                    );

                let elem_shapes = const { &[$($elems::SHAPE),+] };
                if Characteristic::Debug.all(elem_shapes) {
                    builder = builder.debug(|value, f| {
                        impl_facet_for_tuple! {
                            debug on f {
                                $(
                                    (<VTableView<$elems>>::of().debug().unwrap())(
                                        &value.$idx,
                                        f,
                                    )?;
                                )+
                            }
                        }
                    });
                }

                if Characteristic::Default.all(elem_shapes) {
                    builder = builder.default_in_place(|mut dst| {
                        $(
                            unsafe {
                                (<VTableView<$elems>>::of().default_in_place().unwrap())(
                                    dst.field_uninit_at(mem::offset_of!(Self, $idx))
                                );
                            }
                        )+

                        unsafe { dst.assume_init() }
                    });
                }

            //     if Characteristic::Clone.all(elem_shapes) {
            //          builder = builder.clone_into(|src, dst| {
            //             $({
            //                 let offset = mem::offset_of!(Self, $idx);
            //                 unsafe {
            //                     (<VTableView<$elems>>::of().clone_into().unwrap())(
            //                         src.field(offset),
            //                         dst.field_uninit_at(offset),
            //                     );
            //                 }
            //             })+

            //             unsafe { dst.assume_init() }
            //         });
            //    }

                if Characteristic::PartialEq.all(elem_shapes) {
                    builder = builder.eq(|a, b| impl_facet_for_tuple! {
                        ord on ($($elems.$idx,)+),
                        eq(a, b),
                        eq = true
                    });
                }

                if Characteristic::PartialOrd.all(elem_shapes) {
                    builder = builder.partial_ord(|a, b| impl_facet_for_tuple! {
                        ord on ($($elems.$idx,)+),
                        partial_ord(a, b),
                        eq = Some(Ordering::Equal)
                    });
                }

                if Characteristic::Ord.all(elem_shapes) {
                    builder = builder.ord(|a, b| impl_facet_for_tuple! {
                        ord on ($($elems.$idx,)+),
                        ord(a, b),
                        eq = Ordering::Equal
                    });
                }

                if Characteristic::Hash.all(elem_shapes) {
                    builder = builder.hash(|value, hasher_this, hasher_write_fn| {
                        $(
                            (<VTableView<$elems>>::of().hash().unwrap())(
                                &value.$idx,
                                hasher_this,
                                hasher_write_fn,
                            );
                        )+
                   });
                }

                builder.build()
            };

            const SHAPE: &'static Shape<'static> = &const {
                Shape::builder_for_sized::<Self>()
                    .ty(Type::Sequence(SequenceType::Tuple(TupleType {
                        fields: &const {[
                            $(field_in_type!(Self, $idx),)+
                        ]}
                    })))
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

impl_facet_for_tuple! {
    (T0.0, T1.1, T2.2, T3.3, T4.4, T5.5, T6.6, T7.7, T8.8, T9.9, T10.10, T11.11)
}

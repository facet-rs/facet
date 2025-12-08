#![cfg(feature = "nonzero")]

use core::num::NonZero;

use crate::{
    Def, Facet, FieldBuilder, Repr, Shape, ShapeBuilder, StructKind, StructType, Type, UserType,
    VTableDirect, vtable_direct,
};

macro_rules! impl_facet_for_nonzero {
    ($type:ty) => {
        unsafe impl<'a> Facet<'a> for NonZero<$type> {
            const SHAPE: &'static Shape = &const {
                // NonZero implements Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, FromStr
                // but NOT Default
                const VTABLE: VTableDirect = vtable_direct!(NonZero<$type> =>
                    FromStr,
                    Display,
                    Debug,
                    Hash,
                    PartialEq,
                    PartialOrd,
                    Ord,
                    [invariants = {
                        fn invariants(v: &NonZero<$type>) -> Result<(), alloc::string::String> {
                            // NonZero<T>'s internal representation guarantees the value is non-zero
                            // but when constructed via transparent deserialization through the inner
                            // type, we need to verify the invariant
                            let inner_value: $type = v.get();
                            if inner_value == 0 {
                                return Err(alloc::string::String::from("NonZero value cannot be zero"));
                            }
                            Ok(())
                        }
                        invariants
                    }],
                    // try_from converts from the inner type (e.g., u64) to NonZero<u64>
                    // This is called by facet-reflect's end() when finishing a begin_inner() frame
                    [try_from = {
                        /// # Safety
                        /// `src` must point to a valid `$type`, `dst` must be valid for writes
                        unsafe fn try_from(src: &$type, dst: *mut NonZero<$type>) -> Result<(), alloc::string::String> {
                            unsafe {
                                let value: $type = core::ptr::read(src);
                                match NonZero::new(value) {
                                    Some(nonzero) => {
                                        dst.write(nonzero);
                                        Ok(())
                                    }
                                    None => Err(alloc::string::String::from("NonZero value cannot be zero")),
                                }
                            }
                        }
                        // Transmute to match the expected signature (src type becomes the target type)
                        // This is safe because the vtable is type-erased and the actual types come from
                        // the shape's inner field
                        unsafe {
                            core::mem::transmute::<
                                unsafe fn(&$type, *mut NonZero<$type>) -> Result<(), alloc::string::String>,
                                unsafe fn(&NonZero<$type>, *mut NonZero<$type>) -> Result<(), alloc::string::String>,
                            >(try_from)
                        }
                    }],
                );

                ShapeBuilder::for_sized::<NonZero<$type>>("NonZero")
                    .ty(Type::User(UserType::Struct(StructType {
                        repr: Repr::transparent(),
                        kind: StructKind::TupleStruct,
                        fields: &const { [FieldBuilder::new("0", crate::shape_of::<$type>, 0).build()] },
                    })))
                    .inner(<$type as Facet>::SHAPE)
                    .def(Def::Scalar)
                    .vtable_direct(&VTABLE)
                    .eq()
                    .copy()
                    .send()
                    .sync()
                    .build()
            };
        }
    };
}

impl_facet_for_nonzero!(u8);
impl_facet_for_nonzero!(i8);
impl_facet_for_nonzero!(u16);
impl_facet_for_nonzero!(i16);
impl_facet_for_nonzero!(u32);
impl_facet_for_nonzero!(i32);
impl_facet_for_nonzero!(u64);
impl_facet_for_nonzero!(i64);
impl_facet_for_nonzero!(u128);
impl_facet_for_nonzero!(i128);
impl_facet_for_nonzero!(usize);
impl_facet_for_nonzero!(isize);

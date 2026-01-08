#![cfg(feature = "nonzero")]

use core::num::NonZero;

use crate::{
    Def, Facet, FieldBuilder, PtrConst, Repr, Shape, ShapeBuilder, StructKind, StructType,
    TryFromOutcome, Type, UserType, VTableDirect, vtable_direct,
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
                        /// `dst` must be valid for writes, `src` must point to valid data
                        unsafe fn try_from(
                            dst: *mut NonZero<$type>,
                            src_shape: &'static Shape,
                            src: PtrConst,
                        ) -> TryFromOutcome {
                            // Only accept the inner type
                            if src_shape.type_identifier != stringify!($type) {
                                return TryFromOutcome::Unsupported;
                            }
                            unsafe {
                                // Consume the source value
                                let value: $type = core::ptr::read(src.as_byte_ptr() as *const $type);
                                match NonZero::new(value) {
                                    Some(nonzero) => {
                                        dst.write(nonzero);
                                        TryFromOutcome::Converted
                                    }
                                    None => TryFromOutcome::Failed("NonZero value cannot be zero".into()),
                                }
                            }
                        }
                        try_from
                    }],
                    // try_borrow_inner borrows the inner value for serialization
                    // NonZero<T> has transparent repr, so the inner T is at the same address
                    [try_borrow_inner = {
                        /// # Safety
                        /// `ptr` must point to a valid NonZero<$type>
                        unsafe fn try_borrow_inner(ptr: *const NonZero<$type>) -> Result<crate::PtrMut, alloc::string::String> {
                            // NonZero<T> is repr(transparent), so we can just cast the pointer
                            Ok(crate::PtrMut::new(ptr as *mut ()))
                        }
                        try_borrow_inner
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

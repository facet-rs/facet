#![cfg(feature = "fn-ptr")]

use crate::{
    Facet, FunctionAbi, FunctionPointerDef, PointerType, Shape, ShapeBuilder, Type, TypeOpsDirect,
    TypeParam, VTableDirect, Variance, type_ops_direct, vtable_direct,
};

macro_rules! impl_facet_for_fn_ptr {
    // Used to implement the next bigger `fn` type, by taking the next typename out of `remaining`,
    // if it exists.
    {
        continue from $(extern $extern:literal)? fn($($args:ident),*) -> R with $abi:expr,
        remaining ()
    } => {};
    {
        continue from $(extern $extern:literal)? fn($($args:ident),*) -> R with $abi:expr,
        remaining ($next:ident $(, $remaining:ident)*)
    } => {
        impl_facet_for_fn_ptr! {
            impl $(extern $extern)? fn($($args,)* $next) -> R with $abi,
            remaining ($($remaining),*)
        }
    };
    // Actually generate the trait implementation, and keep the remaining possible arguments around
    {
        impl $(extern $extern:literal)? fn($($args:ident),*) -> R with $abi:expr,
        remaining ($($remaining:ident),*)
    } => {
        unsafe impl<'a, $($args,)* R> Facet<'a> for $(extern $extern)? fn($($args),*) -> R
        where
            $($args: Facet<'a>,)*
            R: Facet<'a>,
        {
            const SHAPE: &'static Shape = &const {
                const fn build_vtable<$($args,)* R>() -> VTableDirect {
                    vtable_direct!($(extern $extern)? fn($($args),*) -> R =>
                        Debug,
                        PartialEq,
                        PartialOrd,
                        Ord,
                    )
                }

                const fn build_type_ops<$($args,)* R>() -> TypeOpsDirect {
                    type_ops_direct!($(extern $extern)? fn($($args),*) -> R => Clone)
                }

                ShapeBuilder::for_sized::<$(extern $extern)? fn($($args),*) -> R>("fn")
                    .ty(Type::Pointer(PointerType::Function(
                        FunctionPointerDef::new($abi, &[$($args::SHAPE),*], R::SHAPE)
                    )))
                    .type_params(&[
                        $(TypeParam { name: stringify!($args), shape: $args::SHAPE },)*
                    ])
                    .vtable_direct(&const { build_vtable::<$($args,)* R>() })
                    .type_ops_direct(&const { build_type_ops::<$($args,)* R>() })
                    // Function pointers are invariant over their lifetime parameters.
                    // Arguments are contravariant, returns are covariant, combining to invariant.
                    // See: https://github.com/facet-rs/facet/issues/1664
                    .variance(Variance::INVARIANT)
                    .eq()
                    .copy()
                    .build()
            };
        }
        impl_facet_for_fn_ptr! {
            continue from $(extern $extern)? fn($($args),*) -> R with $abi,
            remaining ($($remaining),*)
        }
    };
    // The entry point into this macro, all smaller `fn` types get implemented as well.
    {$(extern $extern:literal)? fn($($args:ident),*) -> R with $abi:expr} => {
        impl_facet_for_fn_ptr! {
            impl $(extern $extern)? fn() -> R with $abi,
            remaining ($($args),*)
        }
    };
}

impl_facet_for_fn_ptr! { fn(T0, T1, T2, T3, T4, T5) -> R with FunctionAbi::Rust }
impl_facet_for_fn_ptr! { extern "C" fn(T0, T1, T2, T3, T4, T5) -> R with FunctionAbi::C }

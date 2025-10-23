use core::{fmt, hash::Hash, ptr::fn_addr_eq};

use crate::{
    Facet, FunctionAbi, FunctionPointerDef, MarkerTraits, PointerType, Shape, Type, TypeNameOpts,
    TypeParam, ValueVTable,
};

#[inline(always)]
pub fn write_type_name_list(
    f: &mut fmt::Formatter<'_>,
    opts: TypeNameOpts,
    abi: FunctionAbi,
    params: &'static [&'static Shape],
    ret_type: &'static Shape,
) -> fmt::Result {
    if abi != FunctionAbi::Rust {
        f.pad("extern \"")?;
        if let Some(abi) = abi.as_abi_str() {
            f.pad(abi)?;
        }
        f.pad("\" ")?;
    }
    f.pad("fn")?;
    f.pad("(")?;
    if let Some(opts) = opts.for_children() {
        for (index, shape) in params.iter().enumerate() {
            if index > 0 {
                f.pad(", ")?;
            }
            shape.write_type_name(f, opts)?;
        }
    } else {
        write!(f, "…")?;
    }
    f.pad(") -> ")?;
    ret_type.write_type_name(f, opts)?;
    Ok(())
}

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
                Shape::builder_for_sized::<Self>()
                    .type_identifier("fn")
                    .vtable(
                        ValueVTable::builder::<Self>()
                            .type_name(|f, opts| {
                                write_type_name_list(f, opts, $abi, &[$($args::SHAPE),*], R::SHAPE)
                            })
                            .debug(Some(|data, f| fmt::Debug::fmt(data.get(), f)))
                            .clone_into(Some(|src, dst| unsafe { dst.put(src.get().clone()).into() }))
                            .marker_traits(
                                MarkerTraits::EQ
                                    .union(MarkerTraits::SEND)
                                    .union(MarkerTraits::SYNC)
                                    .union(MarkerTraits::COPY)
                                    .union(MarkerTraits::UNPIN)
                                    .union(MarkerTraits::UNWIND_SAFE)
                                    .union(MarkerTraits::REF_UNWIND_SAFE)
                            )
                            .partial_eq(Some(|left, right| {
                                fn_addr_eq(*left.get(), *right.get())
                            }))
                            .partial_ord(Some(|left, right| {
                                #[allow(unpredictable_function_pointer_comparisons)]
                                left.get().partial_cmp(right.get())
                            }))
                            .ord(Some(|left, right| {
                                #[allow(unpredictable_function_pointer_comparisons)]
                                left.get().cmp(right.get())
                            }))
                            .hash(Some(|value, hasher| {
                                value.get().hash(&mut { hasher })
                            }))
                            .build()
                    )
                    .type_params(&[
                        $(TypeParam { name: stringify!($args), shape: $args::SHAPE },)*
                    ])
                    .ty(Type::Pointer(PointerType::Function(({
                        FunctionPointerDef::builder()
                            .parameter_types(&const { [$($args::SHAPE),*] })
                            .return_type(R::SHAPE)
                            .abi($abi)
                            .build()
                    }))))
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

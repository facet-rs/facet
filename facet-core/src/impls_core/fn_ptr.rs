use core::{alloc::Layout, fmt};

use crate::{
    ConstTypeId, Def, Facet, FunctionAbi, FunctionPointerDef, Shape, TypeNameOpts, TypeParam,
    value_vtable,
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
        write!(f, "⋯")?;
    }
    f.pad(") -> ")?;
    ret_type.write_type_name(f, opts)?;
    Ok(())
}

macro_rules! impl_facet_for_fn_ptr {
    // Used to implement the next smaller `fn` type, `remaining` starts out with all original
    // arguments, which one by one get pulled into the left side, until only the last argument
    // remains, which then gets discarded.
    {
        continue to $(extern $extern:literal)? fn() -> R with $abi:expr,
        remaining ()
    } => {};
    {
        continue to $(extern $extern:literal)? fn($($args:ident),*) -> R with $abi:expr,
        remaining ($remaining:ident)
    } => {
        impl_facet_for_fn_ptr! { $(extern $extern)? fn($($args),*) -> R with $abi }
    };
    {
        continue to $(extern $extern:literal)? fn($($args:ident),*) -> R with $abi:expr,
        remaining ($next:ident $(, $remaining:ident)+)
    } => {
        impl_facet_for_fn_ptr! {
            continue to $(extern $extern)? fn($($args,)* $next) -> R with $abi,
            remaining ($($remaining),*)
        }
    };
    // The entry point into this macro, all smaller `fn` types get implemented as well.
    {$(extern $extern:literal)? fn($($args:ident),*) -> R with $abi:expr} => {
        unsafe impl<'a, $($args,)* R> Facet<'a> for $(extern $extern)? fn($($args),*) -> R
        where
            $($args: Facet<'a>,)*
            R: Facet<'a>,
        {
            const SHAPE: &'static Shape = &const {
                fn type_name<'a, $($args,)* R>(
                    f: &mut fmt::Formatter,
                    opts: TypeNameOpts
                ) -> fmt::Result
                where
                    $($args: Facet<'a>,)*
                    R: Facet<'a>
                {
                    write_type_name_list(
                        f,
                        opts,
                        $abi,
                        &[$($args::SHAPE),*],
                        R::SHAPE,
                    )
                }

                Shape::builder()
                    .id(ConstTypeId::of::<Self>())
                    .layout(Layout::new::<Self>())
                    // FIXME: Dont use the macro here we can generate this
                    // FIXME: type name
                    // TODO: ^ is this still applicable?
                    .vtable(value_vtable!(Self, type_name::<$($args,)* R>))
                    .type_params(&[
                        $(TypeParam { name: stringify!($args), shape: || $args::SHAPE },)*
                    ])
                    .def(Def::FunctionPointer({
                        FunctionPointerDef::builder()
                            .parameter_types(&const { [$(|| $args::SHAPE),*] })
                            .return_type(|| R::SHAPE)
                            .abi($abi)
                            .build()
                    }))
                    .build()
            };
        }

        impl_facet_for_fn_ptr! {
            continue to $(extern $extern)? fn() -> R with $abi,
            remaining ($($args),*)
        }
    };
}

impl_facet_for_fn_ptr! {fn(T0, T1, T2, T3, T4, T5) -> R with FunctionAbi::Rust}
impl_facet_for_fn_ptr! {extern "C" fn(T0, T1, T2, T3, T4, T5) -> R with FunctionAbi::C}

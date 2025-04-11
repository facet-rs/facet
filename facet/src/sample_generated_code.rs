//! This defines a few types showcasing various features of the Facet derive macro.
#![allow(warnings)]
extern crate self as facet;
extern crate self as facet_core;
#[prelude_import]
use std::prelude::rust_2024::*;
extern crate std;

use facet::Facet;

/// A struct demonstrating various field types and attributes.
pub struct KitchenSinkStruct {
    /// A basic string field.
    pub basic_field: String,
    /// A field marked as sensitive.
    pub sensitive_field: u64,
    /// A tuple field.
    pub tuple_field: (i32, bool),
    /// An array field.
    pub array_field: [u8; 4],
    /// A static slice field.
    pub slice_field: &'static [u8],
    /// A vector field.
    pub vec_field: Vec<f32>,
    /// A field containing another struct that derives Facet.
    pub nested_struct_field: Point,
}
#[used]
static KITCHEN_SINK_STRUCT_SHAPE: &'static facet::Shape =
    <KitchenSinkStruct as facet::Facet>::SHAPE;
#[automatically_derived]
unsafe impl facet::Facet for KitchenSinkStruct {
    const SHAPE: &'static facet::Shape = &const {
        let fields: &'static [facet::Field] = &const {
            [
                facet::Field::builder()
                    .name("basic_field")
                    .shape(facet::shape_of(&(|s: KitchenSinkStruct| s.basic_field)))
                    .offset({
                        builtin # offset_of(KitchenSinkStruct<>, basic_field)
                    })
                    .flags(facet::FieldFlags::EMPTY)
                    .attributes(&[])
                    .doc(&[" A basic string field."])
                    .build(),
                facet::Field::builder()
                    .name("sensitive_field")
                    .shape(facet::shape_of(&(|s: KitchenSinkStruct| s.sensitive_field)))
                    .offset({
                        builtin # offset_of(KitchenSinkStruct<>, sensitive_field)
                    })
                    .flags(facet::FieldFlags::SENSITIVE)
                    .attributes(&[facet::FieldAttribute::Sensitive])
                    .doc(&[" A field marked as sensitive."])
                    .build(),
                facet::Field::builder()
                    .name("tuple_field")
                    .shape(facet::shape_of(&(|s: KitchenSinkStruct| s.tuple_field)))
                    .offset({
                        builtin # offset_of(KitchenSinkStruct<>, tuple_field)
                    })
                    .flags(facet::FieldFlags::EMPTY)
                    .attributes(&[])
                    .doc(&[" A tuple field."])
                    .build(),
                facet::Field::builder()
                    .name("array_field")
                    .shape(facet::shape_of(&(|s: KitchenSinkStruct| s.array_field)))
                    .offset({
                        builtin # offset_of(KitchenSinkStruct<>, array_field)
                    })
                    .flags(facet::FieldFlags::EMPTY)
                    .attributes(&[])
                    .doc(&[" An array field."])
                    .build(),
                facet::Field::builder()
                    .name("slice_field")
                    .shape(facet::shape_of(&(|s: KitchenSinkStruct| s.slice_field)))
                    .offset({
                        builtin # offset_of(KitchenSinkStruct<>, slice_field)
                    })
                    .flags(facet::FieldFlags::EMPTY)
                    .attributes(&[])
                    .doc(&[" A static slice field."])
                    .build(),
                facet::Field::builder()
                    .name("vec_field")
                    .shape(facet::shape_of(&(|s: KitchenSinkStruct| s.vec_field)))
                    .offset({
                        builtin # offset_of(KitchenSinkStruct<>, vec_field)
                    })
                    .flags(facet::FieldFlags::EMPTY)
                    .attributes(&[])
                    .doc(&[" A vector field."])
                    .build(),
                facet::Field::builder()
                    .name("nested_struct_field")
                    .shape(facet::shape_of(
                        &(|s: KitchenSinkStruct| s.nested_struct_field),
                    ))
                    .offset({
                        builtin # offset_of(KitchenSinkStruct<>,
                                                                        nested_struct_field)
                    })
                    .flags(facet::FieldFlags::EMPTY)
                    .attributes(&[])
                    .doc(&[" A field containing another struct that derives Facet."])
                    .build(),
            ]
        };
        facet::Shape::builder()
            .id(facet::ConstTypeId::of::<Self>())
            .layout(core::alloc::Layout::new::<Self>())
            .vtable(
                &const {
                    let mut builder = ::facet_core::ValueVTable::builder()
                        .type_name(|f, _opts| core::fmt::Write::write_str(f, "KitchenSinkStruct"))
                        .drop_in_place(|data| unsafe { data.drop_in_place::<Self>() });
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::fmt::Display> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.display(|data, f| {
                            use ::facet_core::spez::*;
                            (&&Spez(unsafe { data.as_ref::<Self>() })).spez_display(f)
                        });
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::fmt::Debug> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.debug(|data, f| {
                            use ::facet_core::spez::*;
                            (&&Spez(unsafe { data.as_ref::<Self>() })).spez_debug(f)
                        });
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::default::Default> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.default_in_place(|target| {
                            use ::facet_core::spez::*;
                            unsafe { (&&SpezEmpty::<Self>::SPEZ).spez_default_in_place(target) }
                        });
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::clone::Clone> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.clone_into(|src, dst| {
                            use ::facet_core::spez::*;
                            unsafe { (&&Spez(src.as_ref::<Self>())).spez_clone_into(dst) }
                        });
                    }
                    {
                        let mut traits = ::facet_core::MarkerTraits::empty();
                        if {
                            /// Fallback trait with `False` for `IMPLS` if the type does not
                            /// implement the given trait.
                            trait DoesNotImpl {
                                const IMPLS: bool = false;
                            }
                            impl<T: ?Sized> DoesNotImpl for T {}
                            /// Concrete type with `True` for `IMPLS` if the type implements the
                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                            #[allow(dead_code)]
                            impl<T: ?Sized + core::cmp::Eq> Wrapper<T> {
                                const IMPLS: bool = true;
                            }
                            <Wrapper<Self>>::IMPLS
                        } {
                            traits = traits.union(::facet_core::MarkerTraits::EQ);
                        }
                        if {
                            /// Fallback trait with `False` for `IMPLS` if the type does not
                            /// implement the given trait.
                            trait DoesNotImpl {
                                const IMPLS: bool = false;
                            }
                            impl<T: ?Sized> DoesNotImpl for T {}
                            /// Concrete type with `True` for `IMPLS` if the type implements the
                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                            #[allow(dead_code)]
                            impl<T: ?Sized + core::marker::Send> Wrapper<T> {
                                const IMPLS: bool = true;
                            }
                            <Wrapper<Self>>::IMPLS
                        } {
                            traits = traits.union(::facet_core::MarkerTraits::SEND);
                        }
                        if {
                            /// Fallback trait with `False` for `IMPLS` if the type does not
                            /// implement the given trait.
                            trait DoesNotImpl {
                                const IMPLS: bool = false;
                            }
                            impl<T: ?Sized> DoesNotImpl for T {}
                            /// Concrete type with `True` for `IMPLS` if the type implements the
                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                            #[allow(dead_code)]
                            impl<T: ?Sized + core::marker::Sync> Wrapper<T> {
                                const IMPLS: bool = true;
                            }
                            <Wrapper<Self>>::IMPLS
                        } {
                            traits = traits.union(::facet_core::MarkerTraits::SYNC);
                        }
                        if {
                            /// Fallback trait with `False` for `IMPLS` if the type does not
                            /// implement the given trait.
                            trait DoesNotImpl {
                                const IMPLS: bool = false;
                            }
                            impl<T: ?Sized> DoesNotImpl for T {}
                            /// Concrete type with `True` for `IMPLS` if the type implements the
                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                            #[allow(dead_code)]
                            impl<T: ?Sized + core::marker::Copy> Wrapper<T> {
                                const IMPLS: bool = true;
                            }
                            <Wrapper<Self>>::IMPLS
                        } {
                            traits = traits.union(::facet_core::MarkerTraits::COPY);
                        }
                        builder = builder.marker_traits(traits);
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::cmp::PartialEq> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.eq(|left, right| {
                            use ::facet_core::spez::*;
                            (&&Spez(unsafe { left.as_ref::<Self>() }))
                                .spez_eq(&&Spez(unsafe { right.as_ref::<Self>() }))
                        });
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::cmp::PartialOrd> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.partial_ord(|left, right| {
                            use ::facet_core::spez::*;
                            (&&Spez(unsafe { left.as_ref::<Self>() }))
                                .spez_partial_cmp(&&Spez(unsafe { right.as_ref::<Self>() }))
                        });
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::cmp::Ord> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.ord(|left, right| {
                            use ::facet_core::spez::*;
                            (&&Spez(unsafe { left.as_ref::<Self>() }))
                                .spez_cmp(&&Spez(unsafe { right.as_ref::<Self>() }))
                        });
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::hash::Hash> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.hash(|value, hasher_this, hasher_write_fn| {
                            use ::facet_core::HasherProxy;
                            use ::facet_core::spez::*;
                            (&&Spez(unsafe { value.as_ref::<Self>() })).spez_hash(&mut unsafe {
                                HasherProxy::new(hasher_this, hasher_write_fn)
                            })
                        });
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::str::FromStr> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.parse(|s, target| {
                            use ::facet_core::spez::*;
                            let res = unsafe { (&&SpezEmpty::<Self>::SPEZ).spez_parse(s, target) };
                            res.map(|_| unsafe { target.assume_init() })
                        });
                    }
                    builder.build()
                },
            )
            .def(facet::Def::Struct(
                facet::StructDef::builder()
                    .kind(facet::StructKind::Struct)
                    .fields(fields)
                    .build(),
            ))
            .doc(&[" A struct demonstrating various field types and attributes."])
            .build()
    };
}
/// A simple point struct, also deriving Facet.
pub struct Point {
    pub x: f32,
    pub y: f32,
    /// Nested sensitive data within the struct.
    pub metadata: String,
}
#[used]
static POINT_SHAPE: &'static facet::Shape = <Point as facet::Facet>::SHAPE;
#[automatically_derived]
unsafe impl facet::Facet for Point {
    const SHAPE: &'static facet::Shape = &const {
        let fields: &'static [facet::Field] = &const {
            [
                facet::Field::builder()
                    .name("x")
                    .shape(facet::shape_of(&(|s: Point| s.x)))
                    .offset({
                        builtin # offset_of(Point<>, x)
                    })
                    .flags(facet::FieldFlags::EMPTY)
                    .attributes(&[])
                    .build(),
                facet::Field::builder()
                    .name("y")
                    .shape(facet::shape_of(&(|s: Point| s.y)))
                    .offset({
                        builtin # offset_of(Point<>, y)
                    })
                    .flags(facet::FieldFlags::EMPTY)
                    .attributes(&[])
                    .build(),
                facet::Field::builder()
                    .name("metadata")
                    .shape(facet::shape_of(&(|s: Point| s.metadata)))
                    .offset({
                        builtin # offset_of(Point<>, metadata)
                    })
                    .flags(facet::FieldFlags::SENSITIVE)
                    .attributes(&[facet::FieldAttribute::Sensitive])
                    .doc(&[" Nested sensitive data within the struct."])
                    .build(),
            ]
        };
        facet::Shape::builder()
            .id(facet::ConstTypeId::of::<Self>())
            .layout(core::alloc::Layout::new::<Self>())
            .vtable(
                &const {
                    let mut builder = ::facet_core::ValueVTable::builder()
                        .type_name(|f, _opts| core::fmt::Write::write_str(f, "Point"))
                        .drop_in_place(|data| unsafe { data.drop_in_place::<Self>() });
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::fmt::Display> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.display(|data, f| {
                            use ::facet_core::spez::*;
                            (&&Spez(unsafe { data.as_ref::<Self>() })).spez_display(f)
                        });
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::fmt::Debug> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.debug(|data, f| {
                            use ::facet_core::spez::*;
                            (&&Spez(unsafe { data.as_ref::<Self>() })).spez_debug(f)
                        });
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::default::Default> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.default_in_place(|target| {
                            use ::facet_core::spez::*;
                            unsafe { (&&SpezEmpty::<Self>::SPEZ).spez_default_in_place(target) }
                        });
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::clone::Clone> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.clone_into(|src, dst| {
                            use ::facet_core::spez::*;
                            unsafe { (&&Spez(src.as_ref::<Self>())).spez_clone_into(dst) }
                        });
                    }
                    {
                        let mut traits = ::facet_core::MarkerTraits::empty();
                        if {
                            /// Fallback trait with `False` for `IMPLS` if the type does not
                            /// implement the given trait.
                            trait DoesNotImpl {
                                const IMPLS: bool = false;
                            }
                            impl<T: ?Sized> DoesNotImpl for T {}
                            /// Concrete type with `True` for `IMPLS` if the type implements the
                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                            #[allow(dead_code)]
                            impl<T: ?Sized + core::cmp::Eq> Wrapper<T> {
                                const IMPLS: bool = true;
                            }
                            <Wrapper<Self>>::IMPLS
                        } {
                            traits = traits.union(::facet_core::MarkerTraits::EQ);
                        }
                        if {
                            /// Fallback trait with `False` for `IMPLS` if the type does not
                            /// implement the given trait.
                            trait DoesNotImpl {
                                const IMPLS: bool = false;
                            }
                            impl<T: ?Sized> DoesNotImpl for T {}
                            /// Concrete type with `True` for `IMPLS` if the type implements the
                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                            #[allow(dead_code)]
                            impl<T: ?Sized + core::marker::Send> Wrapper<T> {
                                const IMPLS: bool = true;
                            }
                            <Wrapper<Self>>::IMPLS
                        } {
                            traits = traits.union(::facet_core::MarkerTraits::SEND);
                        }
                        if {
                            /// Fallback trait with `False` for `IMPLS` if the type does not
                            /// implement the given trait.
                            trait DoesNotImpl {
                                const IMPLS: bool = false;
                            }
                            impl<T: ?Sized> DoesNotImpl for T {}
                            /// Concrete type with `True` for `IMPLS` if the type implements the
                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                            #[allow(dead_code)]
                            impl<T: ?Sized + core::marker::Sync> Wrapper<T> {
                                const IMPLS: bool = true;
                            }
                            <Wrapper<Self>>::IMPLS
                        } {
                            traits = traits.union(::facet_core::MarkerTraits::SYNC);
                        }
                        if {
                            /// Fallback trait with `False` for `IMPLS` if the type does not
                            /// implement the given trait.
                            trait DoesNotImpl {
                                const IMPLS: bool = false;
                            }
                            impl<T: ?Sized> DoesNotImpl for T {}
                            /// Concrete type with `True` for `IMPLS` if the type implements the
                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                            #[allow(dead_code)]
                            impl<T: ?Sized + core::marker::Copy> Wrapper<T> {
                                const IMPLS: bool = true;
                            }
                            <Wrapper<Self>>::IMPLS
                        } {
                            traits = traits.union(::facet_core::MarkerTraits::COPY);
                        }
                        builder = builder.marker_traits(traits);
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::cmp::PartialEq> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.eq(|left, right| {
                            use ::facet_core::spez::*;
                            (&&Spez(unsafe { left.as_ref::<Self>() }))
                                .spez_eq(&&Spez(unsafe { right.as_ref::<Self>() }))
                        });
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::cmp::PartialOrd> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.partial_ord(|left, right| {
                            use ::facet_core::spez::*;
                            (&&Spez(unsafe { left.as_ref::<Self>() }))
                                .spez_partial_cmp(&&Spez(unsafe { right.as_ref::<Self>() }))
                        });
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::cmp::Ord> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.ord(|left, right| {
                            use ::facet_core::spez::*;
                            (&&Spez(unsafe { left.as_ref::<Self>() }))
                                .spez_cmp(&&Spez(unsafe { right.as_ref::<Self>() }))
                        });
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::hash::Hash> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.hash(|value, hasher_this, hasher_write_fn| {
                            use ::facet_core::HasherProxy;
                            use ::facet_core::spez::*;
                            (&&Spez(unsafe { value.as_ref::<Self>() })).spez_hash(&mut unsafe {
                                HasherProxy::new(hasher_this, hasher_write_fn)
                            })
                        });
                    }
                    if {
                        /// Fallback trait with `False` for `IMPLS` if the type does not
                        /// implement the given trait.
                        trait DoesNotImpl {
                            const IMPLS: bool = false;
                        }
                        impl<T: ?Sized> DoesNotImpl for T {}
                        /// Concrete type with `True` for `IMPLS` if the type implements the
                        /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                        struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                        #[allow(dead_code)]
                        impl<T: ?Sized + core::str::FromStr> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        builder = builder.parse(|s, target| {
                            use ::facet_core::spez::*;
                            let res = unsafe { (&&SpezEmpty::<Self>::SPEZ).spez_parse(s, target) };
                            res.map(|_| unsafe { target.assume_init() })
                        });
                    }
                    builder.build()
                },
            )
            .def(facet::Def::Struct(
                facet::StructDef::builder()
                    .kind(facet::StructKind::Struct)
                    .fields(fields)
                    .build(),
            ))
            .doc(&[" A simple point struct, also deriving Facet."])
            .build()
    };
}
/// An enum demonstrating different variant types and attributes.
#[repr(u8)]
pub enum KitchenSinkEnum {
    /// A simple unit variant.
    UnitVariant,

    /// A tuple variant with a single element.
    ///
    /// The contained `String` represents an important message payload.
    TupleVariantSimple(String),

    /// A tuple variant with multiple elements.
    ///
    /// Contains important positional data:
    /// - `_0` (i32): An identifier code.
    /// - `_1` (i32): A sequence number.
    /// - `_2` (i32): A status flag.
    TupleVariantMulti(i32, i32, i32),

    /// A struct variant with named fields.
    StructVariant {
        /// The width dimension, crucial for rendering.
        width: f64,
        /// The height dimension, also crucial for rendering.
        height: f64,
    },

    /// A tuple variant marked entirely as sensitive.
    SensitiveTupleVariant(Vec<u8>),

    /// A struct variant containing a sensitive field.
    StructVariantWithSensitiveField {
        /// The main data payload, publicly accessible.
        payload: Vec<u8>,
        /// The sensitive checksum for integrity verification.
        checksum: u32,
    },

    /// A variant marked as arbitrary, potentially skipped during processing.
    ArbitraryVariant((f64, f64)),

    /// A variant containing another enum that derives Facet.
    ///
    /// The nested `SubEnum` indicates a specific sub-state or option.
    NestedEnumVariant(SubEnum),
}
#[used]
static KITCHEN_SINK_ENUM_SHAPE: &'static facet::Shape = <KitchenSinkEnum as facet::Facet>::SHAPE;
#[automatically_derived]
unsafe impl facet::Facet for KitchenSinkEnum {
    const SHAPE: &'static facet::Shape = &const {
        #[repr(C)]
        struct __ShadowKitchenSinkEnum_TupleVariantSimple {
            _discriminant: u8,
            _0: String,
        }
        #[repr(C)]
        struct __ShadowKitchenSinkEnum_TupleVariantMulti {
            _discriminant: u8,
            _0: i32,
            _1: i32,
            _2: i32,
        }
        #[repr(C)]
        struct __ShadowKitchenSinkEnum_StructVariant {
            _discriminant: u8,
            width: f64,
            height: f64,
        }
        #[repr(C)]
        struct __ShadowKitchenSinkEnum_SensitiveTupleVariant {
            _discriminant: u8,
            _0: Vec<u8>,
        }
        #[repr(C)]
        struct __ShadowKitchenSinkEnum_StructVariantWithSensitiveField {
            _discriminant: u8,
            payload: Vec<u8>,
            checksum: u32,
        }
        #[repr(C)]
        struct __ShadowKitchenSinkEnum_ArbitraryVariant {
            _discriminant: u8,
            _0: (f64, f64),
        }
        #[repr(C)]
        struct __ShadowKitchenSinkEnum_NestedEnumVariant {
            _discriminant: u8,
            _0: SubEnum,
        }
        facet::Shape::builder().id(facet::ConstTypeId::of::<Self>()).layout(core::alloc::Layout::new::<Self>()).vtable(&const {
                                                let mut builder =
                                                    ::facet_core::ValueVTable::builder().type_name(|f, _opts|
                                                                core::fmt::Write::write_str(f,
                                                                    "KitchenSinkEnum")).drop_in_place(|data|
                                                            unsafe { data.drop_in_place::<Self>() });
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Display> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.display(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe { data.as_ref::<Self>() })).spez_display(f)
                                                                    });
                                                    }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Debug> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.debug(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe { data.as_ref::<Self>() })).spez_debug(f)
                                                                    });
                                                    }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::default::Default> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.default_in_place(|target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&SpezEmpty::<Self>::SPEZ).spez_default_in_place(target)
                                                                        }
                                                                    });
                                                    }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::clone::Clone> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.clone_into(|src, dst|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&Spez(src.as_ref::<Self>())).spez_clone_into(dst)
                                                                        }
                                                                    });
                                                    }
                                                {
                                                    let mut traits = ::facet_core::MarkerTraits::empty();
                                                    if {
                                                                /// Fallback trait with `False` for `IMPLS` if the type does not
                                                                /// implement the given trait.
                                                                trait DoesNotImpl {
                                                                    const IMPLS: bool = false;
                                                                }
                                                                impl<T: ?Sized> DoesNotImpl for T {}
                                                                /// Concrete type with `True` for `IMPLS` if the type implements the
                                                                /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                                struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::cmp::Eq> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Self>>::IMPLS
                                                            } {
                                                            traits = traits.union(::facet_core::MarkerTraits::EQ);
                                                        }
                                                    if {
                                                                /// Fallback trait with `False` for `IMPLS` if the type does not
                                                                /// implement the given trait.
                                                                trait DoesNotImpl {
                                                                    const IMPLS: bool = false;
                                                                }
                                                                impl<T: ?Sized> DoesNotImpl for T {}
                                                                /// Concrete type with `True` for `IMPLS` if the type implements the
                                                                /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                                struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Send> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Self>>::IMPLS
                                                            } {
                                                            traits = traits.union(::facet_core::MarkerTraits::SEND);
                                                        }
                                                    if {
                                                                /// Fallback trait with `False` for `IMPLS` if the type does not
                                                                /// implement the given trait.
                                                                trait DoesNotImpl {
                                                                    const IMPLS: bool = false;
                                                                }
                                                                impl<T: ?Sized> DoesNotImpl for T {}
                                                                /// Concrete type with `True` for `IMPLS` if the type implements the
                                                                /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                                struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Sync> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Self>>::IMPLS
                                                            } {
                                                            traits = traits.union(::facet_core::MarkerTraits::SYNC);
                                                        }
                                                    if {
                                                                /// Fallback trait with `False` for `IMPLS` if the type does not
                                                                /// implement the given trait.
                                                                trait DoesNotImpl {
                                                                    const IMPLS: bool = false;
                                                                }
                                                                impl<T: ?Sized> DoesNotImpl for T {}
                                                                /// Concrete type with `True` for `IMPLS` if the type implements the
                                                                /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                                struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Copy> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Self>>::IMPLS
                                                            } {
                                                            traits = traits.union(::facet_core::MarkerTraits::COPY);
                                                        }
                                                    builder = builder.marker_traits(traits);
                                                }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialEq> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.eq(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Self>()
                                                                                            })).spez_eq(&&Spez(unsafe { right.as_ref::<Self>() }))
                                                                    });
                                                    }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialOrd> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.partial_ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Self>()
                                                                                            })).spez_partial_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<Self>()
                                                                                        }))
                                                                    });
                                                    }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::Ord> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Self>()
                                                                                            })).spez_cmp(&&Spez(unsafe { right.as_ref::<Self>() }))
                                                                    });
                                                    }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::hash::Hash> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.hash(|value, hasher_this, hasher_write_fn|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        use ::facet_core::HasherProxy;
                                                                        (&&Spez(unsafe {
                                                                                                value.as_ref::<Self>()
                                                                                            })).spez_hash(&mut unsafe {
                                                                                    HasherProxy::new(hasher_this, hasher_write_fn)
                                                                                })
                                                                    });
                                                    }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::str::FromStr> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.parse(|s, target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        let res =
                                                                            unsafe {
                                                                                (&&SpezEmpty::<Self>::SPEZ).spez_parse(s, target)
                                                                            };
                                                                        res.map(|_| unsafe { target.assume_init() })
                                                                    });
                                                    }
                                                builder.build()
                                            }).def(facet::Def::Enum(facet::EnumDef::builder().variants(&const {
                                                            [facet::Variant::builder().name("UnitVariant").discriminant(Some(0)).offset(0).kind(facet::VariantKind::Unit).doc(&[" A simple unit variant."]).build(),
                                                                    {
                                                                        let fields: &'static [facet::Field] =
                                                                            &const {
                                                                                        [facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowKitchenSinkEnum_TupleVariantSimple<>|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowKitchenSinkEnum_TupleVariantSimple<>,
                                                                                                                        _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()]
                                                                                    };
                                                                        facet::Variant::builder().name("TupleVariantSimple").discriminant(Some(1)).offset(0).kind(facet::VariantKind::Tuple {
                                                                                        fields,
                                                                                    }).doc(&[" A tuple variant with a single element.", "",
                                                                                            " The contained `String` represents an important message payload."]).build()
                                                                    },
                                                                    {
                                                                        let fields: &'static [facet::Field] =
                                                                            &const {
                                                                                        [facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowKitchenSinkEnum_TupleVariantMulti<>|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowKitchenSinkEnum_TupleVariantMulti<>,
                                                                                                                        _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                                                                                facet::Field::builder().name("_1").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowKitchenSinkEnum_TupleVariantMulti<>|
                                                                                                                                s._1))).offset({
                                                                                                                    builtin # offset_of(__ShadowKitchenSinkEnum_TupleVariantMulti<>,
                                                                                                                        _1)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                                                                                facet::Field::builder().name("_2").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowKitchenSinkEnum_TupleVariantMulti<>|
                                                                                                                                s._2))).offset({
                                                                                                                    builtin # offset_of(__ShadowKitchenSinkEnum_TupleVariantMulti<>,
                                                                                                                        _2)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()]
                                                                                    };
                                                                        facet::Variant::builder().name("TupleVariantMulti").discriminant(Some(2)).offset(0).kind(facet::VariantKind::Tuple {
                                                                                        fields,
                                                                                    }).doc(&[" A tuple variant with multiple elements.", "",
                                                                                            " Contains important positional data:",
                                                                                            " - `_0` (i32): An identifier code.",
                                                                                            " - `_1` (i32): A sequence number.",
                                                                                            " - `_2` (i32): A status flag."]).build()
                                                                    },
                                                                    {
                                                                        let fields: &'static [facet::Field] =
                                                                            &const {
                                                                                        [facet::Field::builder().name("width").shape(facet::shape_of(&(|s:
                                                                                                                                        __ShadowKitchenSinkEnum_StructVariant<>|
                                                                                                                                    s.width))).offset({
                                                                                                                        builtin # offset_of(__ShadowKitchenSinkEnum_StructVariant<>,
                                                                                                                            width)
                                                                                                                    }).flags(facet::FieldFlags::EMPTY).attributes(&[]).doc(&[" The width dimension, crucial for rendering."]).build(),
                                                                                                facet::Field::builder().name("height").shape(facet::shape_of(&(|s:
                                                                                                                                        __ShadowKitchenSinkEnum_StructVariant<>|
                                                                                                                                    s.height))).offset({
                                                                                                                        builtin # offset_of(__ShadowKitchenSinkEnum_StructVariant<>,
                                                                                                                            height)
                                                                                                                    }).flags(facet::FieldFlags::EMPTY).attributes(&[]).doc(&[" The height dimension, also crucial for rendering."]).build()]
                                                                                    };
                                                                        facet::Variant::builder().name("StructVariant").discriminant(Some(3)).offset(0).kind(facet::VariantKind::Struct {
                                                                                        fields,
                                                                                    }).doc(&[" A struct variant with named fields."]).build()
                                                                    },
                                                                    {
                                                                        let fields: &'static [facet::Field] =
                                                                            &const {
                                                                                        [facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowKitchenSinkEnum_SensitiveTupleVariant<>|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowKitchenSinkEnum_SensitiveTupleVariant<>,
                                                                                                                        _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()]
                                                                                    };
                                                                        facet::Variant::builder().name("SensitiveTupleVariant").discriminant(Some(4)).offset(0).kind(facet::VariantKind::Tuple {
                                                                                        fields,
                                                                                    }).doc(&[" A tuple variant marked entirely as sensitive."]).build()
                                                                    },
                                                                    {
                                                                        let fields: &'static [facet::Field] =
                                                                            &const {
                                                                                        [facet::Field::builder().name("payload").shape(facet::shape_of(&(|s:
                                                                                                                                        __ShadowKitchenSinkEnum_StructVariantWithSensitiveField<>|
                                                                                                                                    s.payload))).offset({
                                                                                                                        builtin # offset_of(__ShadowKitchenSinkEnum_StructVariantWithSensitiveField<>,
                                                                                                                            payload)
                                                                                                                    }).flags(facet::FieldFlags::EMPTY).attributes(&[]).doc(&[" The main data payload, publicly accessible."]).build(),
                                                                                                facet::Field::builder().name("checksum").shape(facet::shape_of(&(|s:
                                                                                                                                        __ShadowKitchenSinkEnum_StructVariantWithSensitiveField<>|
                                                                                                                                    s.checksum))).offset({
                                                                                                                        builtin # offset_of(__ShadowKitchenSinkEnum_StructVariantWithSensitiveField<>,
                                                                                                                            checksum)
                                                                                                                    }).flags(facet::FieldFlags::SENSITIVE).attributes(&[facet::FieldAttribute::Sensitive]).doc(&[" The sensitive checksum for integrity verification."]).build()]
                                                                                    };
                                                                        facet::Variant::builder().name("StructVariantWithSensitiveField").discriminant(Some(5)).offset(0).kind(facet::VariantKind::Struct {
                                                                                        fields,
                                                                                    }).doc(&[" A struct variant containing a sensitive field."]).build()
                                                                    },
                                                                    {
                                                                        let fields: &'static [facet::Field] =
                                                                            &const {
                                                                                        [facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowKitchenSinkEnum_ArbitraryVariant<>|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowKitchenSinkEnum_ArbitraryVariant<>,
                                                                                                                        _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()]
                                                                                    };
                                                                        facet::Variant::builder().name("ArbitraryVariant").discriminant(Some(6)).offset(0).kind(facet::VariantKind::Tuple {
                                                                                        fields,
                                                                                    }).doc(&[" A variant marked as arbitrary, potentially skipped during processing."]).build()
                                                                    },
                                                                    {
                                                                        let fields: &'static [facet::Field] =
                                                                            &const {
                                                                                        [facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowKitchenSinkEnum_NestedEnumVariant<>|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowKitchenSinkEnum_NestedEnumVariant<>,
                                                                                                                        _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()]
                                                                                    };
                                                                        facet::Variant::builder().name("NestedEnumVariant").discriminant(Some(7)).offset(0).kind(facet::VariantKind::Tuple {
                                                                                        fields,
                                                                                    }).doc(&[" A variant containing another enum that derives Facet.",
                                                                                            "",
                                                                                            " The nested `SubEnum` indicates a specific sub-state or option."]).build()
                                                                    }]
                                                        }).repr(facet::EnumRepr::U8).build())).doc(&[" An enum demonstrating different variant types and attributes."]).build()
    };
}
/// A sub-enum used within `KitchenSinkEnum`.
#[repr(u8)]
pub enum SubEnum {
    /// Option A.
    OptionA,

    /// Option B with data.
    OptionB(u8),

    /// A sensitive option.
    SensitiveOption(u64),

    /// An arbitrary option.
    ArbitraryOption(u8),
}
#[used]
static SUB_ENUM_SHAPE: &'static facet::Shape = <SubEnum as facet::Facet>::SHAPE;
#[automatically_derived]
unsafe impl facet::Facet for SubEnum {
    const SHAPE: &'static facet::Shape = &const {
        #[repr(C)]
        struct __ShadowSubEnum_OptionB {
            _discriminant: u8,
            _0: u8,
        }
        #[repr(C)]
        struct __ShadowSubEnum_SensitiveOption {
            _discriminant: u8,
            _0: u64,
        }
        #[repr(C)]
        struct __ShadowSubEnum_ArbitraryOption {
            _discriminant: u8,
            _0: u8,
        }
        facet::Shape::builder().id(facet::ConstTypeId::of::<Self>()).layout(core::alloc::Layout::new::<Self>()).vtable(&const {
                                                let mut builder =
                                                    ::facet_core::ValueVTable::builder().type_name(|f, _opts|
                                                                core::fmt::Write::write_str(f,
                                                                    "SubEnum")).drop_in_place(|data|
                                                            unsafe { data.drop_in_place::<Self>() });
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Display> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.display(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe { data.as_ref::<Self>() })).spez_display(f)
                                                                    });
                                                    }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Debug> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.debug(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe { data.as_ref::<Self>() })).spez_debug(f)
                                                                    });
                                                    }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::default::Default> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.default_in_place(|target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&SpezEmpty::<Self>::SPEZ).spez_default_in_place(target)
                                                                        }
                                                                    });
                                                    }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::clone::Clone> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.clone_into(|src, dst|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&Spez(src.as_ref::<Self>())).spez_clone_into(dst)
                                                                        }
                                                                    });
                                                    }
                                                {
                                                    let mut traits = ::facet_core::MarkerTraits::empty();
                                                    if {
                                                                /// Fallback trait with `False` for `IMPLS` if the type does not
                                                                /// implement the given trait.
                                                                trait DoesNotImpl {
                                                                    const IMPLS: bool = false;
                                                                }
                                                                impl<T: ?Sized> DoesNotImpl for T {}
                                                                /// Concrete type with `True` for `IMPLS` if the type implements the
                                                                /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                                struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::cmp::Eq> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Self>>::IMPLS
                                                            } {
                                                            traits = traits.union(::facet_core::MarkerTraits::EQ);
                                                        }
                                                    if {
                                                                /// Fallback trait with `False` for `IMPLS` if the type does not
                                                                /// implement the given trait.
                                                                trait DoesNotImpl {
                                                                    const IMPLS: bool = false;
                                                                }
                                                                impl<T: ?Sized> DoesNotImpl for T {}
                                                                /// Concrete type with `True` for `IMPLS` if the type implements the
                                                                /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                                struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Send> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Self>>::IMPLS
                                                            } {
                                                            traits = traits.union(::facet_core::MarkerTraits::SEND);
                                                        }
                                                    if {
                                                                /// Fallback trait with `False` for `IMPLS` if the type does not
                                                                /// implement the given trait.
                                                                trait DoesNotImpl {
                                                                    const IMPLS: bool = false;
                                                                }
                                                                impl<T: ?Sized> DoesNotImpl for T {}
                                                                /// Concrete type with `True` for `IMPLS` if the type implements the
                                                                /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                                struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Sync> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Self>>::IMPLS
                                                            } {
                                                            traits = traits.union(::facet_core::MarkerTraits::SYNC);
                                                        }
                                                    if {
                                                                /// Fallback trait with `False` for `IMPLS` if the type does not
                                                                /// implement the given trait.
                                                                trait DoesNotImpl {
                                                                    const IMPLS: bool = false;
                                                                }
                                                                impl<T: ?Sized> DoesNotImpl for T {}
                                                                /// Concrete type with `True` for `IMPLS` if the type implements the
                                                                /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                                struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Copy> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Self>>::IMPLS
                                                            } {
                                                            traits = traits.union(::facet_core::MarkerTraits::COPY);
                                                        }
                                                    builder = builder.marker_traits(traits);
                                                }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialEq> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.eq(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Self>()
                                                                                            })).spez_eq(&&Spez(unsafe { right.as_ref::<Self>() }))
                                                                    });
                                                    }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialOrd> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.partial_ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Self>()
                                                                                            })).spez_partial_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<Self>()
                                                                                        }))
                                                                    });
                                                    }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::Ord> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Self>()
                                                                                            })).spez_cmp(&&Spez(unsafe { right.as_ref::<Self>() }))
                                                                    });
                                                    }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::hash::Hash> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.hash(|value, hasher_this, hasher_write_fn|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        use ::facet_core::HasherProxy;
                                                                        (&&Spez(unsafe {
                                                                                                value.as_ref::<Self>()
                                                                                            })).spez_hash(&mut unsafe {
                                                                                    HasherProxy::new(hasher_this, hasher_write_fn)
                                                                                })
                                                                    });
                                                    }
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::str::FromStr> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Self>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.parse(|s, target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        let res =
                                                                            unsafe {
                                                                                (&&SpezEmpty::<Self>::SPEZ).spez_parse(s, target)
                                                                            };
                                                                        res.map(|_| unsafe { target.assume_init() })
                                                                    });
                                                    }
                                                builder.build()
                                            }).def(facet::Def::Enum(facet::EnumDef::builder().variants(&const {
                                                            [facet::Variant::builder().name("OptionA").discriminant(Some(0)).offset(0).kind(facet::VariantKind::Unit).doc(&[" Option A."]).build(),
                                                                    {
                                                                        let fields: &'static [facet::Field] =
                                                                            &const {
                                                                                        [facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowSubEnum_OptionB<>|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowSubEnum_OptionB<>, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()]
                                                                                    };
                                                                        facet::Variant::builder().name("OptionB").discriminant(Some(1)).offset(0).kind(facet::VariantKind::Tuple {
                                                                                        fields,
                                                                                    }).doc(&[" Option B with data."]).build()
                                                                    },
                                                                    {
                                                                        let fields: &'static [facet::Field] =
                                                                            &const {
                                                                                        [facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowSubEnum_SensitiveOption<>|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowSubEnum_SensitiveOption<>, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()]
                                                                                    };
                                                                        facet::Variant::builder().name("SensitiveOption").discriminant(Some(2)).offset(0).kind(facet::VariantKind::Tuple {
                                                                                        fields,
                                                                                    }).doc(&[" A sensitive option."]).build()
                                                                    },
                                                                    {
                                                                        let fields: &'static [facet::Field] =
                                                                            &const {
                                                                                        [facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowSubEnum_ArbitraryOption<>|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowSubEnum_ArbitraryOption<>, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()]
                                                                                    };
                                                                        facet::Variant::builder().name("ArbitraryOption").discriminant(Some(3)).offset(0).kind(facet::VariantKind::Tuple {
                                                                                        fields,
                                                                                    }).doc(&[" An arbitrary option."]).build()
                                                                    }]
                                                        }).repr(facet::EnumRepr::U8).build())).doc(&[" A sub-enum used within `KitchenSinkEnum`."]).build()
    };
}

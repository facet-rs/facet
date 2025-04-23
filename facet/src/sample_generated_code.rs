//! This defines a few types showcasing various features of the Facet derive macro.
#![allow(warnings)]
#[prelude_import]
use std::prelude::rust_2024::*;
extern crate std;

use crate::Facet;

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
static KITCHEN_SINK_STRUCT_SHAPE: &'static crate::Shape =
    <KitchenSinkStruct as crate::Facet>::SHAPE;
#[automatically_derived]
unsafe impl<'facet> crate::Facet<'facet> for KitchenSinkStruct {
    const SHAPE: &'static crate::Shape = &const {
        let fields: &'static [crate::Field] = &const {
            [
                crate::Field::builder()
                    .name("basic_field")
                    .shape(|| crate::shape_of(&(|s: &KitchenSinkStruct| &s.basic_field)))
                    .offset({
                        builtin # offset_of(KitchenSinkStruct, basic_field)
                    })
                    .flags(crate::FieldFlags::EMPTY)
                    .attributes(&const { [] })
                    .doc(&[" A basic string field."])
                    .build(),
                crate::Field::builder()
                    .name("sensitive_field")
                    .shape(|| crate::shape_of(&(|s: &KitchenSinkStruct| &s.sensitive_field)))
                    .offset({
                        builtin # offset_of(KitchenSinkStruct, sensitive_field)
                    })
                    .flags(crate::FieldFlags::SENSITIVE)
                    .attributes(&const { [crate::FieldAttribute::Sensitive] })
                    .doc(&[" A field marked as sensitive."])
                    .build(),
                crate::Field::builder()
                    .name("tuple_field")
                    .shape(|| crate::shape_of(&(|s: &KitchenSinkStruct| &s.tuple_field)))
                    .offset({
                        builtin # offset_of(KitchenSinkStruct, tuple_field)
                    })
                    .flags(crate::FieldFlags::EMPTY)
                    .attributes(&const { [] })
                    .doc(&[" A tuple field."])
                    .build(),
                crate::Field::builder()
                    .name("array_field")
                    .shape(|| crate::shape_of(&(|s: &KitchenSinkStruct| &s.array_field)))
                    .offset({
                        builtin # offset_of(KitchenSinkStruct, array_field)
                    })
                    .flags(crate::FieldFlags::EMPTY)
                    .attributes(&const { [] })
                    .doc(&[" An array field."])
                    .build(),
                crate::Field::builder()
                    .name("slice_field")
                    .shape(|| crate::shape_of(&(|s: &KitchenSinkStruct| &s.slice_field)))
                    .offset({
                        builtin # offset_of(KitchenSinkStruct, slice_field)
                    })
                    .flags(crate::FieldFlags::EMPTY)
                    .attributes(&const { [] })
                    .doc(&[" A static slice field."])
                    .build(),
                crate::Field::builder()
                    .name("vec_field")
                    .shape(|| crate::shape_of(&(|s: &KitchenSinkStruct| &s.vec_field)))
                    .offset({
                        builtin # offset_of(KitchenSinkStruct, vec_field)
                    })
                    .flags(crate::FieldFlags::EMPTY)
                    .attributes(&const { [] })
                    .doc(&[" A vector field."])
                    .build(),
                crate::Field::builder()
                    .name("nested_struct_field")
                    .shape(|| crate::shape_of(&(|s: &KitchenSinkStruct| &s.nested_struct_field)))
                    .offset({
                        builtin # offset_of(KitchenSinkStruct, nested_struct_field)
                    })
                    .flags(crate::FieldFlags::EMPTY)
                    .attributes(&const { [] })
                    .doc(&[" A field containing another struct that derives Facet."])
                    .build(),
            ]
        };
        let vtable = &const {
            let mut vtable = const {
                let mut builder = ::facet_core::ValueVTable::builder()
                    .type_name(|f, _opts| ::core::fmt::Write::write_str(f, "KitchenSinkStruct"))
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
                        (&&Spez(unsafe { data.get::<Self>() })).spez_display(f)
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
                        (&&Spez(unsafe { data.get::<Self>() })).spez_debug(f)
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
                        unsafe { (&&Spez(src.get::<Self>())).spez_clone_into(dst) }
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
                        impl<T: ?Sized + core::marker::Unpin> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        traits = traits.union(::facet_core::MarkerTraits::UNPIN);
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
                        (&&Spez(unsafe { left.get::<Self>() }))
                            .spez_eq(&&Spez(unsafe { right.get::<Self>() }))
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
                        (&&Spez(unsafe { left.get::<Self>() }))
                            .spez_partial_cmp(&&Spez(unsafe { right.get::<Self>() }))
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
                        (&&Spez(unsafe { left.get::<Self>() }))
                            .spez_cmp(&&Spez(unsafe { right.get::<Self>() }))
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
                        (&&Spez(unsafe { value.get::<Self>() })).spez_hash(&mut unsafe {
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
            };
            vtable
        };
        crate::Shape::builder()
            .id(crate::ConstTypeId::of::<Self>())
            .layout(::core::alloc::Layout::new::<Self>())
            .vtable(vtable)
            .def(crate::Def::Struct(
                crate::Struct::builder()
                    .kind(crate::StructKind::Struct)
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
static POINT_SHAPE: &'static crate::Shape = <Point as crate::Facet>::SHAPE;
#[automatically_derived]
unsafe impl<'facet> crate::Facet<'facet> for Point {
    const SHAPE: &'static crate::Shape = &const {
        let fields: &'static [crate::Field] = &const {
            [
                crate::Field::builder()
                    .name("x")
                    .shape(|| crate::shape_of(&(|s: &Point| &s.x)))
                    .offset({
                        builtin # offset_of(Point, x)
                    })
                    .flags(crate::FieldFlags::EMPTY)
                    .attributes(&const { [] })
                    .build(),
                crate::Field::builder()
                    .name("y")
                    .shape(|| crate::shape_of(&(|s: &Point| &s.y)))
                    .offset({
                        builtin # offset_of(Point, y)
                    })
                    .flags(crate::FieldFlags::EMPTY)
                    .attributes(&const { [] })
                    .build(),
                crate::Field::builder()
                    .name("metadata")
                    .shape(|| crate::shape_of(&(|s: &Point| &s.metadata)))
                    .offset({
                        builtin # offset_of(Point, metadata)
                    })
                    .flags(crate::FieldFlags::SENSITIVE)
                    .attributes(&const { [crate::FieldAttribute::Sensitive] })
                    .doc(&[" Nested sensitive data within the struct."])
                    .build(),
            ]
        };
        let vtable = &const {
            let mut vtable = const {
                let mut builder = ::facet_core::ValueVTable::builder()
                    .type_name(|f, _opts| ::core::fmt::Write::write_str(f, "Point"))
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
                        (&&Spez(unsafe { data.get::<Self>() })).spez_display(f)
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
                        (&&Spez(unsafe { data.get::<Self>() })).spez_debug(f)
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
                        unsafe { (&&Spez(src.get::<Self>())).spez_clone_into(dst) }
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
                        impl<T: ?Sized + core::marker::Unpin> Wrapper<T> {
                            const IMPLS: bool = true;
                        }
                        <Wrapper<Self>>::IMPLS
                    } {
                        traits = traits.union(::facet_core::MarkerTraits::UNPIN);
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
                        (&&Spez(unsafe { left.get::<Self>() }))
                            .spez_eq(&&Spez(unsafe { right.get::<Self>() }))
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
                        (&&Spez(unsafe { left.get::<Self>() }))
                            .spez_partial_cmp(&&Spez(unsafe { right.get::<Self>() }))
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
                        (&&Spez(unsafe { left.get::<Self>() }))
                            .spez_cmp(&&Spez(unsafe { right.get::<Self>() }))
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
                        (&&Spez(unsafe { value.get::<Self>() })).spez_hash(&mut unsafe {
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
            };
            vtable
        };
        crate::Shape::builder()
            .id(crate::ConstTypeId::of::<Self>())
            .layout(::core::alloc::Layout::new::<Self>())
            .vtable(vtable)
            .def(crate::Def::Struct(
                crate::Struct::builder()
                    .kind(crate::StructKind::Struct)
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
static KITCHEN_SINK_ENUM_SHAPE: &'static crate::Shape = <KitchenSinkEnum as crate::Facet>::SHAPE;
#[automatically_derived]
unsafe impl<'__facet> crate::Facet<'__facet> for KitchenSinkEnum {
    const SHAPE: &'static crate::Shape = &const {
        #[repr(C)]
        struct __ShadowKitchenSinkEnum_TupleVariantSimple<'__facet> {
            _discriminant: u8,
            _0: String,
            ___phantom: ::core::marker::PhantomData<*mut &'__facet ()>,
        }
        #[repr(C)]
        struct __ShadowKitchenSinkEnum_TupleVariantMulti<'__facet> {
            _discriminant: u8,
            _0: i32,
            _1: i32,
            _2: i32,
            ___phantom: ::core::marker::PhantomData<*mut &'__facet ()>,
        }
        #[repr(C)]
        struct __ShadowKitchenSinkEnum_StructVariant<'__facet> {
            _discriminant: u8,
            ___phantom: ::core::marker::PhantomData<*mut &'__facet ()>,
            width: f64,
            height: f64,
        }
        #[repr(C)]
        struct __ShadowKitchenSinkEnum_SensitiveTupleVariant<'__facet> {
            _discriminant: u8,
            _0: Vec<u8>,
            ___phantom: ::core::marker::PhantomData<*mut &'__facet ()>,
        }
        #[repr(C)]
        struct __ShadowKitchenSinkEnum_StructVariantWithSensitiveField<'__facet> {
            _discriminant: u8,
            ___phantom: ::core::marker::PhantomData<*mut &'__facet ()>,
            payload: Vec<u8>,
            checksum: u32,
        }
        #[repr(C)]
        struct __ShadowKitchenSinkEnum_ArbitraryVariant<'__facet> {
            _discriminant: u8,
            _0: (f64, f64),
            ___phantom: ::core::marker::PhantomData<*mut &'__facet ()>,
        }
        #[repr(C)]
        struct __ShadowKitchenSinkEnum_NestedEnumVariant<'__facet> {
            _discriminant: u8,
            _0: SubEnum,
            ___phantom: ::core::marker::PhantomData<*mut &'__facet ()>,
        }
        let __facet_variants: &'static [crate::Variant] = &const {
            [
                crate::Variant::builder()
                    .name("UnitVariant")
                    .discriminant(0)
                    .fields(crate::Struct::builder().unit().build())
                    .doc(&[" A simple unit variant."])
                    .build(),
                {
                    let fields: &'static [crate::Field] = &const {
                        [crate::Field::builder().name("_0").shape(||
                                                                                                crate::shape_of(&(|s:
                                                                                                                &__ShadowKitchenSinkEnum_TupleVariantSimple<'__facet>|
                                                                                                            &s._0))).offset({
                                                                                            builtin # offset_of(__ShadowKitchenSinkEnum_TupleVariantSimple<'__facet>,
                                                                                                _0)
                                                                                        }).flags(crate::FieldFlags::EMPTY).attributes(&const {
                                                                                            []
                                                                                        }).build()]
                    };
                    crate::Variant::builder()
                        .name("TupleVariantSimple")
                        .discriminant(1)
                        .fields(crate::Struct::builder().tuple().fields(fields).build())
                        .doc(&[
                            " A tuple variant with a single element.",
                            "",
                            " The contained `String` represents an important message payload.",
                        ])
                        .build()
                },
                {
                    let fields: &'static [crate::Field] = &const {
                        [crate::Field::builder().name("_0").shape(||
                                                                                                crate::shape_of(&(|s:
                                                                                                                &__ShadowKitchenSinkEnum_TupleVariantMulti<'__facet>|
                                                                                                            &s._0))).offset({
                                                                                            builtin # offset_of(__ShadowKitchenSinkEnum_TupleVariantMulti<'__facet>,
                                                                                                _0)
                                                                                        }).flags(crate::FieldFlags::EMPTY).attributes(&const {
                                                                                            []
                                                                                        }).build(),
                                                                        crate::Field::builder().name("_1").shape(||
                                                                                                crate::shape_of(&(|s:
                                                                                                                &__ShadowKitchenSinkEnum_TupleVariantMulti<'__facet>|
                                                                                                            &s._1))).offset({
                                                                                            builtin # offset_of(__ShadowKitchenSinkEnum_TupleVariantMulti<'__facet>,
                                                                                                _1)
                                                                                        }).flags(crate::FieldFlags::EMPTY).attributes(&const {
                                                                                            []
                                                                                        }).build(),
                                                                        crate::Field::builder().name("_2").shape(||
                                                                                                crate::shape_of(&(|s:
                                                                                                                &__ShadowKitchenSinkEnum_TupleVariantMulti<'__facet>|
                                                                                                            &s._2))).offset({
                                                                                            builtin # offset_of(__ShadowKitchenSinkEnum_TupleVariantMulti<'__facet>,
                                                                                                _2)
                                                                                        }).flags(crate::FieldFlags::EMPTY).attributes(&const {
                                                                                            []
                                                                                        }).build()]
                    };
                    crate::Variant::builder()
                        .name("TupleVariantMulti")
                        .discriminant(2)
                        .fields(crate::Struct::builder().tuple().fields(fields).build())
                        .doc(&[
                            " A tuple variant with multiple elements.",
                            "",
                            " Contains important positional data:",
                            " - `_0` (i32): An identifier code.",
                            " - `_1` (i32): A sequence number.",
                            " - `_2` (i32): A status flag.",
                        ])
                        .build()
                },
                {
                    let fields: &'static [crate::Field] = &const {
                        [crate::Field::builder().name("width").shape(||
                                                                                                    crate::shape_of(&(|s:
                                                                                                                    &__ShadowKitchenSinkEnum_StructVariant<'__facet>|
                                                                                                                &s.width))).offset({
                                                                                                builtin # offset_of(__ShadowKitchenSinkEnum_StructVariant<'__facet>,
                                                                                                    width)
                                                                                            }).flags(crate::FieldFlags::EMPTY).attributes(&const {
                                                                                                []
                                                                                            }).doc(&[" The width dimension, crucial for rendering."]).build(),
                                                                        crate::Field::builder().name("height").shape(||
                                                                                                    crate::shape_of(&(|s:
                                                                                                                    &__ShadowKitchenSinkEnum_StructVariant<'__facet>|
                                                                                                                &s.height))).offset({
                                                                                                builtin # offset_of(__ShadowKitchenSinkEnum_StructVariant<'__facet>,
                                                                                                    height)
                                                                                            }).flags(crate::FieldFlags::EMPTY).attributes(&const {
                                                                                                []
                                                                                            }).doc(&[" The height dimension, also crucial for rendering."]).build()]
                    };
                    crate::Variant::builder()
                        .name("StructVariant")
                        .discriminant(3)
                        .fields(crate::Struct::builder().struct_().fields(fields).build())
                        .doc(&[" A struct variant with named fields."])
                        .build()
                },
                {
                    let fields: &'static [crate::Field] = &const {
                        [crate::Field::builder().name("_0").shape(||
                                                                                                crate::shape_of(&(|s:
                                                                                                                &__ShadowKitchenSinkEnum_SensitiveTupleVariant<'__facet>|
                                                                                                            &s._0))).offset({
                                                                                            builtin # offset_of(__ShadowKitchenSinkEnum_SensitiveTupleVariant<'__facet>,
                                                                                                _0)
                                                                                        }).flags(crate::FieldFlags::EMPTY).attributes(&const {
                                                                                            []
                                                                                        }).build()]
                    };
                    crate::Variant::builder()
                        .name("SensitiveTupleVariant")
                        .discriminant(4)
                        .fields(crate::Struct::builder().tuple().fields(fields).build())
                        .doc(&[" A tuple variant marked entirely as sensitive."])
                        .build()
                },
                {
                    let fields: &'static [crate::Field] = &const {
                        [crate::Field::builder().name("payload").shape(||
                                                                                                    crate::shape_of(&(|s:
                                                                                                                    &__ShadowKitchenSinkEnum_StructVariantWithSensitiveField<'__facet>|
                                                                                                                &s.payload))).offset({
                                                                                                builtin # offset_of(__ShadowKitchenSinkEnum_StructVariantWithSensitiveField<'__facet>,
                                                                                                    payload)
                                                                                            }).flags(crate::FieldFlags::EMPTY).attributes(&const {
                                                                                                []
                                                                                            }).doc(&[" The main data payload, publicly accessible."]).build(),
                                                                        crate::Field::builder().name("checksum").shape(||
                                                                                                    crate::shape_of(&(|s:
                                                                                                                    &__ShadowKitchenSinkEnum_StructVariantWithSensitiveField<'__facet>|
                                                                                                                &s.checksum))).offset({
                                                                                                builtin # offset_of(__ShadowKitchenSinkEnum_StructVariantWithSensitiveField<'__facet>,
                                                                                                    checksum)
                                                                                            }).flags(crate::FieldFlags::SENSITIVE).attributes(&const {
                                                                                                [crate::FieldAttribute::Sensitive]
                                                                                            }).doc(&[" The sensitive checksum for integrity verification."]).build()]
                    };
                    crate::Variant::builder()
                        .name("StructVariantWithSensitiveField")
                        .discriminant(5)
                        .fields(crate::Struct::builder().struct_().fields(fields).build())
                        .doc(&[" A struct variant containing a sensitive field."])
                        .build()
                },
                {
                    let fields: &'static [crate::Field] = &const {
                        [crate::Field::builder().name("_0").shape(||
                                                                                                crate::shape_of(&(|s:
                                                                                                                &__ShadowKitchenSinkEnum_ArbitraryVariant<'__facet>|
                                                                                                            &s._0))).offset({
                                                                                            builtin # offset_of(__ShadowKitchenSinkEnum_ArbitraryVariant<'__facet>,
                                                                                                _0)
                                                                                        }).flags(crate::FieldFlags::EMPTY).attributes(&const {
                                                                                            []
                                                                                        }).build()]
                    };
                    crate::Variant::builder().name("ArbitraryVariant").discriminant(6).fields(crate::Struct::builder().tuple().fields(fields).build()).doc(&[" A variant marked as arbitrary, potentially skipped during processing."]).build()
                },
                {
                    let fields: &'static [crate::Field] = &const {
                        [crate::Field::builder().name("_0").shape(||
                                                                                                crate::shape_of(&(|s:
                                                                                                                &__ShadowKitchenSinkEnum_NestedEnumVariant<'__facet>|
                                                                                                            &s._0))).offset({
                                                                                            builtin # offset_of(__ShadowKitchenSinkEnum_NestedEnumVariant<'__facet>,
                                                                                                _0)
                                                                                        }).flags(crate::FieldFlags::EMPTY).attributes(&const {
                                                                                            []
                                                                                        }).build()]
                    };
                    crate::Variant::builder()
                        .name("NestedEnumVariant")
                        .discriminant(7)
                        .fields(crate::Struct::builder().tuple().fields(fields).build())
                        .doc(&[
                            " A variant containing another enum that derives Facet.",
                            "",
                            " The nested `SubEnum` indicates a specific sub-state or option.",
                        ])
                        .build()
                },
            ]
        };
        crate::Shape::builder()
            .id(crate::ConstTypeId::of::<Self>())
            .layout(::core::alloc::Layout::new::<Self>())
            .vtable(
                &const {
                    let mut builder = ::facet_core::ValueVTable::builder()
                        .type_name(|f, _opts| ::core::fmt::Write::write_str(f, "KitchenSinkEnum"))
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
                            (&&Spez(unsafe { data.get::<Self>() })).spez_display(f)
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
                            (&&Spez(unsafe { data.get::<Self>() })).spez_debug(f)
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
                            unsafe { (&&Spez(src.get::<Self>())).spez_clone_into(dst) }
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
                            impl<T: ?Sized + core::marker::Unpin> Wrapper<T> {
                                const IMPLS: bool = true;
                            }
                            <Wrapper<Self>>::IMPLS
                        } {
                            traits = traits.union(::facet_core::MarkerTraits::UNPIN);
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
                            (&&Spez(unsafe { left.get::<Self>() }))
                                .spez_eq(&&Spez(unsafe { right.get::<Self>() }))
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
                            (&&Spez(unsafe { left.get::<Self>() }))
                                .spez_partial_cmp(&&Spez(unsafe { right.get::<Self>() }))
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
                            (&&Spez(unsafe { left.get::<Self>() }))
                                .spez_cmp(&&Spez(unsafe { right.get::<Self>() }))
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
                            (&&Spez(unsafe { value.get::<Self>() })).spez_hash(&mut unsafe {
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
            .def(crate::Def::Enum(
                crate::EnumDef::builder()
                    .variants(__facet_variants)
                    .repr(crate::EnumRepr::U8)
                    .build(),
            ))
            .doc(&[" An enum demonstrating different variant types and attributes."])
            .build()
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
static SUB_ENUM_SHAPE: &'static crate::Shape = <SubEnum as crate::Facet>::SHAPE;
#[automatically_derived]
unsafe impl<'__facet> crate::Facet<'__facet> for SubEnum {
    const SHAPE: &'static crate::Shape = &const {
        #[repr(C)]
        struct __ShadowSubEnum_OptionB<'__facet> {
            _discriminant: u8,
            _0: u8,
            ___phantom: ::core::marker::PhantomData<*mut &'__facet ()>,
        }
        #[repr(C)]
        struct __ShadowSubEnum_SensitiveOption<'__facet> {
            _discriminant: u8,
            _0: u64,
            ___phantom: ::core::marker::PhantomData<*mut &'__facet ()>,
        }
        #[repr(C)]
        struct __ShadowSubEnum_ArbitraryOption<'__facet> {
            _discriminant: u8,
            _0: u8,
            ___phantom: ::core::marker::PhantomData<*mut &'__facet ()>,
        }
        let __facet_variants: &'static [crate::Variant] = &const {
            [
                crate::Variant::builder()
                    .name("OptionA")
                    .discriminant(0)
                    .fields(crate::Struct::builder().unit().build())
                    .doc(&[" Option A."])
                    .build(),
                {
                    let fields: &'static [crate::Field] = &const {
                        [crate::Field::builder()
                            .name("_0")
                            .shape(|| {
                                crate::shape_of(&(|s: &__ShadowSubEnum_OptionB<'__facet>| &s._0))
                            })
                            .offset({
                                builtin # offset_of(__ShadowSubEnum_OptionB<'__facet>, _0)
                            })
                            .flags(crate::FieldFlags::EMPTY)
                            .attributes(&const { [] })
                            .build()]
                    };
                    crate::Variant::builder()
                        .name("OptionB")
                        .discriminant(1)
                        .fields(crate::Struct::builder().tuple().fields(fields).build())
                        .doc(&[" Option B with data."])
                        .build()
                },
                {
                    let fields: &'static [crate::Field] = &const {
                        [crate::Field::builder()
                            .name("_0")
                            .shape(|| {
                                crate::shape_of(
                                    &(|s: &__ShadowSubEnum_SensitiveOption<'__facet>| &s._0),
                                )
                            })
                            .offset({
                                builtin # offset_of(__ShadowSubEnum_SensitiveOption<'__facet>,
                                                                                                _0)
                            })
                            .flags(crate::FieldFlags::EMPTY)
                            .attributes(&const { [] })
                            .build()]
                    };
                    crate::Variant::builder()
                        .name("SensitiveOption")
                        .discriminant(2)
                        .fields(crate::Struct::builder().tuple().fields(fields).build())
                        .doc(&[" A sensitive option."])
                        .build()
                },
                {
                    let fields: &'static [crate::Field] = &const {
                        [crate::Field::builder()
                            .name("_0")
                            .shape(|| {
                                crate::shape_of(
                                    &(|s: &__ShadowSubEnum_ArbitraryOption<'__facet>| &s._0),
                                )
                            })
                            .offset({
                                builtin # offset_of(__ShadowSubEnum_ArbitraryOption<'__facet>,
                                                                                                _0)
                            })
                            .flags(crate::FieldFlags::EMPTY)
                            .attributes(&const { [] })
                            .build()]
                    };
                    crate::Variant::builder()
                        .name("ArbitraryOption")
                        .discriminant(3)
                        .fields(crate::Struct::builder().tuple().fields(fields).build())
                        .doc(&[" An arbitrary option."])
                        .build()
                },
            ]
        };
        crate::Shape::builder()
            .id(crate::ConstTypeId::of::<Self>())
            .layout(::core::alloc::Layout::new::<Self>())
            .vtable(
                &const {
                    let mut builder = ::facet_core::ValueVTable::builder()
                        .type_name(|f, _opts| ::core::fmt::Write::write_str(f, "SubEnum"))
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
                            (&&Spez(unsafe { data.get::<Self>() })).spez_display(f)
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
                            (&&Spez(unsafe { data.get::<Self>() })).spez_debug(f)
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
                            unsafe { (&&Spez(src.get::<Self>())).spez_clone_into(dst) }
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
                            impl<T: ?Sized + core::marker::Unpin> Wrapper<T> {
                                const IMPLS: bool = true;
                            }
                            <Wrapper<Self>>::IMPLS
                        } {
                            traits = traits.union(::facet_core::MarkerTraits::UNPIN);
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
                            (&&Spez(unsafe { left.get::<Self>() }))
                                .spez_eq(&&Spez(unsafe { right.get::<Self>() }))
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
                            (&&Spez(unsafe { left.get::<Self>() }))
                                .spez_partial_cmp(&&Spez(unsafe { right.get::<Self>() }))
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
                            (&&Spez(unsafe { left.get::<Self>() }))
                                .spez_cmp(&&Spez(unsafe { right.get::<Self>() }))
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
                            (&&Spez(unsafe { value.get::<Self>() })).spez_hash(&mut unsafe {
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
            .def(crate::Def::Enum(
                crate::EnumDef::builder()
                    .variants(__facet_variants)
                    .repr(crate::EnumRepr::U8)
                    .build(),
            ))
            .doc(&[" A sub-enum used within `KitchenSinkEnum`."])
            .build()
    };
}

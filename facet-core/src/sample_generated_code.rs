#![feature(prelude_import)]
//! This defines just a bunch of types so that we can see what the generated output looks like
#[prelude_import]
use std::prelude::rust_2024::*;
#[macro_use]
extern crate std;

use facet::Facet;

/// A struct with a couple fields
pub struct FooBar {
    pub foo: String,
    pub bar: u32,
}
#[used]
static FOO_BAR_SHAPE: &'static facet::Shape = <FooBar as facet::Facet>::SHAPE;
#[automatically_derived]
unsafe impl facet::Facet for FooBar {
    const SHAPE: &'static facet::Shape =
        &const {
                    static FIELDS: &[facet::Field] =
                        &[facet::Field::builder().name("foo").shape(facet::shape_of(&(|s:
                                                                        FooBar|
                                                                    s.foo))).offset(









                                                    {
                                                        builtin # offset_of(FooBar, foo)
                                                    }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                    facet::Field::builder().name("bar").shape(facet::shape_of(&(|s:
                                                                        FooBar|
                                                                    s.bar))).offset({
                                                        builtin # offset_of(FooBar, bar)
                                                    }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                    facet::Shape::builder().id(facet::ConstTypeId::of::<FooBar>()).layout(core::alloc::Layout::new::<Self>()).vtable(&const {
                                                let mut builder =
                                                    ::facet_core::ValueVTable::builder().type_name(|f, _opts|
                                                                core::fmt::Write::write_str(f,
                                                                    "FooBar")).drop_in_place(|data|
                                                            unsafe { data.drop_in_place::<FooBar>() });
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Display> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<FooBar>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.display(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe { data.as_ref::<FooBar>() })).spez_display(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Debug> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<FooBar>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.debug(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe { data.as_ref::<FooBar>() })).spez_debug(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::default::Default> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<FooBar>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.default_in_place(|target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&SpezEmpty::<FooBar>::SPEZ).spez_default_in_place(target)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::clone::Clone> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<FooBar>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.clone_into(|src, dst|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&Spez(src.as_ref::<FooBar>())).spez_clone_into(dst)
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::cmp::Eq> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<FooBar>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Send> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<FooBar>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Sync> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<FooBar>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Copy> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<FooBar>>::IMPLS
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialEq> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<FooBar>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.eq(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<FooBar>()
                                                                                            })).spez_eq(&&Spez(unsafe { right.as_ref::<FooBar>() }))
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialOrd> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<FooBar>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.partial_ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<FooBar>()
                                                                                            })).spez_partial_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<FooBar>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::Ord> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<FooBar>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<FooBar>()
                                                                                            })).spez_cmp(&&Spez(unsafe { right.as_ref::<FooBar>() }))
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::hash::Hash> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<FooBar>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.hash(|value, hasher_this, hasher_write_fn|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        use ::facet_core::HasherProxy;
                                                                        (&&Spez(unsafe {
                                                                                                value.as_ref::<FooBar>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::str::FromStr> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<FooBar>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.parse(|s, target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        let res =
                                                                            unsafe {
                                                                                (&&SpezEmpty::<FooBar>::SPEZ).spez_parse(s, target)
                                                                            };
                                                                        res.map(|_| unsafe { target.assume_init() })
                                                                    });
                                                    }
                                                builder.build()
                                            }).def(facet::Def::Struct(facet::StructDef::builder().kind(facet::StructKind::Struct).fields(FIELDS).build())).doc(&[" A struct with a couple fields"]).build()
                };
}
/// Represents different types of messages
#[repr(u8)]
pub enum Message {

    /// Simple notification without data
    Quit,

    /// Movement information
    #[facet(sensitive)]
    Move {
        x: i32,
        y: i32,
    },

    /// Text message with content
    Write(String),

    /// Color change request
    ChangeColor(i32, i32, i32),
}
#[used]
static MESSAGE_SHAPE: &'static facet::Shape =
    <Message as facet::Facet>::SHAPE;
#[automatically_derived]
unsafe impl facet::Facet for Message {
    const SHAPE: &'static facet::Shape =
        &const {
                    #[repr(C)]
                    struct __ShadowMessage_Move {
                        _discriminant: u8,
                        x: i32,
                        y: i32,
                    }
                    #[repr(C)]
                    struct __ShadowMessage_Write {
                        _discriminant: u8,
                        _0: String,
                    }
                    #[repr(C)]
                    struct __ShadowMessage_ChangeColor {
                        _discriminant: u8,
                        _0: i32,
                        _1: i32,
                        _2: i32,
                    }
                    facet::Shape::builder().id(facet::ConstTypeId::of::<Message>()).layout(core::alloc::Layout::new::<Self>()).vtable(&const {
                                                let mut builder =
                                                    ::facet_core::ValueVTable::builder().type_name(|f, _opts|
                                                                core::fmt::Write::write_str(f,
                                                                    "Message")).drop_in_place(|data|
                                                            unsafe { data.drop_in_place::<Message>() });
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Display> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Message>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.display(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                data.as_ref::<Message>()
                                                                                            })).spez_display(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Debug> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Message>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.debug(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe { data.as_ref::<Message>() })).spez_debug(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::default::Default> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Message>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.default_in_place(|target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&SpezEmpty::<Message>::SPEZ).spez_default_in_place(target)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::clone::Clone> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Message>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.clone_into(|src, dst|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&Spez(src.as_ref::<Message>())).spez_clone_into(dst)
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::cmp::Eq> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Message>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Send> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Message>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Sync> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Message>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Copy> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Message>>::IMPLS
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialEq> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Message>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.eq(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Message>()
                                                                                            })).spez_eq(&&Spez(unsafe { right.as_ref::<Message>() }))
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialOrd> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Message>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.partial_ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Message>()
                                                                                            })).spez_partial_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<Message>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::Ord> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Message>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Message>()
                                                                                            })).spez_cmp(&&Spez(unsafe { right.as_ref::<Message>() }))
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::hash::Hash> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Message>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.hash(|value, hasher_this, hasher_write_fn|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        use ::facet_core::HasherProxy;
                                                                        (&&Spez(unsafe {
                                                                                                value.as_ref::<Message>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::str::FromStr> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Message>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.parse(|s, target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        let res =
                                                                            unsafe {
                                                                                (&&SpezEmpty::<Message>::SPEZ).spez_parse(s, target)
                                                                            };
                                                                        res.map(|_| unsafe { target.assume_init() })
                                                                    });
                                                    }
                                                builder.build()
                                            }).def(facet::Def::Enum(facet::EnumDef::builder().variants(&const {
                                                            static VARIANTS: &[facet::Variant] =
                                                                &[facet::Variant::builder().name("Quit").discriminant(Some(0)).kind(facet::VariantKind::Unit).doc(&[" Simple notification without data"]).build(),
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("x").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowMessage_Move|
                                                                                                                                s.x))).offset({
                                                                                                                    builtin # offset_of(__ShadowMessage_Move, x)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                                                                                facet::Field::builder().name("y").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowMessage_Move|
                                                                                                                                s.y))).offset({
                                                                                                                    builtin # offset_of(__ShadowMessage_Move, y)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Move").discriminant(Some(1)).kind(facet::VariantKind::Struct {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" Movement information"]).build()
                                                                            },
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowMessage_Write|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowMessage_Write, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Write").discriminant(Some(2)).kind(facet::VariantKind::Tuple {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" Text message with content"]).build()
                                                                            },
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowMessage_ChangeColor|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowMessage_ChangeColor, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                                                                                facet::Field::builder().name("_1").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowMessage_ChangeColor|
                                                                                                                                s._1))).offset({
                                                                                                                    builtin # offset_of(__ShadowMessage_ChangeColor, _1)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                                                                                facet::Field::builder().name("_2").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowMessage_ChangeColor|
                                                                                                                                s._2))).offset({
                                                                                                                    builtin # offset_of(__ShadowMessage_ChangeColor, _2)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("ChangeColor").discriminant(Some(3)).kind(facet::VariantKind::Tuple {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" Color change request"]).build()
                                                                            }];
                                                            VARIANTS
                                                        }).repr(facet::EnumRepr::U8).build())).doc(&[" Represents different types of messages"]).build()
                };
}
/// Represents geometric shapes
#[repr(u8)]
pub enum Shape {

    /// A circle with radius
    Circle(f64),

    /// A rectangle with width and height
    Rectangle {
        width: f64,
        height: f64,
    },

    /// A triangle with three points
    #[facet(arbitrary)]
    Triangle((f64, f64), (f64, f64), (f64, f64)),
}
#[used]
static SHAPE_SHAPE: &'static facet::Shape = <Shape as facet::Facet>::SHAPE;
#[automatically_derived]
unsafe impl facet::Facet for Shape {
    const SHAPE: &'static facet::Shape =
        &const {
                    #[repr(C)]
                    struct __ShadowShape_Circle {
                        _discriminant: u8,
                        _0: f64,
                    }
                    #[repr(C)]
                    struct __ShadowShape_Rectangle {
                        _discriminant: u8,
                        width: f64,
                        height: f64,
                    }
                    #[repr(C)]
                    struct __ShadowShape_Triangle {
                        _discriminant: u8,
                        _0: (f64, f64),
                        _1: (f64, f64),
                        _2: (f64, f64),
                    }
                    facet::Shape::builder().id(facet::ConstTypeId::of::<Shape>()).layout(core::alloc::Layout::new::<Self>()).vtable(&const {
                                                let mut builder =
                                                    ::facet_core::ValueVTable::builder().type_name(|f, _opts|
                                                                core::fmt::Write::write_str(f,
                                                                    "Shape")).drop_in_place(|data|
                                                            unsafe { data.drop_in_place::<Shape>() });
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Display> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Shape>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.display(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe { data.as_ref::<Shape>() })).spez_display(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Debug> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Shape>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.debug(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe { data.as_ref::<Shape>() })).spez_debug(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::default::Default> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Shape>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.default_in_place(|target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&SpezEmpty::<Shape>::SPEZ).spez_default_in_place(target)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::clone::Clone> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Shape>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.clone_into(|src, dst|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&Spez(src.as_ref::<Shape>())).spez_clone_into(dst)
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::cmp::Eq> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Shape>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Send> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Shape>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Sync> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Shape>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Copy> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Shape>>::IMPLS
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialEq> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Shape>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.eq(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Shape>()
                                                                                            })).spez_eq(&&Spez(unsafe { right.as_ref::<Shape>() }))
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialOrd> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Shape>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.partial_ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Shape>()
                                                                                            })).spez_partial_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<Shape>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::Ord> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Shape>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Shape>()
                                                                                            })).spez_cmp(&&Spez(unsafe { right.as_ref::<Shape>() }))
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::hash::Hash> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Shape>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.hash(|value, hasher_this, hasher_write_fn|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        use ::facet_core::HasherProxy;
                                                                        (&&Spez(unsafe {
                                                                                                value.as_ref::<Shape>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::str::FromStr> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Shape>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.parse(|s, target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        let res =
                                                                            unsafe {
                                                                                (&&SpezEmpty::<Shape>::SPEZ).spez_parse(s, target)
                                                                            };
                                                                        res.map(|_| unsafe { target.assume_init() })
                                                                    });
                                                    }
                                                builder.build()
                                            }).def(facet::Def::Enum(facet::EnumDef::builder().variants(&const {
                                                            static VARIANTS: &[facet::Variant] =
                                                                &[{
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowShape_Circle|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowShape_Circle, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Circle").discriminant(Some(0)).kind(facet::VariantKind::Tuple {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" A circle with radius"]).build()
                                                                            },
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("width").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowShape_Rectangle|
                                                                                                                                s.width))).offset({
                                                                                                                    builtin # offset_of(__ShadowShape_Rectangle, width)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                                                                                facet::Field::builder().name("height").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowShape_Rectangle|
                                                                                                                                s.height))).offset({
                                                                                                                    builtin # offset_of(__ShadowShape_Rectangle, height)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Rectangle").discriminant(Some(1)).kind(facet::VariantKind::Struct {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" A rectangle with width and height"]).build()
                                                                            },
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowShape_Triangle|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowShape_Triangle, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                                                                                facet::Field::builder().name("_1").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowShape_Triangle|
                                                                                                                                s._1))).offset({
                                                                                                                    builtin # offset_of(__ShadowShape_Triangle, _1)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                                                                                facet::Field::builder().name("_2").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowShape_Triangle|
                                                                                                                                s._2))).offset({
                                                                                                                    builtin # offset_of(__ShadowShape_Triangle, _2)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Triangle").discriminant(Some(2)).kind(facet::VariantKind::Tuple {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" A triangle with three points"]).build()
                                                                            }];
                                                            VARIANTS
                                                        }).repr(facet::EnumRepr::U8).build())).doc(&[" Represents geometric shapes"]).build()
                };
}
/// Network packet types
#[repr(u8)]
pub enum Packet {

    /// Data packet with payload
    #[facet(sensitive)]
    Data {
        payload: Vec<u8>,
        checksum: u32,
    },

    /// Control packet
    Control(PacketType, u16),

    /// Array of bytes representing the header
    Header([u8; 4]),

    /// Slice of the packet buffer
    Fragment(&'static [u8]),
}
#[used]
static PACKET_SHAPE: &'static facet::Shape = <Packet as facet::Facet>::SHAPE;
#[automatically_derived]
unsafe impl facet::Facet for Packet {
    const SHAPE: &'static facet::Shape =
        &const {
                    #[repr(C)]
                    struct __ShadowPacket_Data {
                        _discriminant: u8,
                        payload: Vec<u8>,
                        checksum: u32,
                    }
                    #[repr(C)]
                    struct __ShadowPacket_Control {
                        _discriminant: u8,
                        _0: PacketType,
                        _1: u16,
                    }
                    #[repr(C)]
                    struct __ShadowPacket_Header {
                        _discriminant: u8,
                        _0: [u8; 4],
                    }
                    #[repr(C)]
                    struct __ShadowPacket_Fragment {
                        _discriminant: u8,
                        _0: &'static [u8],
                    }
                    facet::Shape::builder().id(facet::ConstTypeId::of::<Packet>()).layout(core::alloc::Layout::new::<Self>()).vtable(&const {
                                                let mut builder =
                                                    ::facet_core::ValueVTable::builder().type_name(|f, _opts|
                                                                core::fmt::Write::write_str(f,
                                                                    "Packet")).drop_in_place(|data|
                                                            unsafe { data.drop_in_place::<Packet>() });
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Display> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Packet>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.display(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe { data.as_ref::<Packet>() })).spez_display(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Debug> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Packet>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.debug(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe { data.as_ref::<Packet>() })).spez_debug(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::default::Default> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Packet>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.default_in_place(|target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&SpezEmpty::<Packet>::SPEZ).spez_default_in_place(target)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::clone::Clone> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Packet>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.clone_into(|src, dst|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&Spez(src.as_ref::<Packet>())).spez_clone_into(dst)
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::cmp::Eq> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Packet>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Send> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Packet>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Sync> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Packet>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Copy> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<Packet>>::IMPLS
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialEq> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Packet>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.eq(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Packet>()
                                                                                            })).spez_eq(&&Spez(unsafe { right.as_ref::<Packet>() }))
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialOrd> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Packet>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.partial_ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Packet>()
                                                                                            })).spez_partial_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<Packet>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::Ord> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Packet>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<Packet>()
                                                                                            })).spez_cmp(&&Spez(unsafe { right.as_ref::<Packet>() }))
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::hash::Hash> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Packet>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.hash(|value, hasher_this, hasher_write_fn|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        use ::facet_core::HasherProxy;
                                                                        (&&Spez(unsafe {
                                                                                                value.as_ref::<Packet>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::str::FromStr> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<Packet>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.parse(|s, target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        let res =
                                                                            unsafe {
                                                                                (&&SpezEmpty::<Packet>::SPEZ).spez_parse(s, target)
                                                                            };
                                                                        res.map(|_| unsafe { target.assume_init() })
                                                                    });
                                                    }
                                                builder.build()
                                            }).def(facet::Def::Enum(facet::EnumDef::builder().variants(&const {
                                                            static VARIANTS: &[facet::Variant] =
                                                                &[{
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("payload").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowPacket_Data|
                                                                                                                                s.payload))).offset({
                                                                                                                    builtin # offset_of(__ShadowPacket_Data, payload)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                                                                                facet::Field::builder().name("checksum").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowPacket_Data|
                                                                                                                                s.checksum))).offset({
                                                                                                                    builtin # offset_of(__ShadowPacket_Data, checksum)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Data").discriminant(Some(0)).kind(facet::VariantKind::Struct {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" Data packet with payload"]).build()
                                                                            },
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowPacket_Control|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowPacket_Control, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                                                                                facet::Field::builder().name("_1").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowPacket_Control|
                                                                                                                                s._1))).offset({
                                                                                                                    builtin # offset_of(__ShadowPacket_Control, _1)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Control").discriminant(Some(1)).kind(facet::VariantKind::Tuple {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" Control packet"]).build()
                                                                            },
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowPacket_Header|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowPacket_Header, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Header").discriminant(Some(2)).kind(facet::VariantKind::Tuple {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" Array of bytes representing the header"]).build()
                                                                            },
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowPacket_Fragment|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowPacket_Fragment, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Fragment").discriminant(Some(3)).kind(facet::VariantKind::Tuple {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" Slice of the packet buffer"]).build()
                                                                            }];
                                                            VARIANTS
                                                        }).repr(facet::EnumRepr::U8).build())).doc(&[" Network packet types"]).build()
                };
}
/// Different types of control packets
#[repr(u8)]
pub enum PacketType {

    /// Acknowledgment packet
    Ack,

    /// Negative acknowledgment
    Nack,

    /// Synchronization packet
    #[facet(sensitive)]
    Sync(u64),

    /// Reset connection
    Reset,
}
#[used]
static PACKET_TYPE_SHAPE: &'static facet::Shape =
    <PacketType as facet::Facet>::SHAPE;
#[automatically_derived]
unsafe impl facet::Facet for PacketType {
    const SHAPE: &'static facet::Shape =
        &const {
                    #[repr(C)]
                    struct __ShadowPacketType_Sync {
                        _discriminant: u8,
                        _0: u64,
                    }
                    facet::Shape::builder().id(facet::ConstTypeId::of::<PacketType>()).layout(core::alloc::Layout::new::<Self>()).vtable(&const {
                                                let mut builder =
                                                    ::facet_core::ValueVTable::builder().type_name(|f, _opts|
                                                                core::fmt::Write::write_str(f,
                                                                    "PacketType")).drop_in_place(|data|
                                                            unsafe { data.drop_in_place::<PacketType>() });
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Display> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<PacketType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.display(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                data.as_ref::<PacketType>()
                                                                                            })).spez_display(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Debug> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<PacketType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.debug(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                data.as_ref::<PacketType>()
                                                                                            })).spez_debug(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::default::Default> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<PacketType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.default_in_place(|target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&SpezEmpty::<PacketType>::SPEZ).spez_default_in_place(target)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::clone::Clone> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<PacketType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.clone_into(|src, dst|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&Spez(src.as_ref::<PacketType>())).spez_clone_into(dst)
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::cmp::Eq> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<PacketType>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Send> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<PacketType>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Sync> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<PacketType>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Copy> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<PacketType>>::IMPLS
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialEq> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<PacketType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.eq(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<PacketType>()
                                                                                            })).spez_eq(&&Spez(unsafe { right.as_ref::<PacketType>() }))
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialOrd> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<PacketType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.partial_ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<PacketType>()
                                                                                            })).spez_partial_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<PacketType>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::Ord> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<PacketType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<PacketType>()
                                                                                            })).spez_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<PacketType>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::hash::Hash> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<PacketType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.hash(|value, hasher_this, hasher_write_fn|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        use ::facet_core::HasherProxy;
                                                                        (&&Spez(unsafe {
                                                                                                value.as_ref::<PacketType>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::str::FromStr> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<PacketType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.parse(|s, target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        let res =
                                                                            unsafe {
                                                                                (&&SpezEmpty::<PacketType>::SPEZ).spez_parse(s, target)
                                                                            };
                                                                        res.map(|_| unsafe { target.assume_init() })
                                                                    });
                                                    }
                                                builder.build()
                                            }).def(facet::Def::Enum(facet::EnumDef::builder().variants(&const {
                                                            static VARIANTS: &[facet::Variant] =
                                                                &[facet::Variant::builder().name("Ack").discriminant(Some(0)).kind(facet::VariantKind::Unit).doc(&[" Acknowledgment packet"]).build(),
                                                                            facet::Variant::builder().name("Nack").discriminant(Some(1)).kind(facet::VariantKind::Unit).doc(&[" Negative acknowledgment"]).build(),
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowPacketType_Sync|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowPacketType_Sync, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Sync").discriminant(Some(2)).kind(facet::VariantKind::Tuple {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" Synchronization packet"]).build()
                                                                            },
                                                                            facet::Variant::builder().name("Reset").discriminant(Some(3)).kind(facet::VariantKind::Unit).doc(&[" Reset connection"]).build()];
                                                            VARIANTS
                                                        }).repr(facet::EnumRepr::U8).build())).doc(&[" Different types of control packets"]).build()
                };
}
/// Events in a system
#[repr(u8)]
pub enum SystemEvent {

    /// Timer events with duration
    Timer {
        #[facet(sensitive)]
        duration_ms: u64,
        repeating: bool,
    },

    /// IO events
    IO(IOType),

    /// User interaction events
    #[facet(arbitrary)]
    UserInput(UserInputType),

    /// System signals with array of parameters
    Signal([i32; 3]),
}
#[used]
static SYSTEM_EVENT_SHAPE: &'static facet::Shape =
    <SystemEvent as facet::Facet>::SHAPE;
#[automatically_derived]
unsafe impl facet::Facet for SystemEvent {
    const SHAPE: &'static facet::Shape =
        &const {
                    #[repr(C)]
                    struct __ShadowSystemEvent_Timer {
                        _discriminant: u8,
                        duration_ms: u64,
                        repeating: bool,
                    }
                    #[repr(C)]
                    struct __ShadowSystemEvent_IO {
                        _discriminant: u8,
                        _0: IOType,
                    }
                    #[repr(C)]
                    struct __ShadowSystemEvent_UserInput {
                        _discriminant: u8,
                        _0: UserInputType,
                    }
                    #[repr(C)]
                    struct __ShadowSystemEvent_Signal {
                        _discriminant: u8,
                        _0: [i32; 3],
                    }
                    facet::Shape::builder().id(facet::ConstTypeId::of::<SystemEvent>()).layout(core::alloc::Layout::new::<Self>()).vtable(&const {
                                                let mut builder =
                                                    ::facet_core::ValueVTable::builder().type_name(|f, _opts|
                                                                core::fmt::Write::write_str(f,
                                                                    "SystemEvent")).drop_in_place(|data|
                                                            unsafe { data.drop_in_place::<SystemEvent>() });
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Display> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<SystemEvent>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.display(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                data.as_ref::<SystemEvent>()
                                                                                            })).spez_display(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Debug> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<SystemEvent>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.debug(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                data.as_ref::<SystemEvent>()
                                                                                            })).spez_debug(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::default::Default> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<SystemEvent>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.default_in_place(|target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&SpezEmpty::<SystemEvent>::SPEZ).spez_default_in_place(target)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::clone::Clone> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<SystemEvent>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.clone_into(|src, dst|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&Spez(src.as_ref::<SystemEvent>())).spez_clone_into(dst)
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::cmp::Eq> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<SystemEvent>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Send> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<SystemEvent>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Sync> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<SystemEvent>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Copy> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<SystemEvent>>::IMPLS
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialEq> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<SystemEvent>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.eq(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<SystemEvent>()
                                                                                            })).spez_eq(&&Spez(unsafe {
                                                                                            right.as_ref::<SystemEvent>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialOrd> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<SystemEvent>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.partial_ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<SystemEvent>()
                                                                                            })).spez_partial_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<SystemEvent>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::Ord> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<SystemEvent>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<SystemEvent>()
                                                                                            })).spez_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<SystemEvent>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::hash::Hash> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<SystemEvent>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.hash(|value, hasher_this, hasher_write_fn|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        use ::facet_core::HasherProxy;
                                                                        (&&Spez(unsafe {
                                                                                                value.as_ref::<SystemEvent>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::str::FromStr> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<SystemEvent>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.parse(|s, target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        let res =
                                                                            unsafe {
                                                                                (&&SpezEmpty::<SystemEvent>::SPEZ).spez_parse(s, target)
                                                                            };
                                                                        res.map(|_| unsafe { target.assume_init() })
                                                                    });
                                                    }
                                                builder.build()
                                            }).def(facet::Def::Enum(facet::EnumDef::builder().variants(&const {
                                                            static VARIANTS: &[facet::Variant] =
                                                                &[{
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("duration_ms").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowSystemEvent_Timer|
                                                                                                                                s.duration_ms))).offset({
                                                                                                                    builtin # offset_of(__ShadowSystemEvent_Timer, duration_ms)
                                                                                                                }).flags(facet::FieldFlags::SENSITIVE).attributes(&[facet::FieldAttribute::Sensitive]).build(),
                                                                                                facet::Field::builder().name("repeating").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowSystemEvent_Timer|
                                                                                                                                s.repeating))).offset({
                                                                                                                    builtin # offset_of(__ShadowSystemEvent_Timer, repeating)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Timer").discriminant(Some(0)).kind(facet::VariantKind::Struct {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" Timer events with duration"]).build()
                                                                            },
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowSystemEvent_IO|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowSystemEvent_IO, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("IO").discriminant(Some(1)).kind(facet::VariantKind::Tuple {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" IO events"]).build()
                                                                            },
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowSystemEvent_UserInput|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowSystemEvent_UserInput, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("UserInput").discriminant(Some(2)).kind(facet::VariantKind::Tuple {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" User interaction events"]).build()
                                                                            },
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowSystemEvent_Signal|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowSystemEvent_Signal, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Signal").discriminant(Some(3)).kind(facet::VariantKind::Tuple {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" System signals with array of parameters"]).build()
                                                                            }];
                                                            VARIANTS
                                                        }).repr(facet::EnumRepr::U8).build())).doc(&[" Events in a system"]).build()
                };
}
/// Types of IO operations
#[repr(u8)]
pub enum IOType {

    /// Read operation
    Read,

    /// Write operation with data
    #[facet(sensitive)]
    Write(Vec<u8>),

    /// Both read and write
    ReadWrite {
        buffer_size: usize,
        timeout_ms: u32,
    },
}
#[used]
static I_O_TYPE_SHAPE: &'static facet::Shape =
    <IOType as facet::Facet>::SHAPE;
#[automatically_derived]
unsafe impl facet::Facet for IOType {
    const SHAPE: &'static facet::Shape =
        &const {
                    #[repr(C)]
                    struct __ShadowIOType_Write {
                        _discriminant: u8,
                        _0: Vec<u8>,
                    }
                    #[repr(C)]
                    struct __ShadowIOType_ReadWrite {
                        _discriminant: u8,
                        buffer_size: usize,
                        timeout_ms: u32,
                    }
                    facet::Shape::builder().id(facet::ConstTypeId::of::<IOType>()).layout(core::alloc::Layout::new::<Self>()).vtable(&const {
                                                let mut builder =
                                                    ::facet_core::ValueVTable::builder().type_name(|f, _opts|
                                                                core::fmt::Write::write_str(f,
                                                                    "IOType")).drop_in_place(|data|
                                                            unsafe { data.drop_in_place::<IOType>() });
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Display> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<IOType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.display(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe { data.as_ref::<IOType>() })).spez_display(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Debug> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<IOType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.debug(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe { data.as_ref::<IOType>() })).spez_debug(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::default::Default> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<IOType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.default_in_place(|target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&SpezEmpty::<IOType>::SPEZ).spez_default_in_place(target)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::clone::Clone> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<IOType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.clone_into(|src, dst|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&Spez(src.as_ref::<IOType>())).spez_clone_into(dst)
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::cmp::Eq> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<IOType>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Send> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<IOType>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Sync> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<IOType>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Copy> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<IOType>>::IMPLS
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialEq> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<IOType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.eq(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<IOType>()
                                                                                            })).spez_eq(&&Spez(unsafe { right.as_ref::<IOType>() }))
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialOrd> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<IOType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.partial_ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<IOType>()
                                                                                            })).spez_partial_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<IOType>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::Ord> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<IOType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<IOType>()
                                                                                            })).spez_cmp(&&Spez(unsafe { right.as_ref::<IOType>() }))
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::hash::Hash> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<IOType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.hash(|value, hasher_this, hasher_write_fn|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        use ::facet_core::HasherProxy;
                                                                        (&&Spez(unsafe {
                                                                                                value.as_ref::<IOType>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::str::FromStr> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<IOType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.parse(|s, target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        let res =
                                                                            unsafe {
                                                                                (&&SpezEmpty::<IOType>::SPEZ).spez_parse(s, target)
                                                                            };
                                                                        res.map(|_| unsafe { target.assume_init() })
                                                                    });
                                                    }
                                                builder.build()
                                            }).def(facet::Def::Enum(facet::EnumDef::builder().variants(&const {
                                                            static VARIANTS: &[facet::Variant] =
                                                                &[facet::Variant::builder().name("Read").discriminant(Some(0)).kind(facet::VariantKind::Unit).doc(&[" Read operation"]).build(),
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowIOType_Write|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowIOType_Write, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Write").discriminant(Some(1)).kind(facet::VariantKind::Tuple {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" Write operation with data"]).build()
                                                                            },
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("buffer_size").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowIOType_ReadWrite|
                                                                                                                                s.buffer_size))).offset({
                                                                                                                    builtin # offset_of(__ShadowIOType_ReadWrite, buffer_size)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                                                                                facet::Field::builder().name("timeout_ms").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowIOType_ReadWrite|
                                                                                                                                s.timeout_ms))).offset({
                                                                                                                    builtin # offset_of(__ShadowIOType_ReadWrite, timeout_ms)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("ReadWrite").discriminant(Some(2)).kind(facet::VariantKind::Struct {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" Both read and write"]).build()
                                                                            }];
                                                            VARIANTS
                                                        }).repr(facet::EnumRepr::U8).build())).doc(&[" Types of IO operations"]).build()
                };
}
/// User input types
#[repr(u8)]
pub enum UserInputType {

    /// Keyboard input
    Keyboard {
        key: char,
        modifiers: u8,
    },

    /// Mouse event
    #[facet(sensitive)]
    Mouse(i32, i32, MouseButton),

    /// Touch event with coordinates array
    Touch([TouchPoint; 5]),
}
#[used]
static USER_INPUT_TYPE_SHAPE: &'static facet::Shape =
    <UserInputType as facet::Facet>::SHAPE;
#[automatically_derived]
unsafe impl facet::Facet for UserInputType {
    const SHAPE: &'static facet::Shape =
        &const {
                    #[repr(C)]
                    struct __ShadowUserInputType_Keyboard {
                        _discriminant: u8,
                        key: char,
                        modifiers: u8,
                    }
                    #[repr(C)]
                    struct __ShadowUserInputType_Mouse {
                        _discriminant: u8,
                        _0: i32,
                        _1: i32,
                        _2: MouseButton,
                    }
                    #[repr(C)]
                    struct __ShadowUserInputType_Touch {
                        _discriminant: u8,
                        _0: [TouchPoint; 5],
                    }
                    facet::Shape::builder().id(facet::ConstTypeId::of::<UserInputType>()).layout(core::alloc::Layout::new::<Self>()).vtable(&const {
                                                let mut builder =
                                                    ::facet_core::ValueVTable::builder().type_name(|f, _opts|
                                                                core::fmt::Write::write_str(f,
                                                                    "UserInputType")).drop_in_place(|data|
                                                            unsafe { data.drop_in_place::<UserInputType>() });
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Display> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<UserInputType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.display(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                data.as_ref::<UserInputType>()
                                                                                            })).spez_display(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Debug> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<UserInputType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.debug(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                data.as_ref::<UserInputType>()
                                                                                            })).spez_debug(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::default::Default> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<UserInputType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.default_in_place(|target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&SpezEmpty::<UserInputType>::SPEZ).spez_default_in_place(target)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::clone::Clone> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<UserInputType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.clone_into(|src, dst|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&Spez(src.as_ref::<UserInputType>())).spez_clone_into(dst)
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::cmp::Eq> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<UserInputType>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Send> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<UserInputType>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Sync> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<UserInputType>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Copy> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<UserInputType>>::IMPLS
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialEq> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<UserInputType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.eq(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<UserInputType>()
                                                                                            })).spez_eq(&&Spez(unsafe {
                                                                                            right.as_ref::<UserInputType>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialOrd> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<UserInputType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.partial_ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<UserInputType>()
                                                                                            })).spez_partial_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<UserInputType>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::Ord> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<UserInputType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<UserInputType>()
                                                                                            })).spez_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<UserInputType>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::hash::Hash> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<UserInputType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.hash(|value, hasher_this, hasher_write_fn|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        use ::facet_core::HasherProxy;
                                                                        (&&Spez(unsafe {
                                                                                                value.as_ref::<UserInputType>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::str::FromStr> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<UserInputType>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.parse(|s, target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        let res =
                                                                            unsafe {
                                                                                (&&SpezEmpty::<UserInputType>::SPEZ).spez_parse(s, target)
                                                                            };
                                                                        res.map(|_| unsafe { target.assume_init() })
                                                                    });
                                                    }
                                                builder.build()
                                            }).def(facet::Def::Enum(facet::EnumDef::builder().variants(&const {
                                                            static VARIANTS: &[facet::Variant] =
                                                                &[{
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("key").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowUserInputType_Keyboard|
                                                                                                                                s.key))).offset({
                                                                                                                    builtin # offset_of(__ShadowUserInputType_Keyboard, key)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                                                                                facet::Field::builder().name("modifiers").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowUserInputType_Keyboard|
                                                                                                                                s.modifiers))).offset({
                                                                                                                    builtin # offset_of(__ShadowUserInputType_Keyboard,
                                                                                                                        modifiers)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Keyboard").discriminant(Some(0)).kind(facet::VariantKind::Struct {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" Keyboard input"]).build()
                                                                            },
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowUserInputType_Mouse|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowUserInputType_Mouse, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                                                                                facet::Field::builder().name("_1").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowUserInputType_Mouse|
                                                                                                                                s._1))).offset({
                                                                                                                    builtin # offset_of(__ShadowUserInputType_Mouse, _1)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                                                                                facet::Field::builder().name("_2").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowUserInputType_Mouse|
                                                                                                                                s._2))).offset({
                                                                                                                    builtin # offset_of(__ShadowUserInputType_Mouse, _2)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Mouse").discriminant(Some(1)).kind(facet::VariantKind::Tuple {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" Mouse event"]).build()
                                                                            },
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowUserInputType_Touch|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowUserInputType_Touch, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Touch").discriminant(Some(2)).kind(facet::VariantKind::Tuple {
                                                                                                fields: FIELDS,
                                                                                            }).doc(&[" Touch event with coordinates array"]).build()
                                                                            }];
                                                            VARIANTS
                                                        }).repr(facet::EnumRepr::U8).build())).doc(&[" User input types"]).build()
                };
}
/// Mouse button types
#[repr(u8)]
pub enum MouseButton {
    Left,
    Right,
    Middle,

    #[facet(arbitrary)]
    Extra(u8),
}
#[used]
static MOUSE_BUTTON_SHAPE: &'static facet::Shape =
    <MouseButton as facet::Facet>::SHAPE;
#[automatically_derived]
unsafe impl facet::Facet for MouseButton {
    const SHAPE: &'static facet::Shape =
        &const {
                    #[repr(C)]
                    struct __ShadowMouseButton_Extra {
                        _discriminant: u8,
                        _0: u8,
                    }
                    facet::Shape::builder().id(facet::ConstTypeId::of::<MouseButton>()).layout(core::alloc::Layout::new::<Self>()).vtable(&const {
                                                let mut builder =
                                                    ::facet_core::ValueVTable::builder().type_name(|f, _opts|
                                                                core::fmt::Write::write_str(f,
                                                                    "MouseButton")).drop_in_place(|data|
                                                            unsafe { data.drop_in_place::<MouseButton>() });
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Display> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<MouseButton>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.display(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                data.as_ref::<MouseButton>()
                                                                                            })).spez_display(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Debug> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<MouseButton>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.debug(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                data.as_ref::<MouseButton>()
                                                                                            })).spez_debug(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::default::Default> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<MouseButton>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.default_in_place(|target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&SpezEmpty::<MouseButton>::SPEZ).spez_default_in_place(target)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::clone::Clone> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<MouseButton>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.clone_into(|src, dst|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&Spez(src.as_ref::<MouseButton>())).spez_clone_into(dst)
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::cmp::Eq> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<MouseButton>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Send> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<MouseButton>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Sync> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<MouseButton>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Copy> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<MouseButton>>::IMPLS
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialEq> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<MouseButton>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.eq(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<MouseButton>()
                                                                                            })).spez_eq(&&Spez(unsafe {
                                                                                            right.as_ref::<MouseButton>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialOrd> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<MouseButton>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.partial_ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<MouseButton>()
                                                                                            })).spez_partial_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<MouseButton>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::Ord> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<MouseButton>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<MouseButton>()
                                                                                            })).spez_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<MouseButton>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::hash::Hash> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<MouseButton>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.hash(|value, hasher_this, hasher_write_fn|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        use ::facet_core::HasherProxy;
                                                                        (&&Spez(unsafe {
                                                                                                value.as_ref::<MouseButton>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::str::FromStr> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<MouseButton>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.parse(|s, target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        let res =
                                                                            unsafe {
                                                                                (&&SpezEmpty::<MouseButton>::SPEZ).spez_parse(s, target)
                                                                            };
                                                                        res.map(|_| unsafe { target.assume_init() })
                                                                    });
                                                    }
                                                builder.build()
                                            }).def(facet::Def::Enum(facet::EnumDef::builder().variants(&const {
                                                            static VARIANTS: &[facet::Variant] =
                                                                &[facet::Variant::builder().name("Left").discriminant(Some(0)).kind(facet::VariantKind::Unit).build(),
                                                                            facet::Variant::builder().name("Right").discriminant(Some(1)).kind(facet::VariantKind::Unit).build(),
                                                                            facet::Variant::builder().name("Middle").discriminant(Some(2)).kind(facet::VariantKind::Unit).build(),
                                                                            {
                                                                                static FIELDS: &[facet::Field] =
                                                                                    &[facet::Field::builder().name("_0").shape(facet::shape_of(&(|s:
                                                                                                                                    __ShadowMouseButton_Extra|
                                                                                                                                s._0))).offset({
                                                                                                                    builtin # offset_of(__ShadowMouseButton_Extra, _0)
                                                                                                                }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build()];
                                                                                facet::Variant::builder().name("Extra").discriminant(Some(3)).kind(facet::VariantKind::Tuple {
                                                                                            fields: FIELDS,
                                                                                        }).build()
                                                                            }];
                                                            VARIANTS
                                                        }).repr(facet::EnumRepr::U8).build())).doc(&[" Mouse button types"]).build()
                };
}
/// Represents a point of touch on a screen
pub struct TouchPoint {
    pub x: f32,
    pub y: f32,
    #[facet(sensitive)]
    pub pressure: f32,
}
#[used]
static TOUCH_POINT_SHAPE: &'static facet::Shape =
    <TouchPoint as facet::Facet>::SHAPE;
#[automatically_derived]
unsafe impl facet::Facet for TouchPoint {
    const SHAPE: &'static facet::Shape =
        &const {
                    static FIELDS: &[facet::Field] =
                        &[facet::Field::builder().name("x").shape(facet::shape_of(&(|s:
                                                                        TouchPoint|
                                                                    s.x))).offset({
                                                        builtin # offset_of(TouchPoint, x)
                                                    }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                    facet::Field::builder().name("y").shape(facet::shape_of(&(|s:
                                                                        TouchPoint|
                                                                    s.y))).offset({
                                                        builtin # offset_of(TouchPoint, y)
                                                    }).flags(facet::FieldFlags::EMPTY).attributes(&[]).build(),
                                    facet::Field::builder().name("pressure").shape(facet::shape_of(&(|s:
                                                                        TouchPoint|
                                                                    s.pressure))).offset({
                                                        builtin # offset_of(TouchPoint, pressure)
                                                    }).flags(facet::FieldFlags::SENSITIVE).attributes(&[facet::FieldAttribute::Sensitive]).build()];
                    facet::Shape::builder().id(facet::ConstTypeId::of::<TouchPoint>()).layout(core::alloc::Layout::new::<Self>()).vtable(&const {
                                                let mut builder =
                                                    ::facet_core::ValueVTable::builder().type_name(|f, _opts|
                                                                core::fmt::Write::write_str(f,
                                                                    "TouchPoint")).drop_in_place(|data|
                                                            unsafe { data.drop_in_place::<TouchPoint>() });
                                                if {
                                                            /// Fallback trait with `False` for `IMPLS` if the type does not
                                                            /// implement the given trait.
                                                            trait DoesNotImpl {
                                                                const IMPLS: bool = false;
                                                            }
                                                            impl<T: ?Sized> DoesNotImpl for T {}
                                                            /// Concrete type with `True` for `IMPLS` if the type implements the
                                                            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Display> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<TouchPoint>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.display(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                data.as_ref::<TouchPoint>()
                                                                                            })).spez_display(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::fmt::Debug> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<TouchPoint>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.debug(|data, f|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                data.as_ref::<TouchPoint>()
                                                                                            })).spez_debug(f)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::default::Default> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<TouchPoint>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.default_in_place(|target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&SpezEmpty::<TouchPoint>::SPEZ).spez_default_in_place(target)
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::clone::Clone> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<TouchPoint>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.clone_into(|src, dst|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        unsafe {
                                                                            (&&Spez(src.as_ref::<TouchPoint>())).spez_clone_into(dst)
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::cmp::Eq> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<TouchPoint>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Send> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<TouchPoint>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Sync> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<TouchPoint>>::IMPLS
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
                                                                struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                                #[allow(dead_code)]
                                                                impl<T: ?Sized + core::marker::Copy> Wrapper<T> {
                                                                    const IMPLS: bool = true;
                                                                }
                                                                <Wrapper<TouchPoint>>::IMPLS
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialEq> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<TouchPoint>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.eq(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<TouchPoint>()
                                                                                            })).spez_eq(&&Spez(unsafe { right.as_ref::<TouchPoint>() }))
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::PartialOrd> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<TouchPoint>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.partial_ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<TouchPoint>()
                                                                                            })).spez_partial_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<TouchPoint>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::cmp::Ord> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<TouchPoint>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.ord(|left, right|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        (&&Spez(unsafe {
                                                                                                left.as_ref::<TouchPoint>()
                                                                                            })).spez_cmp(&&Spez(unsafe {
                                                                                            right.as_ref::<TouchPoint>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::hash::Hash> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<TouchPoint>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.hash(|value, hasher_this, hasher_write_fn|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        use ::facet_core::HasherProxy;
                                                                        (&&Spez(unsafe {
                                                                                                value.as_ref::<TouchPoint>()
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
                                                            struct Wrapper<T: ?Sized>(::impls::_core::marker::PhantomData<T>);
                                                            #[allow(dead_code)]
                                                            impl<T: ?Sized + core::str::FromStr> Wrapper<T> {
                                                                const IMPLS: bool = true;
                                                            }
                                                            <Wrapper<TouchPoint>>::IMPLS
                                                        } {
                                                        builder =
                                                            builder.parse(|s, target|
                                                                    {
                                                                        use ::facet_core::spez::*;
                                                                        let res =
                                                                            unsafe {
                                                                                (&&SpezEmpty::<TouchPoint>::SPEZ).spez_parse(s, target)
                                                                            };
                                                                        res.map(|_| unsafe { target.assume_init() })
                                                                    });
                                                    }
                                                builder.build()
                                            }).def(facet::Def::Struct(facet::StructDef::builder().kind(facet::StructKind::Struct).fields(FIELDS).build())).doc(&[" Represents a point of touch on a screen"]).build()
                };
}

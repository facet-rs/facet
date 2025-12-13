use quote::{ToTokens, quote};

use super::{OwnedCodeGen, RefCodeGen, check_mode::CheckMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImplOption {
    Implement,
    Omit,
}

impl ImplOption {
    fn map<F>(self, f: F) -> Option<proc_macro2::TokenStream>
    where
        F: FnOnce() -> proc_macro2::TokenStream,
    {
        match self {
            Self::Implement => Some(f()),
            Self::Omit => None,
        }
    }
}

impl std::str::FromStr for ImplOption {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "impl" => Ok(Self::Implement),
            "omit" => Ok(Self::Omit),
            _ => Err("valid values are: `impl` or `omit`"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DelegatingImplOption {
    Implement,
    OwnedOnly,
    Omit,
}

impl DelegatingImplOption {
    fn map_owned<F>(self, f: F) -> Option<proc_macro2::TokenStream>
    where
        F: FnOnce() -> proc_macro2::TokenStream,
    {
        match self {
            Self::Implement | Self::OwnedOnly => Some(f()),
            Self::Omit => None,
        }
    }

    fn map_ref<F>(self, f: F) -> Option<proc_macro2::TokenStream>
    where
        F: FnOnce() -> proc_macro2::TokenStream,
    {
        match self {
            Self::Implement => Some(f()),
            Self::Omit | Self::OwnedOnly => None,
        }
    }
}

impl std::str::FromStr for DelegatingImplOption {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "impl" => Ok(Self::Implement),
            "owned" => Ok(Self::OwnedOnly),
            "omit" => Ok(Self::Omit),
            _ => Err("valid values are: `impl`, `owned`, or `omit`"),
        }
    }
}

impl From<ImplOption> for DelegatingImplOption {
    fn from(opt: ImplOption) -> Self {
        match opt {
            ImplOption::Implement => Self::Implement,
            ImplOption::Omit => Self::Omit,
        }
    }
}

#[derive(Debug, Default)]
pub struct Impls {
    pub clone: ImplClone,
    pub debug: ImplDebug,
    pub display: ImplDisplay,
    pub ord: ImplOrd,
    pub serde: ImplSerde,
}

pub(crate) trait ToImpl {
    fn to_owned_impl(&self, _cg: &OwnedCodeGen) -> Option<proc_macro2::TokenStream> {
        None
    }

    fn to_borrowed_impl(&self, _cg: &RefCodeGen) -> Option<proc_macro2::TokenStream> {
        None
    }
}

#[derive(Debug)]
pub struct ImplClone(ImplOption);

impl Default for ImplClone {
    fn default() -> Self {
        Self(ImplOption::Implement)
    }
}

impl From<ImplOption> for ImplClone {
    fn from(opt: ImplOption) -> Self {
        Self(opt)
    }
}

impl ToImpl for ImplClone {
    fn to_owned_impl(&self, _cg: &OwnedCodeGen) -> Option<proc_macro2::TokenStream> {
        self.0.map(|| quote! { #[derive(Clone)] })
    }
}

#[derive(Debug)]
pub struct ImplDisplay(DelegatingImplOption);

impl Default for ImplDisplay {
    fn default() -> Self {
        Self(DelegatingImplOption::Implement)
    }
}

impl From<DelegatingImplOption> for ImplDisplay {
    fn from(opt: DelegatingImplOption) -> Self {
        Self(opt)
    }
}
impl ToImpl for ImplDisplay {
    fn to_owned_impl(&self, cg: &OwnedCodeGen) -> Option<proc_macro2::TokenStream> {
        let ty = cg.ty;
        let ref_ty = cg.ref_ty;
        let core = cg.std_lib.core();
        self.0.map_owned(|| {
            quote! {
                #[automatically_derived]
                impl ::#core::fmt::Display for #ty {
                    #[inline]
                    fn fmt(&self, f: &mut ::#core::fmt::Formatter) -> ::#core::fmt::Result {
                        <#ref_ty as ::#core::fmt::Display>::fmt(::#core::ops::Deref::deref(self), f)
                    }
                }
            }
        })
    }

    fn to_borrowed_impl(&self, cg: &RefCodeGen) -> Option<proc_macro2::TokenStream> {
        let ty = &cg.ty;
        let field_name = &cg.field.name;
        let core = cg.std_lib.core();
        self.0.map_ref(|| {
            quote! {
                #[automatically_derived]
                impl ::#core::fmt::Display for #ty {
                    #[inline]
                    fn fmt(&self, f: &mut ::#core::fmt::Formatter) -> ::#core::fmt::Result {
                        <str as ::#core::fmt::Display>::fmt(&self.#field_name, f)
                    }
                }
            }
        })
    }
}

#[derive(Debug)]
pub struct ImplDebug(DelegatingImplOption);

impl Default for ImplDebug {
    fn default() -> Self {
        Self(DelegatingImplOption::Implement)
    }
}

impl From<DelegatingImplOption> for ImplDebug {
    fn from(opt: DelegatingImplOption) -> Self {
        Self(opt)
    }
}

impl ToImpl for ImplDebug {
    fn to_owned_impl(&self, cg: &OwnedCodeGen) -> Option<proc_macro2::TokenStream> {
        let ty = cg.ty;
        let ref_ty = cg.ref_ty;
        let core = cg.std_lib.core();
        self.0.map_owned(|| {
            quote! {
                #[automatically_derived]
                impl ::#core::fmt::Debug for #ty {
                    #[inline]
                    fn fmt(&self, f: &mut ::#core::fmt::Formatter) -> ::#core::fmt::Result {
                        <#ref_ty as ::#core::fmt::Debug>::fmt(::#core::ops::Deref::deref(self), f)
                    }
                }
            }
        })
    }

    fn to_borrowed_impl(&self, cg: &RefCodeGen) -> Option<proc_macro2::TokenStream> {
        let ty = &cg.ty;
        let field_name = &cg.field.name;
        let core = cg.std_lib.core();
        self.0.map_ref(|| {
            quote! {
                #[automatically_derived]
                impl ::#core::fmt::Debug for #ty {
                    #[inline]
                    fn fmt(&self, f: &mut ::#core::fmt::Formatter) -> ::#core::fmt::Result {
                        <str as ::#core::fmt::Debug>::fmt(&self.#field_name, f)
                    }
                }
            }
        })
    }
}

#[derive(Debug)]
pub struct ImplOrd(DelegatingImplOption);

impl Default for ImplOrd {
    fn default() -> Self {
        Self(DelegatingImplOption::Implement)
    }
}

impl From<DelegatingImplOption> for ImplOrd {
    fn from(opt: DelegatingImplOption) -> Self {
        Self(opt)
    }
}

impl ToImpl for ImplOrd {
    fn to_owned_impl(&self, cg: &OwnedCodeGen) -> Option<proc_macro2::TokenStream> {
        let ty = &cg.ty;
        let field_name = &cg.field.name;
        let core = cg.std_lib.core();
        self.0.map_owned(|| quote! {
            #[automatically_derived]
            impl ::#core::cmp::Ord for #ty {
                #[inline]
                fn cmp(&self, other: &Self) -> ::#core::cmp::Ordering {
                    ::#core::cmp::Ord::cmp(&self.#field_name, &other.#field_name)
                }
            }

            #[automatically_derived]
            impl ::#core::cmp::PartialOrd for #ty {
                #[inline]
                fn partial_cmp(&self, other: &Self) -> ::#core::option::Option<::#core::cmp::Ordering> {
                    ::#core::cmp::PartialOrd::partial_cmp(&self.#field_name, &other.#field_name)
                }
            }
        })
    }

    fn to_borrowed_impl(&self, _cg: &RefCodeGen) -> Option<proc_macro2::TokenStream> {
        self.0.map_ref(|| quote! { #[derive(PartialOrd, Ord)] })
    }
}

#[derive(Debug)]
pub struct ImplSerde(ImplOption);

impl Default for ImplSerde {
    fn default() -> Self {
        Self(ImplOption::Omit)
    }
}

impl From<ImplOption> for ImplSerde {
    fn from(opt: ImplOption) -> Self {
        Self(opt)
    }
}

impl ToImpl for ImplSerde {
    fn to_owned_impl(&self, cg: &OwnedCodeGen) -> Option<proc_macro2::TokenStream> {
        self.0.map(|| {
            let handle_failure = cg.check_mode.serde_err_handler();

            let name = cg.ty;
            let field_name = &cg.field.name;
            let wrapped_type = &cg.field.ty;

            quote! {
                #[automatically_derived]
                impl ::serde::Serialize for #name {
                    fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                        <#wrapped_type as ::serde::Serialize>::serialize(&self.#field_name, serializer)
                    }
                }

                #[allow(clippy::needless_question_mark, clippy::unsafe_derive_deserialize)]
                #[automatically_derived]
                impl<'de> ::serde::Deserialize<'de> for #name {
                    fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                        let raw = <#wrapped_type as ::serde::Deserialize<'de>>::deserialize(deserializer)?;
                        Ok(Self::new(raw)#handle_failure)
                    }
                }
            }
        })
    }

    fn to_borrowed_impl(&self, cg: &RefCodeGen) -> Option<proc_macro2::TokenStream> {
        self.0.map(|| {
            let ty = &cg.ty;
            let check_mode = cg.check_mode;
            let core = cg.std_lib.core();
            let alloc = cg.std_lib.alloc();

            let handle_failure = check_mode.serde_err_handler();

            let deserialize_boxed = cg.owned_ty.map(|owned_ty| {
                quote! {
                    #[automatically_derived]
                    impl<'de> ::serde::Deserialize<'de> for ::#alloc::boxed::Box<#ty> {
                        fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> ::#core::result::Result<Self, D::Error> {
                            let owned = <#owned_ty as ::serde::Deserialize<'de>>::deserialize(deserializer)?;
                            ::#core::result::Result::Ok(owned.into_boxed_ref())
                        }
                    }
                }
            });

            let deserialize = if matches!(check_mode, CheckMode::Normalize(_)) {
                let deserialize_doc = format!(
                    "Deserializes a `{ty}` in normalized form\n\
                    \n\
                    This deserializer _requires_ that the value already be in normalized form. \
                    If values may require normalization, then deserialized as [`{owned}`] or \
                    [`Cow<{ty}>`][{alloc}::borrow::Cow] instead.",
                    ty = ty.to_token_stream(),
                    owned = cg.owned_ty.expect("normalize not available if no owned").to_token_stream(),
                );

                quote! {
                    // impl<'de: 'a, 'a> ::serde::Deserialize<'de> for ::#alloc::borrow::Cow<'a, #name> {
                    //     fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> ::#core::result::Result<Self, D::Error> {
                    //         let raw = <&str as ::serde::Deserialize<'de>>::deserialize(deserializer)?;
                    //         ::#core::result::Result::Ok(#name::from_str(raw)#handle_failure)
                    //     }
                    // }
                    //
                    #[doc = #deserialize_doc]
                    #[allow(clippy::needless_question_mark, clippy::unsafe_derive_deserialize)]
                    #[automatically_derived]
                    impl<'de: 'a, 'a> ::serde::Deserialize<'de> for &'a #ty {
                        fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> ::#core::result::Result<Self, D::Error> {
                            let raw = <&str as ::serde::Deserialize<'de>>::deserialize(deserializer)?;
                            ::#core::result::Result::Ok(#ty::from_normalized_str(raw)#handle_failure)
                        }
                    }
                }
            } else {
                quote! {
                    #[allow(clippy::needless_question_mark, clippy::unsafe_derive_deserialize)]
                    #[automatically_derived]
                    impl<'de: 'a, 'a> ::serde::Deserialize<'de> for &'a #ty {
                        fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> ::#core::result::Result<Self, D::Error> {
                            let raw = <&str as ::serde::Deserialize<'de>>::deserialize(deserializer)?;
                            ::#core::result::Result::Ok(#ty::from_str(raw)#handle_failure)
                        }
                    }
                }
            };

            quote! {
                #[automatically_derived]
                impl ::serde::Serialize for #ty {
                    fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> ::#core::result::Result<S::Ok, S::Error> {
                        <str as ::serde::Serialize>::serialize(self.as_str(), serializer)
                    }
                }

                #deserialize
                #deserialize_boxed
            }
        })
    }
}

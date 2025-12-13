use quote::{ToTokens, quote};
use unsynn::Ident;

use super::{AttrList, CheckMode, Field, Impls, StdLib, impls::ToImpl};

pub struct OwnedCodeGen<'a> {
    #[allow(dead_code)]
    pub common_attrs: &'a [crate::grammar::Attribute],
    pub attrs: &'a AttrList,
    pub body: &'a crate::grammar::ItemStruct,
    pub ty: &'a Ident,
    pub field: &'a Field,
    pub check_mode: &'a CheckMode,
    pub ref_ty: &'a crate::grammar::Type,
    pub std_lib: &'a StdLib,
    pub expose_inner: bool,
    pub impls: &'a Impls,
}

impl<'a> OwnedCodeGen<'a> {
    fn constructor(&self) -> proc_macro2::TokenStream {
        match &self.check_mode {
            CheckMode::None => self.infallible_constructor(),
            CheckMode::Validate(validator) => self.fallible_constructor(validator),
            CheckMode::Normalize(normalizer) => self.normalized_constructor(normalizer),
        }
    }

    fn infallible_constructor(&self) -> proc_macro2::TokenStream {
        let doc_comment = format!("Constructs a new {}", self.ty);
        let static_doc_comment = format!("{doc_comment} from a static reference");

        let param = self.field.name.input_name();
        let create = self.field.self_constructor();
        let ref_ty = self.ref_ty;
        let field_ty = &self.field.ty;
        let alloc = self.std_lib.alloc();

        let vis = self
            .expose_inner
            .then(|| proc_macro2::Ident::new("pub", proc_macro2::Span::call_site()));

        quote! {
            #[doc = #doc_comment]
            #[inline]
            #vis const fn new(#param: #field_ty) -> Self {
                #create
            }

            #[inline]
            #[doc = #static_doc_comment]
            #[track_caller]
            pub fn from_static(raw: &'static str) -> Self {
                ::#alloc::borrow::ToOwned::to_owned(#ref_ty::from_static(raw))
            }
        }
    }

    fn fallible_constructor(&self, validator: &crate::grammar::Type) -> proc_macro2::TokenStream {
        let validator_tokens = validator.to_token_stream();
        let doc_comment = format!(
            "Constructs a new {} if it conforms to [`{}`]",
            self.ty, validator_tokens
        );

        let static_doc_comment = format!(
            "Constructs a new {} from a static reference if it conforms to [`{}`]",
            self.ty, validator_tokens
        );

        let doc_comment_unsafe = format!(
            "Constructs a new {} without validation\n\n# Safety\n\nConsumers of this function \
             must ensure that values conform to [`{}`]. Failure to maintain this invariant may \
             lead to undefined behavior.",
            self.ty, validator_tokens
        );

        let validator = crate::as_validator(validator);
        let param = self.field.name.input_name();
        let create = self.field.self_constructor();
        let ref_ty = self.ref_ty;
        let field_ty = &self.field.ty;
        let core = self.std_lib.core();
        let alloc = self.std_lib.alloc();

        let vis = self
            .expose_inner
            .then(|| proc_macro2::Ident::new("pub", proc_macro2::Span::call_site()));

        quote! {
            #[doc = #doc_comment]
            #[inline]
            #vis fn new(#param: #field_ty) -> ::#core::result::Result<Self, #validator::Error> {
                #validator::validate(#param.as_ref())?;
                ::#core::result::Result::Ok(#create)
            }

            #[doc = #doc_comment_unsafe]
            #[allow(unsafe_code)]
            #[inline]
            #vis const unsafe fn new_unchecked(#param: #field_ty) -> Self {
                #create
            }

            #[inline]
            #[doc = #static_doc_comment]
            #[doc = ""]
            #[doc = "# Panics"]
            #[doc = ""]
            #[doc = "This function will panic if the provided raw string is not valid."]
            #[track_caller]
            pub fn from_static(raw: &'static str) -> Self {
                ::#alloc::borrow::ToOwned::to_owned(#ref_ty::from_static(raw))
            }
        }
    }

    fn normalized_constructor(
        &self,
        normalizer: &crate::grammar::Type,
    ) -> proc_macro2::TokenStream {
        let normalizer_tokens = normalizer.to_token_stream();
        let doc_comment = format!(
            "Constructs a new {} if it conforms to [`{}`] and normalizes the input",
            self.ty, normalizer_tokens
        );

        let static_doc_comment = format!(
            "Constructs a new {} from a static reference if it conforms to [`{}`], normalizing \
             the input",
            self.ty, normalizer_tokens
        );

        let doc_comment_unsafe = format!(
            "Constructs a new {} without validation or normalization\n\n# Safety\n\nConsumers of \
             this function must ensure that values conform to [`{}`] and are in normalized form. \
             Failure to maintain this invariant may lead to undefined behavior.",
            self.ty, normalizer_tokens
        );

        let ty = self.ty;
        let validator = crate::as_validator(normalizer);
        let normalizer = crate::as_normalizer(normalizer);
        let param = self.field.name.input_name();
        let create = self.field.self_constructor();
        let ref_ty = self.ref_ty;
        let field_ty = &self.field.ty;
        let core = self.std_lib.core();

        let vis = self
            .expose_inner
            .then(|| proc_macro2::Ident::new("pub", proc_macro2::Span::call_site()));

        quote! {
            #[doc = #doc_comment]
            #[inline]
            #vis fn new(#param: #field_ty) -> ::#core::result::Result<Self, #validator::Error> {
                let #param = ::#core::convert::From::from(#normalizer::normalize(#param.as_ref())?);
                ::#core::result::Result::Ok(#create)
            }

            #[doc = #doc_comment_unsafe]
            #[allow(unsafe_code)]
            #[inline]
            #vis const unsafe fn new_unchecked(#param: #field_ty) -> Self {
                #create
            }

            #[inline]
            #[doc = #static_doc_comment]
            #[doc = ""]
            #[doc = "# Panics"]
            #[doc = ""]
            #[doc = "This function will panic if the provided raw string is not valid."]
            #[track_caller]
            pub fn from_static(raw: &'static str) -> Self {
                #ref_ty::from_str(raw).expect(concat!("invalid ", stringify!(#ty))).into_owned()
            }
        }
    }

    fn make_into_boxed_ref(&self) -> proc_macro2::TokenStream {
        let doc = format!(
            "Converts this `{}` into a [`Box<{}>`]\n\nThis will drop any excess capacity.",
            self.ty,
            self.ref_ty.to_token_stream(),
        );

        let ref_type = self.ref_ty;
        let field = &self.field.name;
        let alloc = self.std_lib.alloc();
        let box_pointer_reinterpret_safety_comment = {
            let doc = format!(
                "SAFETY: `{ty}` is `#[repr(transparent)]` around a single `str` field, so a `*mut \
                 str` can be safely reinterpreted as a `*mut {ty}`",
                ty = self.ref_ty.to_token_stream(),
            );

            quote! {
                #[doc = #doc]
                fn ptr_safety_comment() {}
            }
        };

        quote! {
            #[doc = #doc]
            #[allow(unsafe_code)]
            #[inline]
            pub fn into_boxed_ref(self) -> ::#alloc::boxed::Box<#ref_type> {
                #box_pointer_reinterpret_safety_comment
                let box_str = ::#alloc::string::String::from(self.#field).into_boxed_str();
                unsafe { ::#alloc::boxed::Box::from_raw(::#alloc::boxed::Box::into_raw(box_str) as *mut #ref_type) }
            }
        }
    }

    fn make_take(&self) -> proc_macro2::TokenStream {
        let field = &self.field.name;
        let field_ty = &self.field.ty;
        let doc = format!(
            "Unwraps the underlying [`{}`] value",
            field_ty.to_token_stream()
        );

        let vis = self
            .expose_inner
            .then(|| proc_macro2::Ident::new("pub", proc_macro2::Span::call_site()));

        quote! {
            #[doc = #doc]
            #[inline]
            #vis fn take(self) -> #field_ty {
                self.#field
            }
        }
    }

    fn inherent(&self) -> proc_macro2::TokenStream {
        let name = self.ty;
        let constructor = self.constructor();
        let into_boxed_ref = self.make_into_boxed_ref();
        let into_string = self.make_take();

        quote! {
            #[automatically_derived]
            impl #name {
                #constructor
                #into_boxed_ref
                #into_string
            }
        }
    }

    fn common_conversion(&self) -> proc_macro2::TokenStream {
        let ty = self.ty;
        let field_name = &self.field.name;
        let ref_ty = self.ref_ty;
        let core = self.std_lib.core();
        let alloc = self.std_lib.alloc();

        quote! {
            #[automatically_derived]
            impl ::#core::convert::From<&'_ #ref_ty> for #ty {
                #[inline]
                fn from(s: &#ref_ty) -> Self {
                    ::#alloc::borrow::ToOwned::to_owned(s)
                }
            }

            #[automatically_derived]
            impl ::#core::convert::From<#ty> for ::#alloc::string::String {
                #[inline]
                fn from(s: #ty) -> Self {
                    ::#core::convert::From::from(s.#field_name)
                }
            }

            #[automatically_derived]
            impl ::#core::borrow::Borrow<#ref_ty> for #ty {
                #[inline]
                fn borrow(&self) -> &#ref_ty {
                    ::#core::ops::Deref::deref(self)
                }
            }

            #[automatically_derived]
            impl ::#core::convert::AsRef<#ref_ty> for #ty {
                #[inline]
                fn as_ref(&self) -> &#ref_ty {
                    ::#core::ops::Deref::deref(self)
                }
            }

            #[automatically_derived]
            impl ::#core::convert::AsRef<str> for #ty {
                #[inline]
                fn as_ref(&self) -> &str {
                    self.as_str()
                }
            }


            #[automatically_derived]
            impl ::#core::convert::From<#ty> for ::#alloc::boxed::Box<#ref_ty> {
                #[inline]
                fn from(r: #ty) -> Self {
                    r.into_boxed_ref()
                }
            }

            #[automatically_derived]
            impl ::#core::convert::From<::#alloc::boxed::Box<#ref_ty>> for #ty {
                #[inline]
                fn from(r: ::#alloc::boxed::Box<#ref_ty>) -> Self {
                    r.into_owned()
                }
            }

            #[automatically_derived]
            impl<'a> ::#core::convert::From<::#alloc::borrow::Cow<'a, #ref_ty>> for #ty {
                #[inline]
                fn from(r: ::#alloc::borrow::Cow<'a, #ref_ty>) -> Self {
                    match r {
                        ::#alloc::borrow::Cow::Borrowed(b) => ::#alloc::borrow::ToOwned::to_owned(b),
                        ::#alloc::borrow::Cow::Owned(o) => o,
                    }
                }
            }

            #[automatically_derived]
            impl<'a> ::#core::convert::From<#ty> for ::#alloc::borrow::Cow<'a, #ref_ty> {
                #[inline]
                fn from(owned: #ty) -> Self {
                    ::#alloc::borrow::Cow::Owned(owned)
                }
            }
        }
    }

    fn infallible_conversion(&self) -> proc_macro2::TokenStream {
        let ty = self.ty;
        let ref_ty = self.ref_ty;
        let field_name = &self.field.name;
        let core = self.std_lib.core();
        let alloc = self.std_lib.alloc();

        quote! {
            #[automatically_derived]
            impl ::#core::convert::From<::#alloc::string::String> for #ty {
                #[inline]
                fn from(s: ::#alloc::string::String) -> Self {
                    Self::new(From::from(s))
                }
            }

            #[automatically_derived]
            impl ::#core::convert::From<&'_ str> for #ty {
                #[inline]
                fn from(s: &str) -> Self {
                    Self::new(::#core::convert::From::from(s))
                }
            }

            #[automatically_derived]
            impl ::#core::convert::From<::#alloc::boxed::Box<str>> for #ty {
                #[inline]
                fn from(s: ::#alloc::boxed::Box<str>) -> Self {
                    Self::new(::#core::convert::From::from(s))
                }
            }

            #[automatically_derived]
            impl ::#core::str::FromStr for #ty {
                type Err = ::#core::convert::Infallible;

                #[inline]
                fn from_str(s: &str) -> ::#core::result::Result<Self, Self::Err> {
                    ::#core::result::Result::Ok(::#core::convert::From::from(s))
                }
            }

            #[automatically_derived]
            impl ::#core::borrow::Borrow<str> for #ty {
                #[inline]
                fn borrow(&self) -> &str {
                    self.as_str()
                }
            }

            #[automatically_derived]
            impl ::#core::ops::Deref for #ty {
                type Target = #ref_ty;

                #[inline]
                fn deref(&self) -> &Self::Target {
                    #ref_ty::from_str(::#core::convert::AsRef::as_ref(&self.#field_name))
                }
            }
        }
    }

    fn unchecked_safety_comment(is_normalized: bool) -> proc_macro2::TokenStream {
        let doc = format!(
            "SAFETY: The value was satisfies the type's invariant and conforms to the required \
             implicit contracts of the {}.",
            if is_normalized {
                "normalizer"
            } else {
                "validator"
            },
        );

        quote! {
            #[doc = #doc]
            fn unchecked_safety_comment() {}
        }
    }

    fn fallible_conversion(&self, validator: &crate::grammar::Type) -> proc_macro2::TokenStream {
        let ty = self.ty;
        let ref_ty = self.ref_ty;
        let field_name = &self.field.name;
        let field_ty = &self.field.ty;
        let validator = crate::as_validator(validator);
        let core = self.std_lib.core();
        let alloc = self.std_lib.alloc();
        let unchecked_safety_comment = Self::unchecked_safety_comment(false);

        quote! {
            #[automatically_derived]
            impl ::#core::convert::TryFrom<::#alloc::string::String> for #ty {
                type Error = #validator::Error;

                #[inline]
                fn try_from(s: ::#alloc::string::String) -> ::#core::result::Result<Self, Self::Error> {
                    const fn ensure_try_from_string_error_converts_to_validator_error<T: ?Sized + From<<#field_ty as ::#core::convert::TryFrom<::#alloc::string::String>>::Error>>() {}
                    ensure_try_from_string_error_converts_to_validator_error::<Self::Error>();

                    Self::new(::#core::convert::TryFrom::try_from(s)?)
                }
            }

            #[automatically_derived]
            impl ::#core::convert::TryFrom<&'_ str> for #ty {
                type Error = #validator::Error;

                #[inline]
                fn try_from(s: &str) -> ::#core::result::Result<Self, Self::Error> {
                    let ref_ty = #ref_ty::from_str(s)?;
                    ::#core::result::Result::Ok(::#alloc::borrow::ToOwned::to_owned(ref_ty))
                }
            }

            #[automatically_derived]
            impl ::#core::str::FromStr for #ty {
                type Err = #validator::Error;

                #[inline]
                fn from_str(s: &str) -> ::#core::result::Result<Self, Self::Err> {
                    let ref_ty = #ref_ty::from_str(s)?;
                    ::#core::result::Result::Ok(::#alloc::borrow::ToOwned::to_owned(ref_ty))
                }
            }

            #[automatically_derived]
            impl ::#core::borrow::Borrow<str> for #ty {
                #[inline]
                fn borrow(&self) -> &str {
                    self.as_str()
                }
            }

            #[automatically_derived]
            impl ::#core::ops::Deref for #ty {
                type Target = #ref_ty;

                #[allow(unsafe_code)]
                #[inline]
                fn deref(&self) -> &Self::Target {
                    #unchecked_safety_comment
                    unsafe { #ref_ty::from_str_unchecked(::#core::convert::AsRef::as_ref(&self.#field_name)) }
                }
            }
        }
    }

    fn normalized_conversion(&self, normalizer: &crate::grammar::Type) -> proc_macro2::TokenStream {
        let ty = self.ty;
        let ref_ty = self.ref_ty;
        let field_name = &self.field.name;
        let field_ty = &self.field.ty;
        let validator = crate::as_validator(normalizer);
        let core = self.std_lib.core();
        let alloc = self.std_lib.alloc();
        let unchecked_safety_comment = Self::unchecked_safety_comment(true);

        quote! {
            #[automatically_derived]
            impl ::#core::convert::TryFrom<::#alloc::string::String> for #ty {
                type Error = #validator::Error;

                #[inline]
                fn try_from(s: ::#alloc::string::String) -> ::#core::result::Result<Self, Self::Error> {
                    const fn ensure_try_from_string_error_converts_to_validator_error<T: ?Sized + From<<#field_ty as ::#core::convert::TryFrom<::#alloc::string::String>>::Error>>() {}
                    ensure_try_from_string_error_converts_to_validator_error::<Self::Error>();

                    Self::new(::#core::convert::TryFrom::try_from(s)?)
                }
            }

            #[automatically_derived]
            impl ::#core::convert::TryFrom<&'_ str> for #ty {
                type Error = #validator::Error;

                #[inline]
                fn try_from(s: &str) -> ::#core::result::Result<Self, Self::Error> {
                    let ref_ty = #ref_ty::from_str(s)?;
                    ::#core::result::Result::Ok(ref_ty.into_owned())
                }
            }

            #[automatically_derived]
            impl ::#core::str::FromStr for #ty {
                type Err = #validator::Error;

                #[inline]
                fn from_str(s: &str) -> ::#core::result::Result<Self, Self::Err> {
                    let ref_ty = #ref_ty::from_str(s)?;
                    ::#core::result::Result::Ok(ref_ty.into_owned())
                }
            }

            #[automatically_derived]
            impl ::#core::ops::Deref for #ty {
                type Target = #ref_ty;

                #[allow(unsafe_code)]
                #[inline]
                fn deref(&self) -> &Self::Target {
                    #unchecked_safety_comment
                    unsafe { #ref_ty::from_str_unchecked(&self.#field_name) }
                }
            }
        }
    }

    fn conversion(&self) -> proc_macro2::TokenStream {
        let common = self.common_conversion();
        let convert = match &self.check_mode {
            CheckMode::None => self.infallible_conversion(),
            CheckMode::Validate(validator) => self.fallible_conversion(validator),
            CheckMode::Normalize(normalizer) => self.normalized_conversion(normalizer),
        };

        quote! {
            #common
            #convert
        }
    }

    pub fn tokens(&self) -> proc_macro2::TokenStream {
        let clone = self.impls.clone.to_owned_impl(self);
        let display = self.impls.display.to_owned_impl(self);
        let debug = self.impls.debug.to_owned_impl(self);
        let ord = self.impls.ord.to_owned_impl(self);
        let serde = self.impls.serde.to_owned_impl(self);

        let owned_attrs: proc_macro2::TokenStream =
            self.attrs.iter().map(|a| quote! {#[#a]}).collect();
        let body = &self.body;
        let inherent = self.inherent();
        let conversion = self.conversion();

        // Only add transparent attribute for tuple structs (unnamed fields)
        let facet_attr = match &self.field.name {
            super::FieldName::Unnamed => quote! {
                #[facet(crate = ::strid::facet, transparent)]
            },
            super::FieldName::Named(_) => quote! {
                #[facet(crate = ::strid::facet)]
            },
        };

        quote! {
            #clone
            #[derive(Hash, PartialEq, Eq, ::strid::facet::Facet)]
            #facet_attr
            #[repr(transparent)]
            #owned_attrs
            #body

            #inherent
            #conversion
            #debug
            #display
            #ord
            #serde
        }
    }
}

use quote::{ToTokens, TokenStreamExt, quote};

use super::{AttrList, CheckMode, Field, FieldName, Impls, StdLib, impls::ToImpl};

pub struct RefCodeGen<'a> {
    pub doc: &'a [syn::Lit],
    pub common_attrs: &'a [syn::Attribute],
    pub attrs: &'a AttrList,
    pub vis: &'a syn::Visibility,
    pub ty: &'a syn::Type,
    pub ident: syn::Ident,
    pub field: Field,
    pub check_mode: &'a CheckMode,
    pub owned_ty: Option<&'a syn::Ident>,
    pub std_lib: &'a StdLib,
    pub impls: &'a Impls,
}

impl<'a> RefCodeGen<'a> {
    fn inherent(&self) -> proc_macro2::TokenStream {
        let ty = &self.ty;
        let field_name = &self.field.name;
        let inherent = self.check_inherent();

        quote! {
            #[automatically_derived]
            impl #ty {
                #inherent

                /// Provides access to the underlying value as a string slice.
                #[inline]
                pub const fn as_str(&self) -> &str {
                    &self.#field_name
                }
            }
        }
    }

    fn check_inherent(&self) -> proc_macro2::TokenStream {
        match self.check_mode {
            CheckMode::None => self.infallible_inherent(),
            CheckMode::Validate(validator) => self.fallible_inherent(validator),
            CheckMode::Normalize(normalizer) => self.normalized_inherent(normalizer),
        }
    }

    fn pointer_reinterpret_safety_comment(&self, is_mut: bool) -> proc_macro2::TokenStream {
        let doc = format!(
            "SAFETY: `{ty}` is `#[repr(transparent)]` around a single `str` field, so a `*{ptr} \
             str` can be safely reinterpreted as a `*{ptr} {ty}`",
            ty = self.ident,
            ptr = if is_mut { "mut" } else { "const" },
        );

        quote! {
            #[doc = #doc]
            fn ptr_safety_comment() {}
        }
    }

    fn unchecked_safety_comment(is_normalized: bool) -> proc_macro2::TokenStream {
        let doc = format!(
            "SAFETY: The value was just checked and found to already conform to the required \
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

    fn infallible_inherent(&self) -> proc_macro2::TokenStream {
        let ty = &self.ty;
        let core = self.std_lib.core();
        let alloc = self.std_lib.alloc();

        let doc_comment = format!(
            "Transparently reinterprets the string slice as a strongly-typed {}",
            self.ident
        );

        let static_doc_comment = format!(
            "Transparently reinterprets the static string slice as a strongly-typed {}",
            self.ident
        );

        let pointer_reinterpret_safety_comment = self.pointer_reinterpret_safety_comment(false);

        let into_owned = self.owned_ty.map(|owned_ty| {
            let into_owned_doc = format!(
                "Converts a [`Box<{}>`] into a [`{}`] without copying or allocating",
                self.ident, owned_ty,
            );

            let box_pointer_reinterpret_safety_comment =
                self.pointer_reinterpret_safety_comment(true);

            quote! {
                #[allow(unsafe_code)]
                #[inline]
                #[doc = #into_owned_doc]
                pub fn into_owned(self: ::#alloc::boxed::Box<#ty>) -> #owned_ty {
                    #box_pointer_reinterpret_safety_comment
                    let raw = ::#alloc::boxed::Box::into_raw(self);
                    let boxed = unsafe { ::#alloc::boxed::Box::from_raw(raw as *mut str) };
                    #owned_ty::new(::#core::convert::From::from(boxed))
                }
            }
        });

        quote! {
            #[allow(unsafe_code)]
            #[inline]
            #[doc = #doc_comment]
            pub const fn from_str(raw: &str) -> &Self {
                let ptr: *const str = raw;
                #pointer_reinterpret_safety_comment
                unsafe {
                    &*(ptr as *const Self)
                }
            }

            #[inline]
            #[doc = #static_doc_comment]
            #[track_caller]
            pub const fn from_static(raw: &'static str) -> &'static Self {
                Self::from_str(raw)
            }

            #into_owned
        }
    }

    fn fallible_inherent(&self, validator: &syn::Type) -> proc_macro2::TokenStream {
        let doc_comment = format!(
            "Transparently reinterprets the string slice as a strongly-typed {} if it conforms to \
             [`{}`]",
            self.ident,
            validator.to_token_stream(),
        );

        let static_doc_comment = format!(
            "Transparently reinterprets the static string slice as a strongly-typed {} if it \
             conforms to [`{}`]",
            self.ident,
            validator.to_token_stream(),
        );

        let doc_comment_unsafe = format!(
            "Transparently reinterprets the string slice as a strongly-typed {} without validating",
            self.ident,
        );

        let ty = &self.ty;
        let core = self.std_lib.core();
        let alloc = self.std_lib.alloc();
        let unchecked_safety_comment = Self::unchecked_safety_comment(false);
        let pointer_reinterpret_safety_comment = self.pointer_reinterpret_safety_comment(false);
        let into_owned = self.owned_ty.map(|owned_ty| {
            let into_owned_doc = format!(
                "Converts a [`Box<{}>`] into a [`{}`] without copying or allocating",
                self.ident, owned_ty,
            );

            let box_pointer_reinterpret_safety_comment =
                self.pointer_reinterpret_safety_comment(true);

            quote! {
                #[allow(unsafe_code)]
                #[inline]
                #[doc = #into_owned_doc]
                pub fn into_owned(self: ::#alloc::boxed::Box<#ty>) -> #owned_ty {
                    #box_pointer_reinterpret_safety_comment
                    let raw = ::#alloc::boxed::Box::into_raw(self);
                    let boxed = unsafe { ::#alloc::boxed::Box::from_raw(raw as *mut str) };
                    let s = ::#core::convert::From::from(boxed);
                    #unchecked_safety_comment
                    unsafe { #owned_ty::new_unchecked(s) }
                }
            }
        });

        let validator = crate::as_validator(validator);

        quote! {
            #[allow(unsafe_code)]
            #[inline]
            #[doc = #doc_comment]
            pub fn from_str(raw: &str) -> ::#core::result::Result<&Self, #validator::Error> {
                #validator::validate(raw)?;
                #unchecked_safety_comment
                ::#core::result::Result::Ok(unsafe { Self::from_str_unchecked(raw) })
            }

            #[allow(unsafe_code)]
            #[inline]
            #[doc = #doc_comment_unsafe]
            pub const unsafe fn from_str_unchecked(raw: &str) -> &Self {
                #pointer_reinterpret_safety_comment
                &*(raw as *const str as *const Self)
            }

            #[inline]
            #[doc = #static_doc_comment]
            #[doc = ""]
            #[doc = "# Panics"]
            #[doc = ""]
            #[doc = "This function will panic if the provided raw string is not valid."]
            #[track_caller]
            pub fn from_static(raw: &'static str) -> &'static Self {
                Self::from_str(raw).expect(concat!("invalid ", stringify!(#ty)))
            }

            #into_owned
        }
    }

    fn normalized_inherent(&self, normalizer: &syn::Type) -> proc_macro2::TokenStream {
        let doc_comment = format!(
            "Transparently reinterprets the string slice as a strongly-typed {} if it conforms to \
             [`{}`], normalizing if necessary",
            self.ident,
            normalizer.to_token_stream(),
        );

        let static_doc_comment = format!(
            "Transparently reinterprets a static string slice as a strongly-typed {} if it \
             conforms to [`{}`], normalizing if necessary",
            self.ident,
            normalizer.to_token_stream(),
        );

        let doc_comment_norm = format!(
            "Transparently reinterprets the string slice as a strongly-typed `{}` if it conforms \
             to [`{}`], producing an error if normalization is necessary",
            self.ident,
            normalizer.to_token_stream(),
        );

        let doc_comment_unsafe = format!(
            "Transparently reinterprets the string slice as a strongly-typed `{}` without \
             validating\n\n# Safety\n\nCalls to this function must ensure that the value being \
             passed conforms to [`{}`] and is already in normalized form. Failure to do this may \
             result in undefined behavior if other code relies on this invariant.",
            self.ident,
            normalizer.to_token_stream(),
        );

        let doc_comment_cow_unsafe = format!(
            "Transparently reinterprets the [`Cow<str>`][std::borrow::Cow] as a strongly-typed \
             [`Cow`][std::borrow::Cow]`<{}>` without validating\n\n# Safety\n\nCalls to this \
             function must ensure that the value being passed conforms to [`{}`] and is already \
             in normalized form. Failure to do this may result in undefined behavior if other \
             code relies on this invariant.",
            self.ident,
            normalizer.to_token_stream(),
        );

        let ty = &self.ty;
        let core = self.std_lib.core();
        let alloc = self.std_lib.alloc();
        let unchecked_safety_comment = Self::unchecked_safety_comment(true);
        let pointer_reinterpret_safety_comment = self.pointer_reinterpret_safety_comment(false);

        let validator = crate::as_validator(normalizer);
        let normalizer = crate::as_normalizer(normalizer);

        let into_owned = self.owned_ty.map(|owned_ty| {
            let into_owned_doc = format!(
                "Converts a [`Box<{}>`] into a [`{}`] without copying or allocating",
                self.ident,
                owned_ty,
            );

            let box_pointer_reinterpret_safety_comment = self.pointer_reinterpret_safety_comment(true);

            quote! {
                #[allow(unsafe_code)]
                #[inline]
                #[doc = #doc_comment]
                pub fn from_str(raw: &str) -> ::#core::result::Result<::#alloc::borrow::Cow<Self>, #validator::Error> {
                    let cow = #normalizer::normalize(raw)?;
                    #unchecked_safety_comment
                    ::#core::result::Result::Ok(unsafe { Self::from_cow_str_unchecked(cow) })
                }

                #[allow(unsafe_code)]
                #[inline]
                #[doc = #doc_comment_cow_unsafe]
                unsafe fn from_cow_str_unchecked(cow: ::#alloc::borrow::Cow<str>) -> ::#alloc::borrow::Cow<Self> {
                    match cow {
                        ::#alloc::borrow::Cow::Borrowed(raw) => {
                            let value = unsafe { Self::from_str_unchecked(raw) };
                            ::#alloc::borrow::Cow::Borrowed(value)
                        }
                        ::#alloc::borrow::Cow::Owned(normalized) => {
                            let value = unsafe { #owned_ty::new_unchecked(::#core::convert::From::from(normalized)) };
                            ::#alloc::borrow::Cow::Owned(value)
                        }
                    }
                }

                #[allow(unsafe_code)]
                #[inline]
                #[doc = #into_owned_doc]
                pub fn into_owned(self: ::#alloc::boxed::Box<#ty>) -> #owned_ty {
                    #box_pointer_reinterpret_safety_comment
                    let raw = ::#alloc::boxed::Box::into_raw(self);
                    let boxed = unsafe { ::#alloc::boxed::Box::from_raw(raw as *mut str) };
                    let s = ::#core::convert::From::from(boxed);
                    #unchecked_safety_comment
                    unsafe { #owned_ty::new_unchecked(s) }
                }
            }
        });

        quote! {
            #[allow(unsafe_code)]
            #[inline]
            #[doc = #doc_comment_norm]
            pub fn from_normalized_str(raw: &str) -> ::#core::result::Result<&Self, #validator::Error> {
                #validator::validate(raw)?;
                #unchecked_safety_comment
                ::#core::result::Result::Ok(unsafe { Self::from_str_unchecked(raw) })
            }

            #[allow(unsafe_code)]
            #[inline]
            #[doc = #doc_comment_unsafe]
            pub const unsafe fn from_str_unchecked(raw: &str) -> &Self {
                #pointer_reinterpret_safety_comment
                &*(raw as *const str as *const Self)
            }

            #[inline]
            #[doc = #static_doc_comment]
            #[doc = ""]
            #[doc = "# Panics"]
            #[doc = ""]
            #[doc = "This function will panic if the provided raw string is not normalized."]
            #[track_caller]
            pub fn from_static(raw: &'static str) -> &'static Self {
                Self::from_normalized_str(raw).expect(concat!("non-normalized ", stringify!(#ty)))
            }

            #into_owned
        }
    }

    fn comparison(&self) -> Option<proc_macro2::TokenStream> {
        self.owned_ty.map(|owned_ty| {
            let ty = &self.ty;
            let core = self.std_lib.core();
            let alloc = self.std_lib.alloc();

            let create = match &self.field.name {
                FieldName::Unnamed => quote! { #owned_ty(self.0.into()) },
                FieldName::Named(field_name) => {
                    quote! { #owned_ty { #field_name: self.#field_name.into() } }
                }
            };

            quote! {
                #[automatically_derived]
                impl ::#alloc::borrow::ToOwned for #ty {
                    type Owned = #owned_ty;

                    #[inline]
                    fn to_owned(&self) -> Self::Owned {
                        #create
                    }
                }

                #[automatically_derived]
                impl ::#core::cmp::PartialEq<#ty> for #owned_ty {
                    #[inline]
                    fn eq(&self, other: &#ty) -> bool {
                        self.as_str() == other.as_str()
                    }
                }

                #[automatically_derived]
                impl ::#core::cmp::PartialEq<#owned_ty> for #ty {
                    #[inline]
                    fn eq(&self, other: &#owned_ty) -> bool {
                        self.as_str() == other.as_str()
                    }
                }

                #[automatically_derived]
                impl ::#core::cmp::PartialEq<&'_ #ty> for #owned_ty {
                    #[inline]
                    fn eq(&self, other: &&#ty) -> bool {
                        self.as_str() == other.as_str()
                    }
                }

                #[automatically_derived]
                impl ::#core::cmp::PartialEq<#owned_ty> for &'_ #ty {
                    #[inline]
                    fn eq(&self, other: &#owned_ty) -> bool {
                        self.as_str() == other.as_str()
                    }
                }
            }
        })
    }

    fn conversion(&self) -> proc_macro2::TokenStream {
        let ty = &self.ty;
        let field_name = &self.field.name;
        let core = self.std_lib.core();
        let alloc = self.std_lib.alloc();
        let pointer_reinterpret_safety_comment = self.pointer_reinterpret_safety_comment(false);

        let from_str = match &self.check_mode {
            CheckMode::None => quote! {
                #[automatically_derived]
                impl<'a> ::#core::convert::From<&'a str> for &'a #ty {
                    #[inline]
                    fn from(s: &'a str) -> &'a #ty {
                        #ty::from_str(s)
                    }
                }

                #[automatically_derived]
                impl ::#core::borrow::Borrow<str> for #ty {
                    #[inline]
                    fn borrow(&self) -> &str {
                        &self.#field_name
                    }
                }
            },
            CheckMode::Validate(validator) => {
                let validator = crate::as_validator(validator);
                quote! {
                    #[automatically_derived]
                    impl<'a> ::#core::convert::TryFrom<&'a str> for &'a #ty {
                        type Error = #validator::Error;

                        #[inline]
                        fn try_from(s: &'a str) -> ::#core::result::Result<&'a #ty, Self::Error> {
                            #ty::from_str(s)
                        }
                    }

                    #[automatically_derived]
                    impl ::#core::borrow::Borrow<str> for #ty {
                        #[inline]
                        fn borrow(&self) -> &str {
                            &self.#field_name
                        }
                    }
                }
            }
            CheckMode::Normalize(normalizer) => {
                let validator = crate::as_validator(normalizer);
                quote! {
                    #[automatically_derived]
                    impl<'a> ::#core::convert::TryFrom<&'a str> for &'a #ty {
                        type Error = #validator::Error;

                        #[inline]
                        fn try_from(s: &'a str) -> ::#core::result::Result<&'a #ty, Self::Error> {
                            #ty::from_normalized_str(s)
                        }
                    }
                }
            }
        };

        let alloc_from = self.owned_ty.is_some().then(|| {
            quote!{
                #[automatically_derived]
                impl<'a> ::#core::convert::From<&'a #ty> for ::#alloc::borrow::Cow<'a, #ty> {
                    #[inline]
                    fn from(r: &'a #ty) -> Self {
                        ::#alloc::borrow::Cow::Borrowed(r)
                    }
                }


                #[automatically_derived]
                impl<'a, 'b: 'a> ::#core::convert::From<&'a ::#alloc::borrow::Cow<'b, #ty>> for &'a #ty {
                    #[inline]
                    fn from(r: &'a ::#alloc::borrow::Cow<'b, #ty>) -> &'a #ty {
                        ::#core::borrow::Borrow::borrow(r)
                    }
                }

                #[automatically_derived]
                impl ::#core::convert::From<&'_ #ty> for ::#alloc::rc::Rc<#ty> {
                    #[allow(unsafe_code)]
                    #[inline]
                    fn from(r: &'_ #ty) -> Self {
                        #pointer_reinterpret_safety_comment
                        let rc = ::#alloc::rc::Rc::<str>::from(r.as_str());
                        unsafe { ::#alloc::rc::Rc::from_raw(::#alloc::rc::Rc::into_raw(rc) as *const #ty) }
                    }
                }

                #[automatically_derived]
                impl ::#core::convert::From<&'_ #ty> for ::#alloc::sync::Arc<#ty> {
                    #[allow(unsafe_code)]
                    #[inline]
                    fn from(r: &'_ #ty) -> Self {
                        #pointer_reinterpret_safety_comment
                        let arc = ::#alloc::sync::Arc::<str>::from(r.as_str());
                        unsafe { ::#alloc::sync::Arc::from_raw(::#alloc::sync::Arc::into_raw(arc) as *const #ty) }
                    }
                }
            }
        });

        quote! {
            #from_str

            #[automatically_derived]
            impl ::#core::convert::AsRef<str> for #ty {
                #[inline]
                fn as_ref(&self) -> &str {
                    &self.#field_name
                }
            }

            #alloc_from
        }
    }

    pub fn tokens(&self) -> proc_macro2::TokenStream {
        let inherent = self.inherent();
        let comparison = self.comparison();
        let conversion = self.conversion();
        let debug = self.impls.debug.to_borrowed_impl(self);
        let display = self.impls.display.to_borrowed_impl(self);
        let ord = self.impls.ord.to_borrowed_impl(self);
        let serde = self.impls.serde.to_borrowed_impl(self);

        let ref_doc: proc_macro2::TokenStream =
            self.doc.iter().map(|d| quote! { #[doc = #d] }).collect();
        let ref_attrs: proc_macro2::TokenStream =
            self.attrs.iter().map(|a| quote! {#[#a]}).collect();
        let common_attrs = {
            let mut attrs = proc_macro2::TokenStream::new();
            if self.doc.is_empty() {
                attrs.append_all(self.common_attrs);
            } else {
                attrs.append_all(self.common_attrs.iter().filter(|a| !is_doc_attribute(a)));
            }
            attrs
        };
        let vis = self.vis;
        let ty = &self.ty;
        let field_attrs = {
            let mut attrs = proc_macro2::TokenStream::new();
            attrs.append_all(&self.field.attrs);
            attrs
        };
        let body = match &self.field.name {
            FieldName::Named(name) => quote! ( { #field_attrs #name: str } ),
            FieldName::Unnamed => quote! { ( #field_attrs str ); },
        };

        quote! {
            #[repr(transparent)]
            #[derive(Hash, PartialEq, Eq)]
            #ord
            #ref_doc
            #ref_attrs
            #common_attrs
            #vis struct #ty #body

            #inherent
            #comparison
            #conversion
            #debug
            #display
            #serde
        }
    }
}

fn is_doc_attribute(attr: &syn::Attribute) -> bool {
    if let Some(ident) = attr.path().get_ident() {
        ident == "doc"
    } else {
        false
    }
}

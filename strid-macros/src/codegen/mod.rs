use quote::{ToTokens, TokenStreamExt, format_ident};
use symbol::{parse_expr_as_lit, parse_lit_into_string, parse_lit_into_type};
use syn::spanned::Spanned;

pub use self::{borrowed::RefCodeGen, owned::OwnedCodeGen};
use self::{
    check_mode::{CheckMode, IndefiniteCheckMode},
    impls::{DelegatingImplOption, ImplOption, Impls},
};

mod borrowed;
mod check_mode;
mod impls;
mod owned;
mod symbol;

pub type AttrList = syn::punctuated::Punctuated<syn::Meta, syn::Token![,]>;

#[derive(Clone, Debug)]
pub struct StdLib {
    core: proc_macro2::Ident,
    alloc: proc_macro2::Ident,
}

impl StdLib {
    pub fn no_std(span: proc_macro2::Span) -> Self {
        Self {
            core: proc_macro2::Ident::new("core", span),
            alloc: proc_macro2::Ident::new("alloc", span),
        }
    }

    pub fn core(&self) -> &proc_macro2::Ident {
        &self.core
    }

    pub fn alloc(&self) -> &proc_macro2::Ident {
        &self.alloc
    }
}

impl Default for StdLib {
    fn default() -> Self {
        Self {
            core: proc_macro2::Ident::new("std", proc_macro2::Span::call_site()),
            alloc: proc_macro2::Ident::new("std", proc_macro2::Span::call_site()),
        }
    }
}

pub struct Params {
    ref_ty: Option<syn::Type>,
    ref_doc: Vec<syn::Lit>,
    ref_attrs: AttrList,
    owned_attrs: AttrList,
    std_lib: StdLib,
    check_mode: IndefiniteCheckMode,
    expose_inner: bool,
    impls: Impls,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            ref_ty: None,
            ref_doc: Vec::new(),
            ref_attrs: AttrList::new(),
            owned_attrs: AttrList::new(),
            std_lib: StdLib::default(),
            check_mode: IndefiniteCheckMode::None,
            expose_inner: true,
            impls: Impls::default(),
        }
    }
}

impl syn::parse::Parse for Params {
    fn parse(input: syn::parse::ParseStream) -> Result<Self, syn::Error> {
        let mut params = Self::default();
        let args =
            syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated(input)?;

        for arg in args {
            match &arg {
                syn::Meta::NameValue(nv) if nv.path == symbol::REF => {
                    params.ref_ty = Some(parse_lit_into_type(
                        symbol::REF,
                        parse_expr_as_lit(&nv.value)?,
                    )?);
                }
                syn::Meta::NameValue(nv) if nv.path == symbol::VALIDATOR => {
                    let validator =
                        parse_lit_into_type(symbol::VALIDATOR, parse_expr_as_lit(&nv.value)?)?;
                    params
                        .check_mode
                        .try_set_validator(Some(validator))
                        .map_err(|s| syn::Error::new_spanned(nv, s))?;
                }
                syn::Meta::NameValue(nv) if nv.path == symbol::NORMALIZER => {
                    let normalizer =
                        parse_lit_into_type(symbol::NORMALIZER, parse_expr_as_lit(&nv.value)?)?;
                    params
                        .check_mode
                        .try_set_normalizer(Some(normalizer))
                        .map_err(|s| syn::Error::new_spanned(nv, s))?;
                }
                syn::Meta::NameValue(nv) if nv.path == symbol::REF_DOC => {
                    params
                        .ref_doc
                        .push(parse_expr_as_lit(&nv.value)?.to_owned());
                }
                syn::Meta::List(nv) if nv.path == symbol::REF_ATTR => {
                    params.ref_attrs.extend(nv.parse_args::<syn::Meta>());
                }
                syn::Meta::List(nv) if nv.path == symbol::OWNED_ATTR => {
                    params.owned_attrs.extend(nv.parse_args::<syn::Meta>());
                }
                syn::Meta::NameValue(nv) if nv.path == symbol::DEBUG => {
                    params.impls.debug =
                        parse_lit_into_string(symbol::DEBUG, parse_expr_as_lit(&nv.value)?)?
                            .parse::<DelegatingImplOption>()
                            .map_err(|e| syn::Error::new_spanned(&arg, e.to_owned()))?
                            .into();
                }
                syn::Meta::NameValue(nv) if nv.path == symbol::DISPLAY => {
                    params.impls.display =
                        parse_lit_into_string(symbol::DISPLAY, parse_expr_as_lit(&nv.value)?)?
                            .parse::<DelegatingImplOption>()
                            .map_err(|e| syn::Error::new_spanned(&arg, e.to_owned()))?
                            .into();
                }
                syn::Meta::NameValue(nv) if nv.path == symbol::ORD => {
                    params.impls.ord =
                        parse_lit_into_string(symbol::ORD, parse_expr_as_lit(&nv.value)?)?
                            .parse::<DelegatingImplOption>()
                            .map_err(|e| syn::Error::new_spanned(&arg, e.to_owned()))?
                            .into();
                }
                syn::Meta::NameValue(nv) if nv.path == symbol::CLONE => {
                    params.impls.clone =
                        parse_lit_into_string(symbol::CLONE, parse_expr_as_lit(&nv.value)?)?
                            .parse::<ImplOption>()
                            .map_err(|e| syn::Error::new_spanned(&arg, e.to_owned()))?
                            .into();
                }
                syn::Meta::NameValue(nv) if nv.path == symbol::SERDE => {
                    params.impls.serde =
                        parse_lit_into_string(symbol::SERDE, parse_expr_as_lit(&nv.value)?)?
                            .parse::<ImplOption>()
                            .map_err(|e| syn::Error::new_spanned(&arg, e.to_owned()))?
                            .into();
                }
                syn::Meta::Path(p) if p == symbol::SERDE => {
                    params.impls.serde = ImplOption::Implement.into();
                }
                syn::Meta::Path(p) if p == symbol::VALIDATOR => {
                    params
                        .check_mode
                        .try_set_validator(None)
                        .map_err(|s| syn::Error::new_spanned(p, s))?;
                }
                syn::Meta::Path(p) if p == symbol::NORMALIZER => {
                    params
                        .check_mode
                        .try_set_normalizer(None)
                        .map_err(|s| syn::Error::new_spanned(p, s))?;
                }
                syn::Meta::Path(p) if p == symbol::NO_STD => {
                    params.std_lib = StdLib::no_std(p.span());
                }
                syn::Meta::Path(p) if p == symbol::NO_EXPOSE => {
                    params.expose_inner = false;
                }
                syn::Meta::Path(path) | syn::Meta::NameValue(syn::MetaNameValue { path, .. }) => {
                    return Err(syn::Error::new_spanned(
                        &arg,
                        format!("unsupported argument `{}`", path.to_token_stream()),
                    ));
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        &arg,
                        "unsupported argument".to_string(),
                    ));
                }
            }
        }

        Ok(params)
    }
}

impl Params {
    pub fn build(self, mut body: syn::ItemStruct) -> Result<CodeGen, syn::Error> {
        let Params {
            ref_ty,
            ref_doc,
            ref_attrs,
            owned_attrs,
            std_lib,
            check_mode,
            expose_inner,
            impls,
        } = self;

        create_field_if_none(&mut body.fields);
        let (wrapped_type, field_ident, field_attrs) = get_field_info(&body.fields)?;
        let owned_ty = &body.ident;
        let ref_ty = ref_ty.unwrap_or_else(|| infer_ref_type_from_owned_name(owned_ty));
        let check_mode = check_mode.infer_validator_if_missing(owned_ty);
        let field = Field {
            attrs: field_attrs.to_owned(),
            name: field_ident
                .cloned()
                .map_or(FieldName::Unnamed, FieldName::Named),
            ty: wrapped_type.to_owned(),
        };

        Ok(CodeGen {
            check_mode,
            body,
            field,

            owned_attrs,

            ref_doc,
            ref_attrs,
            ref_ty,

            std_lib,
            expose_inner,
            impls,
        })
    }
}

pub struct ParamsRef {
    std_lib: StdLib,
    check_mode: IndefiniteCheckMode,
    impls: Impls,
}

impl Default for ParamsRef {
    fn default() -> Self {
        Self {
            std_lib: StdLib::default(),
            check_mode: IndefiniteCheckMode::None,
            impls: Impls::default(),
        }
    }
}

impl syn::parse::Parse for ParamsRef {
    fn parse(input: syn::parse::ParseStream) -> Result<Self, syn::Error> {
        let mut params = Self::default();
        let args =
            syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated(input)?;

        for arg in args {
            match &arg {
                syn::Meta::NameValue(nv) if nv.path == symbol::VALIDATOR => {
                    let validator =
                        parse_lit_into_type(symbol::VALIDATOR, parse_expr_as_lit(&nv.value)?)?;
                    params
                        .check_mode
                        .try_set_validator(Some(validator))
                        .map_err(|s| syn::Error::new_spanned(nv, s))?;
                }
                syn::Meta::NameValue(nv) if nv.path == symbol::DEBUG => {
                    params.impls.debug =
                        parse_lit_into_string(symbol::DEBUG, parse_expr_as_lit(&nv.value)?)?
                            .parse::<ImplOption>()
                            .map_err(|e| syn::Error::new_spanned(nv, e.to_owned()))
                            .map(DelegatingImplOption::from)?
                            .into();
                }
                syn::Meta::NameValue(nv) if nv.path == symbol::DISPLAY => {
                    params.impls.display =
                        parse_lit_into_string(symbol::DISPLAY, parse_expr_as_lit(&nv.value)?)?
                            .parse::<ImplOption>()
                            .map_err(|e| syn::Error::new_spanned(nv, e.to_owned()))
                            .map(DelegatingImplOption::from)?
                            .into();
                }
                syn::Meta::NameValue(nv) if nv.path == symbol::ORD => {
                    params.impls.ord =
                        parse_lit_into_string(symbol::ORD, parse_expr_as_lit(&nv.value)?)?
                            .parse::<ImplOption>()
                            .map_err(|e| syn::Error::new_spanned(nv, e.to_owned()))
                            .map(DelegatingImplOption::from)?
                            .into();
                }
                syn::Meta::NameValue(nv) if nv.path == symbol::SERDE => {
                    params.impls.serde =
                        parse_lit_into_string(symbol::SERDE, parse_expr_as_lit(&nv.value)?)?
                            .parse::<ImplOption>()
                            .map_err(|e| syn::Error::new_spanned(nv, e.to_owned()))?
                            .into();
                }
                syn::Meta::Path(p) if p == symbol::SERDE => {
                    params.impls.serde = ImplOption::Implement.into();
                }
                syn::Meta::Path(p) if p == symbol::VALIDATOR => {
                    params
                        .check_mode
                        .try_set_validator(None)
                        .map_err(|s| syn::Error::new_spanned(p, s))?;
                }
                syn::Meta::Path(p) if p == symbol::NO_STD => {
                    params.std_lib = StdLib::no_std(p.span());
                }
                syn::Meta::Path(path) | syn::Meta::NameValue(syn::MetaNameValue { path, .. }) => {
                    return Err(syn::Error::new_spanned(
                        &arg,
                        format!("unsupported argument `{}`", path.to_token_stream()),
                    ));
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        &arg,
                        "unsupported argument".to_string(),
                    ));
                }
            }
        }

        Ok(params)
    }
}

impl ParamsRef {
    pub fn build(self, body: &mut syn::ItemStruct) -> Result<proc_macro2::TokenStream, syn::Error> {
        let ParamsRef {
            std_lib,
            check_mode,
            impls,
        } = self;

        create_ref_field_if_none(&mut body.fields);
        let (wrapped_type, field_ident, field_attrs) = get_field_info(&body.fields)?;
        let ref_ty = &body.ident;
        let check_mode = check_mode.infer_validator_if_missing(ref_ty);
        let field = Field {
            attrs: field_attrs.to_owned(),
            name: field_ident
                .cloned()
                .map_or(FieldName::Unnamed, FieldName::Named),
            ty: wrapped_type.to_owned(),
        };

        let code_gen = RefCodeGen {
            doc: &[],
            common_attrs: &body.attrs,
            attrs: &syn::punctuated::Punctuated::default(),
            vis: &body.vis,
            ty: &syn::Type::Verbatim(body.ident.to_token_stream()),
            ident: body.ident.clone(),
            field,
            check_mode: &check_mode,
            owned_ty: None,
            std_lib: &std_lib,
            impls: &impls,
        }
        .tokens();

        Ok(code_gen)
    }
}

pub struct CodeGen {
    check_mode: CheckMode,
    body: syn::ItemStruct,
    field: Field,

    owned_attrs: AttrList,

    ref_doc: Vec<syn::Lit>,
    ref_attrs: AttrList,
    ref_ty: syn::Type,

    std_lib: StdLib,
    expose_inner: bool,
    impls: Impls,
}

impl CodeGen {
    pub fn generate(&self) -> proc_macro2::TokenStream {
        let owned = self.owned().tokens();
        let ref_ = self.borrowed().tokens();

        quote::quote! {
            #owned
            #ref_
        }
    }

    pub fn owned(&self) -> OwnedCodeGen<'_> {
        OwnedCodeGen {
            common_attrs: &self.body.attrs,
            check_mode: &self.check_mode,
            body: &self.body,
            field: &self.field,
            attrs: &self.owned_attrs,
            ty: &self.body.ident,
            ref_ty: &self.ref_ty,
            std_lib: &self.std_lib,
            expose_inner: self.expose_inner,
            impls: &self.impls,
        }
    }

    pub fn borrowed(&self) -> RefCodeGen<'_> {
        RefCodeGen {
            doc: &self.ref_doc,
            common_attrs: &self.body.attrs,
            check_mode: &self.check_mode,
            vis: &self.body.vis,
            field: self.field.clone(),
            attrs: &self.ref_attrs,
            ty: &self.ref_ty,
            ident: syn::Ident::new(
                &self.ref_ty.to_token_stream().to_string(),
                self.ref_ty.span(),
            ),
            owned_ty: Some(&self.body.ident),
            std_lib: &self.std_lib,
            impls: &self.impls,
        }
    }
}

fn infer_ref_type_from_owned_name(name: &syn::Ident) -> syn::Type {
    let name_str = name.to_string();
    if name_str.ends_with("Buf") || name_str.ends_with("String") {
        syn::Type::Path(syn::TypePath {
            qself: None,
            path: syn::Path::from(format_ident!("{}", name_str[..name_str.len() - 3])),
        })
    } else {
        syn::Type::Path(syn::TypePath {
            qself: None,
            path: syn::Path::from(format_ident!("{}Ref", name_str)),
        })
    }
}

fn create_field_if_none(fields: &mut syn::Fields) {
    if fields.is_empty() {
        let field = syn::Field {
            vis: syn::Visibility::Inherited,
            attrs: Vec::new(),
            colon_token: None,
            ident: None,
            ty: syn::Type::Verbatim(
                syn::Ident::new("String", proc_macro2::Span::call_site()).into_token_stream(),
            ),
            mutability: syn::FieldMutability::None,
        };

        *fields = syn::Fields::Unnamed(syn::FieldsUnnamed {
            paren_token: syn::token::Paren::default(),
            unnamed: std::iter::once(field).collect(),
        });
    }
}

fn create_ref_field_if_none(fields: &mut syn::Fields) {
    if fields.is_empty() {
        let field = syn::Field {
            vis: syn::Visibility::Inherited,
            attrs: Vec::new(),
            colon_token: None,
            ident: None,
            ty: syn::Type::Verbatim(
                syn::Ident::new("str", proc_macro2::Span::call_site()).into_token_stream(),
            ),
            mutability: syn::FieldMutability::None,
        };

        *fields = syn::Fields::Unnamed(syn::FieldsUnnamed {
            paren_token: syn::token::Paren::default(),
            unnamed: std::iter::once(field).collect(),
        });
    }
}

fn get_field_info(
    fields: &syn::Fields,
) -> Result<(&syn::Type, Option<&syn::Ident>, &[syn::Attribute]), syn::Error> {
    let mut iter = fields.iter();
    let field = iter.next().unwrap();

    if iter.next().is_some() {
        return Err(syn::Error::new_spanned(
            fields,
            "typed string can only have one field",
        ));
    }

    Ok((&field.ty, field.ident.as_ref(), &field.attrs))
}

#[derive(Clone)]
pub struct Field {
    pub attrs: Vec<syn::Attribute>,
    pub name: FieldName,
    pub ty: syn::Type,
}

impl Field {
    fn self_constructor(&self) -> SelfConstructorImpl<'_> {
        SelfConstructorImpl(self)
    }
}

#[derive(Clone)]
pub enum FieldName {
    Named(syn::Ident),
    Unnamed,
}

impl FieldName {
    fn constructor_delimiter(&self) -> proc_macro2::Delimiter {
        match self {
            FieldName::Named(_) => proc_macro2::Delimiter::Brace,
            FieldName::Unnamed => proc_macro2::Delimiter::Parenthesis,
        }
    }

    fn input_name(&self) -> proc_macro2::Ident {
        match self {
            FieldName::Named(name) => name.clone(),
            FieldName::Unnamed => proc_macro2::Ident::new("raw", proc_macro2::Span::call_site()),
        }
    }
}

impl ToTokens for FieldName {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        match self {
            Self::Named(ident) => ident.to_tokens(tokens),
            Self::Unnamed => tokens.append(proc_macro2::Literal::u8_unsuffixed(0)),
        }
    }
}

struct SelfConstructorImpl<'a>(&'a Field);

impl<'a> ToTokens for SelfConstructorImpl<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let Self(field) = self;
        tokens.append(proc_macro2::Ident::new(
            "Self",
            proc_macro2::Span::call_site(),
        ));
        tokens.append(proc_macro2::Group::new(
            field.name.constructor_delimiter(),
            field.name.input_name().into_token_stream(),
        ));
    }
}

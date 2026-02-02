use proc_macro2::Literal;
use quote::{ToTokens, TokenStreamExt};
use symbol::{parse_lit_into_string, parse_lit_into_type};
use unsynn::{IParse, ToTokenIter};

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

pub type AttrList = Vec<crate::attr_grammar::AttrArg>;

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
    ref_ty: Option<crate::grammar::Type>,
    ref_doc: Vec<Literal>,
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

impl Params {
    pub fn from_args(args: crate::attr_grammar::AttrArgs) -> Result<Self, String> {
        let mut params = Self::default();

        for delim in args.args.iter() {
            let arg = &delim.value;
            let name = arg.name();

            if name == symbol::REF {
                if let Some(lit) = arg.value() {
                    let type_str = parse_lit_into_string(symbol::REF, lit)?;
                    params.ref_ty = Some(parse_lit_into_type(symbol::REF, &type_str)?);
                } else {
                    return Err("expected ref_name = \"TypeName\"".to_string());
                }
            } else if name == symbol::VALIDATOR {
                let validator = if let Some(lit) = arg.value() {
                    let type_str = parse_lit_into_string(symbol::VALIDATOR, lit)?;
                    Some(parse_lit_into_type(symbol::VALIDATOR, &type_str)?)
                } else {
                    None
                };
                params.check_mode.try_set_validator(validator)?;
            } else if name == symbol::NORMALIZER {
                let normalizer = if let Some(lit) = arg.value() {
                    let type_str = parse_lit_into_string(symbol::NORMALIZER, lit)?;
                    Some(parse_lit_into_type(symbol::NORMALIZER, &type_str)?)
                } else {
                    None
                };
                params.check_mode.try_set_normalizer(normalizer)?;
            } else if name == symbol::REF_DOC {
                if let Some(lit) = arg.value() {
                    params.ref_doc.push(lit.clone());
                } else {
                    return Err("expected ref_doc = \"doc comment\"".to_string());
                }
            } else if name == symbol::REF_ATTR {
                // Store the raw list contents for now
                // In a real implementation, you'd parse these properly
                if let Some(_contents) = arg.list_contents() {
                    // For now, we'll skip storing ref_attrs
                    // A full implementation would parse these
                }
            } else if name == symbol::OWNED_ATTR {
                if let Some(_contents) = arg.list_contents() {
                    // For now, we'll skip storing owned_attrs
                }
            } else if name == symbol::DEBUG {
                if let Some(lit) = arg.value() {
                    params.impls.debug = parse_lit_into_string(symbol::DEBUG, lit)?
                        .parse::<DelegatingImplOption>()
                        .map_err(|e| e.to_string())?
                        .into();
                } else {
                    return Err("expected debug = \"impl|owned|omit\"".to_string());
                }
            } else if name == symbol::DISPLAY {
                if let Some(lit) = arg.value() {
                    params.impls.display = parse_lit_into_string(symbol::DISPLAY, lit)?
                        .parse::<DelegatingImplOption>()
                        .map_err(|e| e.to_string())?
                        .into();
                } else {
                    return Err("expected display = \"impl|owned|omit\"".to_string());
                }
            } else if name == symbol::ORD {
                if let Some(lit) = arg.value() {
                    params.impls.ord = parse_lit_into_string(symbol::ORD, lit)?
                        .parse::<DelegatingImplOption>()
                        .map_err(|e| e.to_string())?
                        .into();
                } else {
                    return Err("expected ord = \"impl|owned|omit\"".to_string());
                }
            } else if name == symbol::CLONE {
                if let Some(lit) = arg.value() {
                    params.impls.clone = parse_lit_into_string(symbol::CLONE, lit)?
                        .parse::<ImplOption>()
                        .map_err(|e| e.to_string())?
                        .into();
                } else {
                    return Err("expected clone = \"impl|omit\"".to_string());
                }
            } else if name == symbol::SERDE {
                if let Some(lit) = arg.value() {
                    params.impls.serde = parse_lit_into_string(symbol::SERDE, lit)?
                        .parse::<ImplOption>()
                        .map_err(|e| e.to_string())?
                        .into();
                } else {
                    params.impls.serde = ImplOption::Implement.into();
                }
            } else if name == symbol::NO_STD {
                params.std_lib = StdLib::no_std(proc_macro2::Span::call_site());
            } else if name == symbol::NO_EXPOSE {
                params.expose_inner = false;
            } else {
                return Err(format!("unsupported argument `{}`", name));
            }
        }

        Ok(params)
    }
}

impl Params {
    pub fn build(self, mut body: crate::grammar::ItemStruct) -> Result<CodeGen, String> {
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
            attrs: field_attrs.to_vec(),
            name: field_ident
                .map(|i| FieldName::Named(i.clone()))
                .unwrap_or(FieldName::Unnamed),
            ty: wrapped_type.clone(),
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

impl ParamsRef {
    pub fn from_args(args: crate::attr_grammar::AttrArgs) -> Result<Self, String> {
        let mut params = Self::default();

        for delim in args.args.iter() {
            let arg = &delim.value;
            let name = arg.name();

            if name == symbol::VALIDATOR {
                let validator = if let Some(lit) = arg.value() {
                    let type_str = parse_lit_into_string(symbol::VALIDATOR, lit)?;
                    Some(parse_lit_into_type(symbol::VALIDATOR, &type_str)?)
                } else {
                    None
                };
                params.check_mode.try_set_validator(validator)?;
            } else if name == symbol::DEBUG {
                if let Some(lit) = arg.value() {
                    params.impls.debug = parse_lit_into_string(symbol::DEBUG, lit)?
                        .parse::<ImplOption>()
                        .map_err(|e| e.to_string())
                        .map(DelegatingImplOption::from)?
                        .into();
                } else {
                    return Err("expected debug = \"impl|omit\"".to_string());
                }
            } else if name == symbol::DISPLAY {
                if let Some(lit) = arg.value() {
                    params.impls.display = parse_lit_into_string(symbol::DISPLAY, lit)?
                        .parse::<ImplOption>()
                        .map_err(|e| e.to_string())
                        .map(DelegatingImplOption::from)?
                        .into();
                } else {
                    return Err("expected display = \"impl|omit\"".to_string());
                }
            } else if name == symbol::ORD {
                if let Some(lit) = arg.value() {
                    params.impls.ord = parse_lit_into_string(symbol::ORD, lit)?
                        .parse::<ImplOption>()
                        .map_err(|e| e.to_string())
                        .map(DelegatingImplOption::from)?
                        .into();
                } else {
                    return Err("expected ord = \"impl|omit\"".to_string());
                }
            } else if name == symbol::SERDE {
                if let Some(lit) = arg.value() {
                    params.impls.serde = parse_lit_into_string(symbol::SERDE, lit)?
                        .parse::<ImplOption>()
                        .map_err(|e| e.to_string())?
                        .into();
                } else {
                    params.impls.serde = ImplOption::Implement.into();
                }
            } else if name == symbol::NO_STD {
                params.std_lib = StdLib::no_std(proc_macro2::Span::call_site());
            } else {
                return Err(format!("unsupported argument `{}`", name));
            }
        }

        Ok(params)
    }
}

impl ParamsRef {
    pub fn build(
        self,
        body: &mut crate::grammar::ItemStruct,
    ) -> Result<proc_macro2::TokenStream, String> {
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
            attrs: field_attrs.to_vec(),
            name: field_ident
                .map(|i| FieldName::Named(i.clone()))
                .unwrap_or(FieldName::Unnamed),
            ty: wrapped_type.clone(),
        };

        // Create a verbatim type from the ident
        let ty_tokens = body.ident.to_token_stream();
        let mut ty_iter = ty_tokens.to_token_iter();
        let ty = ty_iter
            .parse::<crate::grammar::Type>()
            .map_err(|e| format!("failed to parse type: {}", e))?;

        let code_gen = RefCodeGen {
            doc: &[],
            common_attrs: &body.attrs,
            attrs: &vec![],
            vis: body.vis.as_ref(),
            ty: &ty,
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
    body: crate::grammar::ItemStruct,
    field: Field,

    owned_attrs: AttrList,

    ref_doc: Vec<Literal>,
    ref_attrs: AttrList,
    ref_ty: crate::grammar::Type,

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
            vis: self.body.vis.as_ref(),
            field: self.field.clone(),
            attrs: &self.ref_attrs,
            ty: &self.ref_ty,
            ident: {
                let tokens = self.ref_ty.to_token_stream();
                let mut iter = tokens.to_token_iter();
                iter.parse::<unsynn::Ident>().unwrap_or_else(|_| {
                    unsynn::Ident::from(proc_macro2::Ident::new(
                        "UnknownType",
                        proc_macro2::Span::call_site(),
                    ))
                })
            },
            owned_ty: Some(&self.body.ident),
            std_lib: &self.std_lib,
            impls: &self.impls,
        }
    }
}

fn infer_ref_type_from_owned_name(name: &unsynn::Ident) -> crate::grammar::Type {
    let name_str = name.to_string();
    let ref_name = if name_str.ends_with("Buf") || name_str.ends_with("String") {
        &name_str[..name_str.len() - 3]
    } else {
        &format!("{}Ref", name_str)
    };

    // Parse the ref name as a type
    let tokens: proc_macro2::TokenStream = ref_name.parse().unwrap();
    let mut iter = tokens.to_token_iter();
    iter.parse::<crate::grammar::Type>()
        .expect("failed to parse ref type")
}

fn create_field_if_none(fields: &mut crate::grammar::Fields) {
    use crate::grammar::Fields;

    // If it's a unit struct, convert it to an unnamed tuple struct with String
    if matches!(fields, Fields::Unit(_)) {
        // Parse a dummy struct to extract the fields structure
        let dummy_struct: proc_macro2::TokenStream = "struct Dummy(String);".parse().unwrap();
        let mut iter = dummy_struct.to_token_iter();
        let parsed = iter
            .parse::<crate::grammar::ItemStruct>()
            .expect("failed to parse dummy struct");

        // Extract the fields from the parsed struct
        if let Fields::Unnamed(ref unnamed) = parsed.fields {
            *fields = Fields::Unnamed(unnamed.clone());
        }
    }
}

fn create_ref_field_if_none(fields: &mut crate::grammar::Fields) {
    // For unsynn, if fields is empty, we don't need to create a default field
    // The parsing should have already handled this, or we can just leave it empty
    // This function is kept for compatibility but may not be needed
    let _ = fields; // Suppress unused warning
}

fn get_field_info<'a>(
    fields: &'a crate::grammar::Fields,
) -> Result<
    (
        &'a crate::grammar::Type,
        Option<&'a unsynn::Ident>,
        &'a [crate::grammar::Attribute],
    ),
    String,
> {
    use crate::grammar::Fields;

    match fields {
        Fields::Named(f) => {
            if f.content.is_empty() {
                return Err("struct must have at least one field".to_string());
            }
            if f.content.len() > 1 {
                return Err("typed string can only have one field".to_string());
            }
            let field = &f.content[0].value;
            Ok((&field.ty, Some(&field.ident), &field.attrs))
        }
        Fields::Unnamed(f) => {
            if f.content.is_empty() {
                return Err("struct must have at least one field".to_string());
            }
            if f.content.len() > 1 {
                return Err("typed string can only have one field".to_string());
            }
            let field = &f.content[0].value;
            Ok((&field.ty, None, &field.attrs))
        }
        Fields::Unit(_) => {
            Err("unit structs are not supported - struct must have at least one field".to_string())
        }
    }
}

#[derive(Clone)]
pub struct Field {
    pub attrs: Vec<crate::grammar::Attribute>,
    pub name: FieldName,
    pub ty: crate::grammar::Type,
}

impl Field {
    fn self_constructor(&self) -> SelfConstructorImpl<'_> {
        SelfConstructorImpl(self)
    }
}

#[derive(Clone)]
pub enum FieldName {
    Named(unsynn::Ident),
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
            Self::Unnamed => tokens.append(Literal::u8_unsuffixed(0)),
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

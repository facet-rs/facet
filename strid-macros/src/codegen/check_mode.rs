use quote::ToTokens;

pub const VALIDATOR: &str = "validator";
pub const NORMALIZER: &str = "normalizer";

#[derive(Default)]
pub enum CheckMode {
    #[default]
    None,
    Validate(syn::Type),
    Normalize(syn::Type),
}

impl CheckMode {
    pub fn serde_err_handler(&self) -> Option<proc_macro2::TokenStream> {
        match self {
            Self::None => None,
            _ => Some(quote::quote! {.map_err(<D::Error as ::serde::de::Error>::custom)?}),
        }
    }
}

#[derive(Clone, Default)]
pub enum IndefiniteCheckMode {
    #[default]
    None,
    Validate(Option<syn::Type>),
    Normalize(Option<syn::Type>),
}

impl IndefiniteCheckMode {
    pub fn try_set_validator(&mut self, validator: Option<syn::Type>) -> Result<(), String> {
        if matches!(self, Self::None) {
            *self = Self::Validate(validator);
            return Ok(());
        }

        let err_desc = if matches!(self, Self::Validate(_)) {
            format!("{} can only be specified once", VALIDATOR)
        } else {
            format!(
                "only one of {} and {} can be specified at a time",
                VALIDATOR, NORMALIZER,
            )
        };

        Err(err_desc)
    }

    pub fn try_set_normalizer(&mut self, normalizer: Option<syn::Type>) -> Result<(), String> {
        if matches!(self, Self::None) {
            *self = Self::Normalize(normalizer);
            return Ok(());
        }

        let err_desc = if matches!(self, Self::Normalize(_)) {
            format!("{} can only be specified once", NORMALIZER)
        } else {
            format!(
                "only one of {} and {} can be specified at a time",
                VALIDATOR, NORMALIZER,
            )
        };

        Err(err_desc)
    }

    pub fn infer_validator_if_missing(self, default: &syn::Ident) -> CheckMode {
        match self {
            Self::None => CheckMode::None,
            Self::Validate(Some(validator)) => CheckMode::Validate(validator),
            Self::Validate(None) => CheckMode::Validate(ident_to_type(default)),
            Self::Normalize(Some(normalizer)) => CheckMode::Normalize(normalizer),
            Self::Normalize(None) => CheckMode::Normalize(ident_to_type(default)),
        }
    }
}

pub fn ident_to_type(ident: &syn::Ident) -> syn::Type {
    let tokens = ident.to_token_stream();

    syn::parse_quote!(#tokens)
}

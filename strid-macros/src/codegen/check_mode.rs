use quote::ToTokens;
use unsynn::{IParse, ToTokenIter};

pub const VALIDATOR: &str = "validator";
pub const NORMALIZER: &str = "normalizer";

#[derive(Default)]
pub enum CheckMode {
    #[default]
    None,
    Validate(crate::grammar::Type),
    Normalize(crate::grammar::Type),
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
    Validate(Option<crate::grammar::Type>),
    Normalize(Option<crate::grammar::Type>),
}

impl IndefiniteCheckMode {
    pub fn try_set_validator(
        &mut self,
        validator: Option<crate::grammar::Type>,
    ) -> Result<(), String> {
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

    pub fn try_set_normalizer(
        &mut self,
        normalizer: Option<crate::grammar::Type>,
    ) -> Result<(), String> {
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

    pub fn infer_validator_if_missing(self, default: &unsynn::Ident) -> CheckMode {
        match self {
            Self::None => CheckMode::None,
            Self::Validate(Some(validator)) => CheckMode::Validate(validator),
            Self::Validate(None) => CheckMode::Validate(ident_to_type(default)),
            Self::Normalize(Some(normalizer)) => CheckMode::Normalize(normalizer),
            Self::Normalize(None) => CheckMode::Normalize(ident_to_type(default)),
        }
    }
}

pub fn ident_to_type(ident: &unsynn::Ident) -> crate::grammar::Type {
    let tokens = ident.to_token_stream();
    let mut iter = tokens.to_token_iter();

    // Parse the identifier as a type
    iter.parse::<crate::grammar::Type>()
        .expect("failed to parse identifier as type")
}

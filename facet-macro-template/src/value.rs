//! Template values

use facet_macro_parse::{PEnum, PStruct, PStructField, PVariant};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::collections::HashMap;

/// A value in the template evaluation context
#[derive(Debug, Clone)]
pub enum Value {
    /// A token stream (type name, field type, etc.)
    Tokens(TokenStream2),
    /// A string (doc comment, attribute value, etc.)
    String(String),
    /// A boolean
    Bool(bool),
    /// An integer (field index, etc.)
    Int(usize),
    /// A list of values (variants, fields, etc.)
    List(Vec<Value>),
    /// An object with named fields
    Object(HashMap<String, Value>),
    /// A parsed struct
    Struct(Box<PStruct>),
    /// A parsed enum
    Enum(Box<PEnum>),
    /// A parsed variant
    Variant(Box<PVariant>),
    /// A parsed field
    Field(Box<PStructField>),
}

impl Value {
    /// Convert to tokens for interpolation
    pub fn to_tokens(&self) -> TokenStream2 {
        match self {
            Value::Tokens(ts) => ts.clone(),
            Value::String(s) => {
                let lit = proc_macro2::Literal::string(s);
                quote! { #lit }
            }
            Value::Bool(b) => {
                if *b {
                    quote! { true }
                } else {
                    quote! { false }
                }
            }
            Value::Int(n) => {
                let lit = proc_macro2::Literal::usize_unsuffixed(*n);
                quote! { #lit }
            }
            Value::List(_) => {
                panic!("Cannot convert list to tokens directly")
            }
            Value::Object(_) => {
                panic!("Cannot convert object to tokens directly")
            }
            Value::Struct(s) => {
                let name = s.name();
                quote! { #name }
            }
            Value::Enum(e) => {
                let name = e.name();
                quote! { #name }
            }
            Value::Variant(v) => {
                let name = v.raw_ident();
                quote! { #name }
            }
            Value::Field(f) => f.name.raw.to_token_stream(),
        }
    }

    /// Get a field from an object or structured value
    pub fn get(&self, key: &str) -> Option<Value> {
        match self {
            Value::Object(map) => map.get(key).cloned(),
            Value::Struct(s) => Self::get_struct_field(s, key),
            Value::Enum(e) => Self::get_enum_field(e, key),
            Value::Variant(v) => Self::get_variant_field(v, key),
            Value::Field(f) => Self::get_field_field(f, key),
            _ => None,
        }
    }

    fn get_struct_field(s: &PStruct, key: &str) -> Option<Value> {
        match key {
            "name" => {
                let name = s.name();
                Some(Value::Tokens(quote! { #name }))
            }
            "doc" => Some(Value::String(s.doc())),
            "fields" => {
                let fields: Vec<Value> = s
                    .kind
                    .fields()
                    .iter()
                    .cloned()
                    .map(|f| Value::Field(Box::new(f)))
                    .collect();
                Some(Value::List(fields))
            }
            "is_unit" => Some(Value::Bool(s.kind.is_unit())),
            "is_tuple" => Some(Value::Bool(s.kind.is_tuple())),
            "is_named" => Some(Value::Bool(s.kind.is_named())),
            _ => None,
        }
    }

    fn get_enum_field(e: &PEnum, key: &str) -> Option<Value> {
        match key {
            "name" => {
                let name = e.name();
                Some(Value::Tokens(quote! { #name }))
            }
            "doc" => Some(Value::String(e.doc())),
            "variants" => {
                let variants: Vec<Value> = e
                    .variants
                    .iter()
                    .cloned()
                    .map(|v| Value::Variant(Box::new(v)))
                    .collect();
                Some(Value::List(variants))
            }
            _ => None,
        }
    }

    fn get_variant_field(v: &PVariant, key: &str) -> Option<Value> {
        match key {
            "name" => {
                let ident = v.raw_ident();
                Some(Value::Tokens(quote! { #ident }))
            }
            "effective_name" => Some(Value::String(v.effective_name().to_string())),
            "doc" => Some(Value::String(v.doc())),
            "fields" => {
                let fields: Vec<Value> = v
                    .kind
                    .fields()
                    .iter()
                    .cloned()
                    .map(|f| Value::Field(Box::new(f)))
                    .collect();
                Some(Value::List(fields))
            }
            "is_unit" => Some(Value::Bool(v.kind.is_unit())),
            "is_tuple" => Some(Value::Bool(v.kind.is_tuple())),
            "is_struct" => Some(Value::Bool(v.kind.is_struct())),
            _ => None,
        }
    }

    fn get_field_field(f: &PStructField, key: &str) -> Option<Value> {
        match key {
            "name" => Some(Value::Tokens(f.raw_ident())),
            "effective_name" => Some(Value::String(f.effective_name().to_string())),
            "ty" => Some(Value::Tokens(f.ty.clone())),
            "doc" => Some(Value::String(f.doc())),
            _ => None,
        }
    }

    /// Get as a list for iteration
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(v) => Some(v),
            _ => None,
        }
    }

    /// Get as a bool for conditionals
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::String(s) => !s.is_empty(),
            Value::List(v) => !v.is_empty(),
            Value::Int(n) => *n != 0,
            _ => true,
        }
    }
}

impl From<PStruct> for Value {
    fn from(s: PStruct) -> Self {
        Value::Struct(Box::new(s))
    }
}

impl From<PEnum> for Value {
    fn from(e: PEnum) -> Self {
        Value::Enum(Box::new(e))
    }
}

impl From<PVariant> for Value {
    fn from(v: PVariant) -> Self {
        Value::Variant(Box::new(v))
    }
}

impl From<PStructField> for Value {
    fn from(f: PStructField) -> Self {
        Value::Field(Box::new(f))
    }
}

use quote::ToTokens;

impl From<TokenStream2> for Value {
    fn from(ts: TokenStream2) -> Self {
        Value::Tokens(ts)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<usize> for Value {
    fn from(n: usize) -> Self {
        Value::Int(n)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::List(v.into_iter().map(Into::into).collect())
    }
}

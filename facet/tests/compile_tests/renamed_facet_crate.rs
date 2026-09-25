//! A crate that reaches facet only under another name must be able to derive
//! `Facet` with `#[facet(crate = ...)]`, whatever attributes it uses.
//!
//! Built with `renamed_facet = { package = "facet", ... }` and no `facet`
//! dependency, so any generated `::facet::` path fails to resolve. One
//! attribute of each payload kind the grammar supports, builtin and extension.

#![allow(dead_code)]

use facet_testattrs as testattrs;
use facet_validate as validate;
use renamed_facet::Facet;

// Builtin, container level: unit, newtype_str, opt_str.
#[derive(Facet)]
#[facet(crate = ::renamed_facet, transparent)]
struct Transparent(String);

#[derive(Facet)]
#[facet(crate = ::renamed_facet, deny_unknown_fields, rename_all = "camelCase")]
struct Container {
    a: u8,
}

#[derive(Facet)]
#[facet(crate = ::renamed_facet, tag = "t", content = "c")]
#[repr(u8)]
enum Tagged {
    A(u8),
    B { x: u8 },
}

// Builtin, field level: newtype_str, predicate, make_t, shape_type, flags.
#[derive(Facet)]
#[facet(crate = ::renamed_facet)]
struct Fields {
    #[facet(rename = "b")]
    a: String,
    #[facet(skip_serializing_if = String::is_empty)]
    c: String,
    #[facet(default = 7)]
    d: u8,
    #[facet(opaque, proxy = String)]
    e: Opaque,
    #[facet(sensitive, flatten)]
    f: Container,
}

struct Opaque;

impl TryFrom<String> for Opaque {
    type Error = &'static str;
    fn try_from(_: String) -> Result<Self, Self::Error> {
        Ok(Opaque)
    }
}

impl TryFrom<&Opaque> for String {
    type Error = &'static str;
    fn try_from(_: &Opaque) -> Result<Self, Self::Error> {
        Ok(String::new())
    }
}

// Extension: newtype_i64, newtype_usize, validator, unit.
#[derive(Facet)]
#[facet(crate = ::renamed_facet)]
struct Validated {
    #[facet(validate::min = 0, validate::max = 9)]
    n: i64,
    #[facet(validate::max_length = 5, validate::email)]
    s: String,
    #[facet(validate::custom = check)]
    c: String,
}

fn check(_: &str) -> Result<(), String> {
    Ok(())
}

// Extension: newtype_opt_char, struct payload, list(shape_type).
#[derive(Facet)]
#[facet(crate = ::renamed_facet)]
struct TestAttrs {
    #[facet(testattrs::short = 'v')]
    a: bool,
    #[facet(testattrs::column(rename = "user_name", indexed))]
    b: String,
    #[facet(testattrs::lenient_width(f32, i32))]
    c: f64,
}

fn main() {}

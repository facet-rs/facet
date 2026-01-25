use facet::Facet;
use facet_error as _;

/// The struct passed into facet_args::builder has some problems: some fields are not
/// annotated, etc.
#[derive(Facet, Debug)]
#[facet(derive(Error))]
#[repr(u8)]
pub enum SchemaError {
    /// The
    BadSchema(&'static str),
}

// A kind of argument
pub enum ArgKind {
    Positional,
    Named { short: Option<char> },
}

/// A schema "parsed" from
pub struct Schema {}

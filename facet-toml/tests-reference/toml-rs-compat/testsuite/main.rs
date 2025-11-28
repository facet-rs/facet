#![recursion_limit = "256"]
#![cfg(all(feature = "parse", feature = "display", feature = "serde"))]

macro_rules! map( ($($k:expr => $v:expr),*) => ({
    let mut _m = Map::new();
    $(_m.insert($k.to_owned(), $v);)*
    _m
}) );

mod macros;
mod table;
mod value;

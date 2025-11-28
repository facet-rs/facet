//! Allows reading from shapes

mod value;
pub use value::*;

mod struct_;
pub use struct_::*;

mod enum_;
pub use enum_::*;

mod fields;
pub use fields::*;

mod list;
pub use list::*;

mod list_like;
pub use list_like::*;

mod ndarray;
pub use ndarray::*;

mod map;
pub use map::*;

mod set;
pub use set::*;

mod option;
pub use option::*;

mod pointer;
pub use pointer::*;

mod tuple;
pub use tuple::*;

mod dynamic_value;
pub use dynamic_value::*;

#[cfg(feature = "alloc")]
mod owned;
#[cfg(feature = "alloc")]
pub use owned::*;

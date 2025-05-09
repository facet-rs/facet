//! Allows reading from shapes

mod value;
pub use value::*;

mod struct_;
pub use struct_::*;

mod enum_;
pub use enum_::*;

mod list;
pub use list::*;

mod list_like;
pub use list_like::*;

mod map;
pub use map::*;

mod option;
pub use option::*;

mod smartptr;
pub use smartptr::*;

mod tuple;
pub use tuple::*;

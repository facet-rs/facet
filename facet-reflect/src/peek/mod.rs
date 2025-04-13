//! Allows reading from shapes

#[cfg(feature = "alloc")]
extern crate alloc;

mod value;
pub use value::*;

mod struct_;
pub use struct_::*;

mod enum_;
pub use enum_::*;

mod list;
pub use list::*;

mod map;
pub use map::*;

mod option;
pub use option::*;

mod smartptr;
pub use smartptr::*;

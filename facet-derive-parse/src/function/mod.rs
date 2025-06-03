/// Parsing the `fn_shape!`` macro input
pub mod fn_shape_input;
/// Parsing the function body
pub mod func_body;
/// Parsing function parameters
pub mod func_params;
/// Parsing the function signature
pub mod func_sig;
/// Parsing generic functions
pub mod generics;
/// Parsing the return type
pub mod ret_type;
/// Parsing type parameters
pub mod type_params;

pub use fn_shape_input::*;
pub use func_body::*;
pub use func_params::*;
pub use func_sig::*;
pub use generics::*;
pub use ret_type::*;
pub use type_params::*;

//! A macro for generating builder patterns for structs.
//!
//! This provides a way to generate the repetitive parts of a builder
//! while allowing custom methods and constructors to be added manually.

/// Generates a builder struct and basic setter methods.
///
/// # Example
///
/// ```ignore
/// builder_def! {
///     /// Documentation for the target struct
///     #[derive(Clone, Copy, Debug)]
///     pub struct Field + FieldBuilder {
///         /// Required field - panics if not set
///         pub name: &'static str,
///         /// Optional field - defaults to None
///         pub rename: Option<&'static str>,
///         /// Field with default value
///         pub flags: FieldFlags = FieldFlags::empty(),
///     }
/// }
///
/// // Then add custom constructors/methods manually:
/// impl FieldBuilder {
///     pub const fn new(name: &'static str, shape: &'static Shape, offset: usize) -> Self {
///         Self::default()
///             .name(name)
///             .shape(ShapeRef::Static(shape))
///             .offset(offset)
///     }
/// }
/// ```
///
/// This generates:
/// - The struct as written (with defaults applied)
/// - `TargetBuilder` struct with `Option<T>` for each field
/// - `TargetBuilder::default()` (all fields None, except those with defaults)
/// - Setter methods for each field
/// - `TargetBuilder::build() -> Target` (panics if required fields not set)
///
/// Field types:
/// - `field: T` - Required, panics if not set
/// - `field: Option<T>` - Optional, defaults to None
/// - `field: T = expr` - Has default value, uses expr if not set
#[macro_export]
macro_rules! builder_def {
    (
        $(#[$struct_meta:meta])*
        $vis:vis struct $name:ident + $builder:ident {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field:ident : $field_ty:ty $(= $default:expr)?
            ),* $(,)?
        }
    ) => {
        $(#[$struct_meta])*
        $vis struct $name {
            $(
                $(#[$field_meta])*
                $field_vis $field: $field_ty,
            )*
        }

        /// Builder for the struct
        #[derive(Clone, Copy, Debug)]
        $vis struct $builder {
            $(
                $field: Option<$field_ty>,
            )*
        }

        impl $builder {
            /// Creates a new builder with all fields set to None (or their defaults).
            pub const fn default() -> Self {
                Self {
                    $(
                        $field: $crate::builder_def!(@default $($default)?),
                    )*
                }
            }

            $(
                #[doc = concat!("Set the `", stringify!($field), "` field")]
                pub const fn $field(mut self, value: $field_ty) -> Self {
                    self.$field = Some(value);
                    self
                }
            )*

            /// Build the struct
            ///
            /// # Panics
            ///
            /// Panics if any required field was not set.
            pub const fn build(self) -> $name {
                $name {
                    $(
                        $field: $crate::builder_def!(@unwrap self.$field, stringify!($name), stringify!($field), $field_ty, $($default)?),
                    )*
                }
            }
        }
    };

    // Default value helper - if default provided, use Some(default), else None
    (@default $default:expr) => { Some($default) };
    (@default) => { None };

    // Unwrap helper - handles required fields, Option<T> fields, and fields with defaults
    // Field with default value - use default if None
    (@unwrap $value:expr, $struct:expr, $field:expr, $ty:ty, $default:expr) => {
        match $value {
            Some(v) => v,
            None => $default,
        }
    };
    // Optional field (Option<T>) - pass through as-is
    (@unwrap $value:expr, $struct:expr, $field:expr, Option<$inner:ty>,) => {
        $value
    };
    // Required field - panic if None
    (@unwrap $value:expr, $struct:expr, $field:expr, $ty:ty,) => {
        match $value {
            Some(v) => v,
            None => panic!(concat!($struct, "Builder::", $field, "() must be called")),
        }
    };
}

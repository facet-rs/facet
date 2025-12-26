#![warn(missing_docs)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

//! Pretty assertions for Facet types.
//!
//! Unlike `assert_eq!` which requires `PartialEq`, `assert_same!` works with any
//! `Facet` type by doing structural comparison via reflection.

mod same;

pub use facet_diff::DiffReport;
pub use facet_diff_core::layout::{
    AnsiBackend, BuildOptions, ColorBackend, DiffFlavor, JsonFlavor, PlainBackend, RenderOptions,
    RustFlavor, XmlFlavor,
};
pub use same::{
    SameOptions, SameReport, Sameness, check_same, check_same_report, check_same_with,
    check_same_with_report, check_sameish, check_sameish_report, check_sameish_with,
    check_sameish_with_report,
};

// =============================================================================
// assert_same! - Same-type comparison (the common case)
// =============================================================================

/// Asserts that two values are structurally the same.
///
/// This macro does not require `PartialEq` - it uses Facet reflection to
/// compare values structurally. Both values must have the same type, which
/// enables type inference to flow between arguments.
///
/// For comparing values of different types (e.g., during migrations), use
/// [`assert_sameish!`] instead.
///
/// # Panics
///
/// Panics if the values are not structurally same, displaying a colored diff
/// showing exactly what differs.
///
/// Also panics if either value contains an opaque type that cannot be inspected.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_assert::assert_same;
///
/// #[derive(Facet)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let a = Person { name: "Alice".into(), age: 30 };
/// let b = Person { name: "Alice".into(), age: 30 };
/// assert_same!(a, b);
/// ```
///
/// Type inference works naturally:
/// ```
/// use facet_assert::assert_same;
///
/// let x: Option<Option<i32>> = Some(None);
/// assert_same!(x, Some(None)); // Type of Some(None) inferred from x
/// ```
#[macro_export]
macro_rules! assert_same {
    ($left:expr, $right:expr $(,)?) => {
        match $crate::check_same(&$left, &$right) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_same!(left, right)` failed\n\n{diff}\n"
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_same!(left, right)` failed: cannot compare opaque type `{type_name}`"
                );
            }
        }
    };
    ($left:expr, $right:expr, $($arg:tt)+) => {
        match $crate::check_same(&$left, &$right) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_same!(left, right)` failed: {}\n\n{diff}\n",
                    format_args!($($arg)+)
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_same!(left, right)` failed: {}: cannot compare opaque type `{type_name}`",
                    format_args!($($arg)+)
                );
            }
        }
    };
}

/// Asserts that two values are structurally the same with custom options.
///
/// Like [`assert_same!`], but allows configuring comparison behavior via [`SameOptions`].
///
/// # Panics
///
/// Panics if the values are not structurally same, displaying a colored diff.
///
/// # Example
///
/// ```
/// use facet_assert::{assert_same_with, SameOptions};
///
/// let a = 1.0000001_f64;
/// let b = 1.0000002_f64;
///
/// // This would fail with exact comparison:
/// // assert_same!(a, b);
///
/// // But passes with tolerance:
/// assert_same_with!(a, b, SameOptions::new().float_tolerance(1e-6));
/// ```
#[macro_export]
macro_rules! assert_same_with {
    ($left:expr, $right:expr, $options:expr $(,)?) => {
        match $crate::check_same_with(&$left, &$right, $options) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_same_with!(left, right, options)` failed\n\n{diff}\n"
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_same_with!(left, right, options)` failed: cannot compare opaque type `{type_name}`"
                );
            }
        }
    };
    ($left:expr, $right:expr, $options:expr, $($arg:tt)+) => {
        match $crate::check_same_with(&$left, &$right, $options) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_same_with!(left, right, options)` failed: {}\n\n{diff}\n",
                    format_args!($($arg)+)
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_same_with!(left, right, options)` failed: {}: cannot compare opaque type `{type_name}`",
                    format_args!($($arg)+)
                );
            }
        }
    };
}

/// Asserts that two values are structurally the same (debug builds only).
///
/// Like [`assert_same!`], but only enabled in debug builds.
#[macro_export]
macro_rules! debug_assert_same {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            $crate::assert_same!($($arg)*);
        }
    };
}

/// Asserts that two values are structurally the same with custom options (debug builds only).
///
/// Like [`assert_same_with!`], but only enabled in debug builds.
#[macro_export]
macro_rules! debug_assert_same_with {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            $crate::assert_same_with!($($arg)*);
        }
    };
}

// =============================================================================
// assert_sameish! - Cross-type comparison (for migrations, etc.)
// =============================================================================

/// Asserts that two values of potentially different types are structurally the same.
///
/// Unlike [`assert_same!`], this allows comparing values of different types.
/// Two values are "sameish" if they have the same structure and values,
/// even if they have different type names.
///
/// **Note:** Because the two arguments can have different types, the compiler
/// cannot infer types from one side to the other. If you get type inference
/// errors, either add type annotations or use [`assert_same!`] instead.
///
/// # Panics
///
/// Panics if the values are not structurally same, displaying a colored diff.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use facet_assert::assert_sameish;
///
/// #[derive(Facet)]
/// struct PersonV1 {
///     name: String,
///     age: u32,
/// }
///
/// #[derive(Facet)]
/// struct PersonV2 {
///     name: String,
///     age: u32,
/// }
///
/// let old = PersonV1 { name: "Alice".into(), age: 30 };
/// let new = PersonV2 { name: "Alice".into(), age: 30 };
/// assert_sameish!(old, new); // Different types, same structure
/// ```
#[macro_export]
macro_rules! assert_sameish {
    ($left:expr, $right:expr $(,)?) => {
        match $crate::check_sameish(&$left, &$right) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_sameish!(left, right)` failed\n\n{diff}\n"
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_sameish!(left, right)` failed: cannot compare opaque type `{type_name}`"
                );
            }
        }
    };
    ($left:expr, $right:expr, $($arg:tt)+) => {
        match $crate::check_sameish(&$left, &$right) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_sameish!(left, right)` failed: {}\n\n{diff}\n",
                    format_args!($($arg)+)
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_sameish!(left, right)` failed: {}: cannot compare opaque type `{type_name}`",
                    format_args!($($arg)+)
                );
            }
        }
    };
}

/// Asserts that two values of different types are structurally the same with custom options.
///
/// Like [`assert_sameish!`], but allows configuring comparison behavior via [`SameOptions`].
#[macro_export]
macro_rules! assert_sameish_with {
    ($left:expr, $right:expr, $options:expr $(,)?) => {
        match $crate::check_sameish_with(&$left, &$right, $options) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_sameish_with!(left, right, options)` failed\n\n{diff}\n"
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_sameish_with!(left, right, options)` failed: cannot compare opaque type `{type_name}`"
                );
            }
        }
    };
    ($left:expr, $right:expr, $options:expr, $($arg:tt)+) => {
        match $crate::check_sameish_with(&$left, &$right, $options) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_sameish_with!(left, right, options)` failed: {}\n\n{diff}\n",
                    format_args!($($arg)+)
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_sameish_with!(left, right, options)` failed: {}: cannot compare opaque type `{type_name}`",
                    format_args!($($arg)+)
                );
            }
        }
    };
}

/// Asserts that two values of different types are structurally the same (debug builds only).
///
/// Like [`assert_sameish!`], but only enabled in debug builds.
#[macro_export]
macro_rules! debug_assert_sameish {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            $crate::assert_sameish!($($arg)*);
        }
    };
}

/// Asserts that two values of different types are structurally the same with options (debug builds only).
///
/// Like [`assert_sameish_with!`], but only enabled in debug builds.
#[macro_export]
macro_rules! debug_assert_sameish_with {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            $crate::assert_sameish_with!($($arg)*);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[derive(Facet)]
    struct Person {
        name: String,
        age: u32,
    }

    #[derive(Facet)]
    struct PersonV2 {
        name: String,
        age: u32,
    }

    #[derive(Facet)]
    struct Different {
        name: String,
        score: f64,
    }

    #[test]
    fn same_type_same_values() {
        let a = Person {
            name: "Alice".into(),
            age: 30,
        };
        let b = Person {
            name: "Alice".into(),
            age: 30,
        };
        assert_same!(a, b);
    }

    #[test]
    fn different_types_same_structure() {
        let a = Person {
            name: "Alice".into(),
            age: 30,
        };
        let b = PersonV2 {
            name: "Alice".into(),
            age: 30,
        };
        // Use assert_sameish! for cross-type comparison
        assert_sameish!(a, b);
    }

    #[test]
    fn same_type_different_values() {
        let a = Person {
            name: "Alice".into(),
            age: 30,
        };
        let b = Person {
            name: "Bob".into(),
            age: 30,
        };

        match check_same(&a, &b) {
            Sameness::Different(_) => {} // expected
            other => panic!(
                "expected Different, got {:?}",
                matches!(other, Sameness::Same)
            ),
        }
    }

    #[test]
    fn diff_report_renders_multiple_flavors() {
        let a = Person {
            name: "Alice".into(),
            age: 30,
        };
        let b = Person {
            name: "Bob".into(),
            age: 45,
        };

        let report = match check_same_report(&a, &b) {
            SameReport::Different(report) => report,
            _ => panic!("expected Different"),
        };

        let rust = report.render_plain_rust();
        assert!(rust.contains("Person"));

        let json = report.render_plain_json();
        assert!(json.contains("\"name\""));

        let xml = report.render_plain_xml();
        // Person is a proxy type (struct without xml:: attributes), so it gets @ prefix
        assert!(xml.contains("<@Person"));
    }

    #[test]
    fn primitives() {
        assert_same!(42i32, 42i32);
        assert_same!("hello", "hello");
        assert_same!(true, true);
    }

    #[test]
    fn vectors() {
        let a = vec![1, 2, 3];
        let b = vec![1, 2, 3];
        assert_same!(a, b);
    }

    #[test]
    fn vectors_different() {
        let a = vec![1, 2, 3];
        let b = vec![1, 2, 4];

        match check_same(&a, &b) {
            Sameness::Different(_) => {} // expected
            _ => panic!("expected Different"),
        }
    }

    #[test]
    fn options() {
        let a: Option<i32> = Some(42);
        let b: Option<i32> = Some(42);
        assert_same!(a, b);

        let c: Option<i32> = None;
        let d: Option<i32> = None;
        assert_same!(c, d);
    }

    #[test]
    fn float_exact_same() {
        let a = 1.0_f64;
        let b = 1.0_f64;
        assert_same!(a, b);
    }

    #[test]
    fn float_exact_different() {
        let a = 1.0000001_f64;
        let b = 1.0000002_f64;

        match check_same(&a, &b) {
            Sameness::Different(_) => {} // expected - exact comparison fails
            _ => panic!("expected Different"),
        }
    }

    #[test]
    fn float_with_tolerance_same() {
        let a = 1.0000001_f64;
        let b = 1.0000002_f64;

        // With tolerance, these should be considered the same
        assert_same_with!(a, b, SameOptions::new().float_tolerance(1e-6));
    }

    #[test]
    fn float_with_tolerance_different() {
        let a = 1.0_f64;
        let b = 2.0_f64;

        // Even with tolerance, 1.0 vs 2.0 are different
        match check_same_with(&a, &b, SameOptions::new().float_tolerance(1e-6)) {
            Sameness::Different(_) => {} // expected
            _ => panic!("expected Different"),
        }
    }

    #[test]
    fn f32_with_tolerance() {
        let a = 1.0000001_f32;
        let b = 1.0000002_f32;

        // With tolerance, these should be considered the same
        assert_same_with!(a, b, SameOptions::new().float_tolerance(1e-5));
    }

    #[test]
    fn struct_with_float_tolerance() {
        #[derive(Facet)]
        struct Measurement {
            name: String,
            value: f64,
        }

        let a = Measurement {
            name: "temperature".into(),
            value: 98.6000001,
        };
        let b = Measurement {
            name: "temperature".into(),
            value: 98.6000002,
        };

        // Exact comparison fails
        match check_same(&a, &b) {
            Sameness::Different(_) => {} // expected
            _ => panic!("expected Different"),
        }

        // With tolerance, they're the same
        assert_same_with!(a, b, SameOptions::new().float_tolerance(1e-6));
    }

    #[test]
    fn vec_of_floats_with_tolerance() {
        let a = vec![1.0000001_f64, 2.0000001_f64, 3.0000001_f64];
        let b = vec![1.0000002_f64, 2.0000002_f64, 3.0000002_f64];

        // Exact comparison fails
        match check_same(&a, &b) {
            Sameness::Different(_) => {} // expected
            _ => panic!("expected Different"),
        }

        // With tolerance, they're the same
        assert_same_with!(a, b, SameOptions::new().float_tolerance(1e-6));
    }

    #[test]
    fn nested_struct_with_float_tolerance() {
        #[derive(Facet)]
        struct Point {
            x: f64,
            y: f64,
        }

        #[derive(Facet)]
        struct Line {
            start: Point,
            end: Point,
        }

        let a = Line {
            start: Point {
                x: 0.0000001,
                y: 0.0000001,
            },
            end: Point {
                x: 1.0000001,
                y: 1.0000001,
            },
        };
        let b = Line {
            start: Point {
                x: 0.0000002,
                y: 0.0000002,
            },
            end: Point {
                x: 1.0000002,
                y: 1.0000002,
            },
        };

        assert_same_with!(a, b, SameOptions::new().float_tolerance(1e-6));
    }

    // Tests for type inference (the key fix from issue #1161)
    mod type_inference {
        use super::*;

        #[test]
        fn with_option() {
            // This is the key test from issue #1161
            // Type inference flows from x to the right-hand side
            let x: Option<Option<i32>> = Some(None);
            assert_same!(x, Some(None)); // Works! Type flows from x
        }

        #[test]
        fn with_nested_option() {
            let x: Option<Option<Option<i32>>> = Some(Some(None));
            assert_same!(x, Some(Some(None))); // Triple nesting works too
        }

        #[test]
        fn with_result() {
            let x: Result<Option<i32>, ()> = Ok(None);
            assert_same!(x, Ok(None)); // Works with Result too
        }

        #[test]
        fn with_vec() {
            let x: Vec<Option<i32>> = vec![Some(1), None, Some(3)];
            assert_same!(x, vec![Some(1), None, Some(3)]);
        }
    }

    // Tests for sameish (cross-type comparison)
    mod sameish {
        use super::*;

        #[test]
        fn different_types_same_structure() {
            let a = Person {
                name: "Alice".into(),
                age: 30,
            };
            let b = PersonV2 {
                name: "Alice".into(),
                age: 30,
            };
            assert_sameish!(a, b);
        }

        #[test]
        fn check_sameish_detects_differences() {
            let a = Person {
                name: "Alice".into(),
                age: 30,
            };
            let b = PersonV2 {
                name: "Bob".into(),
                age: 30,
            };

            match check_sameish(&a, &b) {
                Sameness::Different(_) => {} // expected
                _ => panic!("expected Different"),
            }
        }

        #[test]
        fn with_options_float_tolerance() {
            #[derive(Facet)]
            struct MeasurementV1 {
                value: f64,
            }

            #[derive(Facet)]
            struct MeasurementV2 {
                value: f64,
            }

            let a = MeasurementV1 { value: 1.0000001 };
            let b = MeasurementV2 { value: 1.0000002 };

            assert_sameish_with!(a, b, SameOptions::new().float_tolerance(1e-6));
        }

        #[test]
        fn with_custom_message() {
            let a = Person {
                name: "Alice".into(),
                age: 30,
            };
            let b = PersonV2 {
                name: "Alice".into(),
                age: 30,
            };
            assert_sameish!(a, b, "custom message: {} vs {}", "Person", "PersonV2");
        }
    }
}

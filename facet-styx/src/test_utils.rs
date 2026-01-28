//! Test utilities for snapshot testing with ANSI-stripped output.

/// Assert a snapshot with ANSI escape codes stripped.
///
/// This macro wraps `insta::assert_snapshot!` but strips ANSI escape codes
/// from the input first, making snapshots readable and diff-friendly.
#[macro_export]
macro_rules! assert_snapshot_stripped {
    ($value:expr) => {{
        let stripped = String::from_utf8(strip_ansi_escapes::strip(&$value)).unwrap();
        insta::assert_snapshot!(stripped);
    }};
    ($name:expr, $value:expr) => {{
        let stripped = String::from_utf8(strip_ansi_escapes::strip(&$value)).unwrap();
        insta::assert_snapshot!($name, stripped);
    }};
}

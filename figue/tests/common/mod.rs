pub fn strip_ansi(text: &str) -> String {
    strip_ansi_escapes::strip_str(text).replace('\\', "/")
}

/// Do snapshot testing for a diagnostic error
#[macro_export]
macro_rules! assert_diag_snapshot {
    ($err:expr) => {
        insta::assert_snapshot!($crate::common::strip_ansi(&$err.to_string()))
    };
}

/// Do snapshot testing for help text (strips ANSI codes)
#[macro_export]
macro_rules! assert_help_snapshot {
    ($help:expr) => {
        insta::assert_snapshot!($crate::common::strip_ansi(&$help))
    };
    ($name:expr, $help:expr) => {
        insta::assert_snapshot!($name, $crate::common::strip_ansi(&$help))
    };
}

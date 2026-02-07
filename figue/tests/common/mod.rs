/// Do snapshot testing for a diagnostic error
#[macro_export]
macro_rules! assert_diag_snapshot {
    ($err:expr) => {
        insta::assert_snapshot!(strip_ansi_escapes::strip_str(&$err.to_string()))
    };
}

/// Do snapshot testing for help text (strips ANSI codes)
#[macro_export]
macro_rules! assert_help_snapshot {
    ($help:expr) => {
        insta::assert_snapshot!(strip_ansi_escapes::strip_str(&$help))
    };
    ($name:expr, $help:expr) => {
        insta::assert_snapshot!($name, strip_ansi_escapes::strip_str(&$help))
    };
}

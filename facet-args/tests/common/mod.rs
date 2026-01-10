/// Do snapshot testing for a diagnostic error
#[macro_export]
macro_rules! assert_diag_snapshot {
    ($err:expr) => {
        insta::assert_snapshot!($err.to_string())
    };
}

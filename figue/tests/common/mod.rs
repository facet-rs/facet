/// Strip ANSI escape codes from a string for snapshot testing
pub fn strip_ansi(s: &str) -> String {
    // Simple regex-free ANSI stripping: remove ESC [ ... m sequences
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip ESC
            if chars.peek() == Some(&'[') {
                chars.next(); // Skip '['
                // Skip until 'm'
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == 'm' {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Strip binary paths from help text for reproducible snapshots.
/// Replaces paths like `/path/to/target/debug/deps/main-abc123` with `<PROGRAM>`.
pub fn strip_binary_paths(s: &str) -> String {
    let mut result = String::new();
    for line in s.lines() {
        let processed = if line.contains("/target/") && line.contains("/deps/") {
            // This line contains a binary path - replace it
            if let Some(start) = line.find('/') {
                let prefix = &line[..start];
                format!("{}<PROGRAM>", prefix)
            } else {
                "<PROGRAM>".to_string()
            }
        } else {
            line.to_string()
        };
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&processed);
    }
    result
}

/// Do snapshot testing for a diagnostic error
#[macro_export]
macro_rules! assert_diag_snapshot {
    ($err:expr) => {
        insta::assert_snapshot!($crate::common::strip_ansi(&$err.to_string()))
    };
}

/// Do snapshot testing for help text (strips ANSI codes and binary paths)
#[macro_export]
macro_rules! assert_help_snapshot {
    ($help:expr) => {
        insta::assert_snapshot!($crate::common::strip_binary_paths(
            &$crate::common::strip_ansi(&$help)
        ))
    };
    ($name:expr, $help:expr) => {
        insta::assert_snapshot!(
            $name,
            $crate::common::strip_binary_paths(&$crate::common::strip_ansi(&$help))
        )
    };
}

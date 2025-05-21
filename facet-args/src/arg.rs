use alloc::borrow::Cow;
use facet_deserialize::{Subspan, SubspanMeta};

#[derive(Debug)]
pub(crate) enum ArgType<'a> {
    LongFlag(Cow<'a, str>),
    ShortFlag(&'a str),
    Positional,
    None,
}

impl<'a> ArgType<'a> {
    pub(crate) fn parse(arg: &'a str) -> Self {
        if let Some(key) = arg.strip_prefix("--") {
            ArgType::LongFlag(Self::kebab_to_snake(key))
        } else if let Some(key) = arg.strip_prefix('-') {
            ArgType::ShortFlag(key)
        } else if !arg.is_empty() {
            ArgType::Positional
        } else {
            ArgType::None
        }
    }

    pub(crate) fn kebab_to_snake(input: &str) -> Cow<str> {
        if !input.contains('-') {
            return Cow::Borrowed(input);
        }
        Cow::Owned(input.replace('-', "_"))
    }
}

// This trait implementation allows for using a Subspan together with an arg string
impl<'a> From<(&'a Subspan, &'a str)> for ArgType<'a> {
    /// Converts a subspan and argument string into the appropriate ArgType.
    ///
    /// - For KeyValue metadata: key part (offset 0) is parsed normally (ShortFlag or LongFlag),
    ///   value part (offset > 0) is treated as Positional
    /// - For Delimiter metadata: treated as Positional
    /// - For no metadata: parsed normally
    fn from((subspan, arg): (&'a Subspan, &'a str)) -> Self {
        if subspan.offset >= arg.len() {
            return ArgType::None;
        }

        let end = core::cmp::min(subspan.offset + subspan.len, arg.len());
        let part = &arg[subspan.offset..end];

        // Check metadata for special handling
        if let Some(meta) = &subspan.meta {
            match meta {
                SubspanMeta::KeyValue => {
                    // For KeyValue, if it's the value part (offset > 0),
                    // treat it as a positional argument regardless of content
                    if subspan.offset > 0 {
                        return if !part.is_empty() {
                            ArgType::Positional
                        } else {
                            ArgType::None
                        };
                    }
                    // Otherwise parse key part normally with parse()
                }
                SubspanMeta::Delimiter(_) => {
                    // For delimited values, treat as positional
                    return if !part.is_empty() {
                        ArgType::Positional
                    } else {
                        ArgType::None
                    };
                }
            }
        }

        // Default parsing for keys and non-special cases
        ArgType::parse(part)
    }
}

/// Extracts a substring from arg based on a subspan
pub(crate) fn extract_subspan<'a>(subspan: &Subspan, arg: &'a str) -> &'a str {
    if subspan.offset >= arg.len() {
        return "";
    }
    let end = core::cmp::min(subspan.offset + subspan.len, arg.len());
    &arg[subspan.offset..end]
}

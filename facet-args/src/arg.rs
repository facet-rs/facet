#[derive(Debug)]
pub(crate) enum ArgType<'a> {
    // A `--` separator after which we only expect positional arguments
    DoubleDash,

    // `--{arg}`
    LongFlag(&'a str),

    // `-{a}`
    ShortFlag(&'a str),

    // `{a}`
    Positional,

    // End of the argument list
    None,
}

impl<'a> ArgType<'a> {
    pub(crate) fn parse(arg: &'a str) -> Self {
        if let Some(key) = arg.strip_prefix("--") {
            if key.is_empty() {
                ArgType::DoubleDash
            } else {
                ArgType::LongFlag(key)
            }
        } else if let Some(key) = arg.strip_prefix('-') {
            if key.is_empty() {
                // Bare `-` is treated as positional (commonly means stdin)
                ArgType::Positional
            } else {
                ArgType::ShortFlag(key)
            }
        } else if !arg.is_empty() {
            ArgType::Positional
        } else {
            ArgType::None
        }
    }
}

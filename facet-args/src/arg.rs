use alloc::borrow::Cow;

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

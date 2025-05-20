use alloc::borrow::Cow;
use facet_deserialize::{Outcome, Raw, Scalar, Span, Spanned};

pub(crate) fn parse_scalar<'a>(arg: &'a str, span: Span<Raw>) -> Spanned<Outcome<'a>, Raw> {
    // Try to parse numbers in order of specificity
    if let Ok(v) = arg.parse::<u64>() {
        return Spanned {
            node: Outcome::Scalar(Scalar::U64(v)),
            span,
        };
    }
    if let Ok(v) = arg.parse::<i64>() {
        return Spanned {
            node: Outcome::Scalar(Scalar::I64(v)),
            span,
        };
    }
    if let Ok(v) = arg.parse::<f64>() {
        return Spanned {
            node: Outcome::Scalar(Scalar::F64(v)),
            span,
        };
    }

    // Default to string
    Spanned {
        node: Outcome::Scalar(Scalar::String(Cow::Borrowed(arg))),
        span,
    }
}

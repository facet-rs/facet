//! Scalar interpretation — the `interp[…]` rules of the Styx specification.
//!
//! Styx scalars are opaque text at parse time; the target type decides how a
//! scalar is read (`interp[interp.coerce.none]`). This module is the one place
//! that decision is spelled out for numbers, so the parser's hint dispatch
//! stays a table and the rules stay testable on their own.
//!
//! Before this existed the parser called `str::parse` directly, which is a
//! *different language* from the one the spec describes in both directions:
//! it rejects `1_000_000` and `0xff5500` (spec-valid, `interp[interp.int.*]`)
//! and accepts `Infinity`/`NaN`/`1e5_` (spec-invalid, the special forms are a
//! closed case-sensitive set per `interp[interp.float.special]`).
//!
//! Failure is `None` here and becomes a deferred error at the call site: an
//! uninterpretable scalar stays a string and the deserializer reports it with
//! the target type in hand, which is the context `interp[interp.error.context]`
//! asks for.
//!
//! The functions are public because the rules belong to the FORMAT, not to
//! this deserializer. A reader that drives `StyxParser` directly — a
//! type-directed decoder has no deserializer to hint it — must interpret
//! scalars identically or the two disagree about what a document says, and it
//! can only do that by calling these rather than restating them.

/// Strip `interp[interp.int.decimal]` readability underscores.
///
/// An underscore is only admissible BETWEEN digits, so `1_000` reads and
/// `_1`, `1_`, and `0x_ff` do not. `digits` is the alphabet that counts as a
/// digit for this radix, which is what makes the rule reusable for the
/// `interp[interp.int.hex]`/`octal`/`binary` bodies.
fn strip_underscores(text: &str, digits: fn(char) -> bool) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut previous_was_digit = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '_' {
            // Between digits: something admissible before, something
            // admissible after.
            if !previous_was_digit || !chars.peek().copied().is_some_and(digits) {
                return None;
            }
            continue;
        }
        if !digits(ch) {
            return None;
        }
        out.push(ch);
        previous_was_digit = true;
    }
    if out.is_empty() { None } else { Some(out) }
}

/// The radix prefix of an integer body, if any: `interp[interp.int.hex]`,
/// `interp[interp.int.octal]`, `interp[interp.int.binary]`.
fn split_radix(text: &str) -> (u32, &str, fn(char) -> bool) {
    let is_dec: fn(char) -> bool = |ch| ch.is_ascii_digit();
    let is_hex: fn(char) -> bool = |ch| ch.is_ascii_hexdigit();
    let is_oct: fn(char) -> bool = |ch| ch.is_digit(8);
    let is_bin: fn(char) -> bool = |ch| ch == '0' || ch == '1';
    match text.as_bytes() {
        [b'0', b'x' | b'X', ..] => (16, &text[2..], is_hex),
        [b'0', b'o' | b'O', ..] => (8, &text[2..], is_oct),
        [b'0', b'b' | b'B', ..] => (2, &text[2..], is_bin),
        _ => (10, text, is_dec),
    }
}

/// Split a leading sign, per `interp[interp.int.decimal]`.
///
/// The radix-prefixed forms are specified as *starting with* `0x`/`0o`/`0b`,
/// so a sign is only admissible on a decimal integer — the same rule TOML
/// states outright ("non-negative integer values may also be expressed in
/// hexadecimal, octal, or binary"). `-0xff` is therefore not an integer, and
/// falls through to the deferred type error rather than being invented here.
fn split_sign(text: &str) -> (bool, &str) {
    match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    }
}

/// Interpret a scalar as a signed integer. `None` when the text is not an
/// integer at all, and also when it is one that overflows `i64`
/// (`interp[interp.int.range]` — the narrower target types are range-checked
/// downstream, where the target is known).
#[must_use]
pub fn parse_i64(text: &str) -> Option<i64> {
    let (negative, body) = split_sign(text);
    let (radix, body, digits) = split_radix(body);
    if radix != 10 && text.starts_with(['-', '+']) {
        return None;
    }
    let body = strip_underscores(body, digits)?;
    // Parse through the sign so `-9223372036854775808` is representable.
    let signed = if negative { format!("-{body}") } else { body };
    i64::from_str_radix(&signed, radix).ok()
}

/// Interpret a scalar as an unsigned integer. A `-` sign is admissible
/// syntax that simply cannot be represented, so it fails here and is reported
/// against the target type.
#[must_use]
pub fn parse_u64(text: &str) -> Option<u64> {
    let (negative, body) = split_sign(text);
    if negative {
        return None;
    }
    let (radix, body, digits) = split_radix(body);
    if radix != 10 && text.starts_with('+') {
        return None;
    }
    let body = strip_underscores(body, digits)?;
    u64::from_str_radix(&body, radix).ok()
}

/// Interpret a scalar as a floating-point number, per
/// `interp[interp.float.syntax]` and `interp[interp.float.special]`.
///
/// The special forms are a CLOSED, case-sensitive set — `inf`, `+inf`,
/// `-inf`, `nan` — which is the rule Rust's own `f64` parser does not
/// implement (it also takes `Infinity`, `NaN`, `INF`). Accepting those would
/// make a document that only this implementation can read.
///
/// Integer-shaped text (`30`) IS admissible for a float target: the "at least
/// one of fraction or exponent" sentence distinguishes the float LITERAL form
/// from the integer one, while what a scalar means here is decided by the
/// target type (`interp[interp.coerce.none]`).
#[must_use]
pub fn parse_f64(text: &str) -> Option<f64> {
    match text {
        "inf" | "+inf" => return Some(f64::INFINITY),
        "-inf" => return Some(f64::NEG_INFINITY),
        "nan" => return Some(f64::NAN),
        _ => {}
    }

    let (negative, body) = split_sign(text);
    let (mantissa, exponent) = match body.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (body, None),
    };
    let (integer, fraction) = match mantissa.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (mantissa, None),
    };

    let is_digit: fn(char) -> bool = |ch| ch.is_ascii_digit();
    let mut normalized = String::with_capacity(text.len());
    if negative {
        normalized.push('-');
    }
    normalized.push_str(&strip_underscores(integer, is_digit)?);
    if let Some(fraction) = fraction {
        normalized.push('.');
        normalized.push_str(&strip_underscores(fraction, is_digit)?);
    }
    if let Some(exponent) = exponent {
        normalized.push('e');
        let (exponent_negative, exponent_body) = split_sign(exponent);
        if exponent_negative {
            normalized.push('-');
        }
        normalized.push_str(&strip_underscores(exponent_body, is_digit)?);
    }
    // Every remaining rejection (an empty part, a stray character) already
    // failed above; this parse only converts.
    normalized.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_integers_take_signs_leading_zeros_and_underscores() {
        // interp[interp.int.decimal]
        assert_eq!(parse_i64("8080"), Some(8080));
        assert_eq!(parse_i64("-42"), Some(-42));
        assert_eq!(parse_i64("+42"), Some(42));
        assert_eq!(parse_i64("007"), Some(7));
        assert_eq!(parse_i64("1_000_000"), Some(1_000_000));
        assert_eq!(parse_u64("1_000_000"), Some(1_000_000));
    }

    #[test]
    fn radix_prefixed_integers_are_read_in_their_base() {
        // interp[interp.int.hex] / octal / binary
        assert_eq!(parse_i64("0xff5500"), Some(16_733_440));
        assert_eq!(parse_i64("0XFF"), Some(255));
        assert_eq!(parse_i64("0xFF_FF"), Some(65535));
        assert_eq!(parse_i64("0o755"), Some(493));
        assert_eq!(parse_i64("0O755"), Some(493));
        assert_eq!(parse_i64("0b1010"), Some(10));
        assert_eq!(parse_i64("0B1111_0000"), Some(240));
        assert_eq!(parse_u64("0xff"), Some(255));
    }

    #[test]
    fn underscores_are_only_admissible_between_digits() {
        assert_eq!(parse_i64("_1"), None);
        assert_eq!(parse_i64("1_"), None);
        assert_eq!(parse_i64("1__0"), None);
        assert_eq!(parse_i64("0x_ff"), None);
        assert_eq!(parse_f64("1_.5"), None);
        assert_eq!(parse_f64("1.5_"), None);
    }

    #[test]
    fn a_radix_prefix_admits_only_its_own_digits() {
        assert_eq!(parse_i64("0o8"), None);
        assert_eq!(parse_i64("0b2"), None);
        assert_eq!(parse_i64("0xg"), None);
        assert_eq!(parse_i64("0x"), None);
    }

    #[test]
    fn a_sign_belongs_to_the_decimal_form_alone() {
        // The radix forms are specified as STARTING WITH their prefix, so a
        // signed one is not an integer and defers to the type error.
        assert_eq!(parse_i64("-0xff"), None);
        assert_eq!(parse_i64("+0b1"), None);
        assert_eq!(parse_u64("-1"), None);
    }

    #[test]
    fn integers_beyond_the_range_do_not_silently_wrap() {
        // interp[interp.int.range]
        assert_eq!(parse_i64("9223372036854775807"), Some(i64::MAX));
        assert_eq!(parse_i64("-9223372036854775808"), Some(i64::MIN));
        assert_eq!(parse_i64("9223372036854775808"), None);
        assert_eq!(parse_u64("18446744073709551616"), None);
    }

    #[test]
    fn floats_take_fractions_exponents_and_underscores() {
        // interp[interp.float.syntax]
        assert_eq!(parse_f64("12.5"), Some(12.5));
        assert_eq!(parse_f64("6.022e23"), Some(6.022e23));
        assert_eq!(parse_f64("1.5e-10"), Some(1.5e-10));
        assert_eq!(parse_f64("1E+3"), Some(1000.0));
        assert_eq!(parse_f64("1_234.062_5"), Some(1234.0625));
        assert_eq!(parse_f64("-273.15"), Some(-273.15));
        // Integer-shaped text reads as a float when a float is what was asked
        // for: the target type decides (interp[interp.coerce.none]).
        assert_eq!(parse_f64("30"), Some(30.0));
    }

    #[test]
    fn the_special_float_forms_are_closed_and_case_sensitive() {
        // interp[interp.float.special]
        assert_eq!(parse_f64("inf"), Some(f64::INFINITY));
        assert_eq!(parse_f64("+inf"), Some(f64::INFINITY));
        assert_eq!(parse_f64("-inf"), Some(f64::NEG_INFINITY));
        assert!(parse_f64("nan").is_some_and(f64::is_nan));
        // Rust's own parser takes all of these; the spec does not.
        assert_eq!(parse_f64("Infinity"), None);
        assert_eq!(parse_f64("infinity"), None);
        assert_eq!(parse_f64("INF"), None);
        assert_eq!(parse_f64("NaN"), None);
        assert_eq!(parse_f64("nAn"), None);
    }

    #[test]
    fn text_that_is_not_a_number_is_not_invented_into_one() {
        assert_eq!(parse_i64("localhost"), None);
        assert_eq!(parse_i64(""), None);
        assert_eq!(parse_i64("12abc"), None);
        assert_eq!(parse_f64(""), None);
        assert_eq!(parse_f64("1.2.3"), None);
        assert_eq!(parse_f64("1e"), None);
        assert_eq!(parse_f64("."), None);
    }
}

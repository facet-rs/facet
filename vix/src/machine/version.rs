use std::cmp::Ordering;

use semver::Version;

pub(super) fn parse(text: &str) -> Result<Version, String> {
    let normalized = normalize_input(text);
    Version::parse(&normalized).map_err(|err| format!("version({text:?}) parse error: {err}"))
}

pub(super) fn parse_bytes(bytes: &[u8]) -> Result<Version, String> {
    let text = std::str::from_utf8(bytes).map_err(|err| err.to_string())?;
    Version::parse(text).map_err(|err| format!("stored Version {text:?} parse error: {err}"))
}

pub(super) fn canonical_bytes(version: &Version) -> Vec<u8> {
    version.to_string().into_bytes()
}

pub(super) fn cmp_total(a: &[u8], b: &[u8]) -> Result<Ordering, String> {
    Ok(parse_bytes(a)?.cmp(&parse_bytes(b)?))
}

pub(super) fn cmp_precedence(a: &[u8], b: &[u8]) -> Result<Ordering, String> {
    Ok(parse_bytes(a)?.cmp_precedence(&parse_bytes(b)?))
}

fn normalize_input(text: &str) -> String {
    let text = text.strip_prefix("GLIBC_").unwrap_or(text);
    let (core_and_pre, build) = text
        .split_once('+')
        .map_or((text, None), |(core, build)| (core, Some(build)));
    let (core, pre) = core_and_pre
        .split_once('-')
        .map_or((core_and_pre, None), |(core, pre)| (core, Some(pre)));
    let dot_count = core.bytes().filter(|byte| *byte == b'.').count();
    let mut normalized = core.to_string();
    if dot_count == 1 {
        normalized.push_str(".0");
    }
    if let Some(pre) = pre {
        normalized.push('-');
        normalized.push_str(pre);
    }
    if let Some(build) = build {
        normalized.push('+');
        normalized.push_str(build);
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glibc_names_normalize_to_semver() {
        assert_eq!(parse("GLIBC_2.35").unwrap().to_string(), "2.35.0");
        assert!(parse("GLIBC_").is_err());
    }
}

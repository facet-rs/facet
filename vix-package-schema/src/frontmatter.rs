//! Slicing a manifest out of a `main.vix` without parsing vix.
//!
//! This is not a manifest parser and not a vix parser. It is the byte-level
//! half of `r[vixen.package.frontmatter-reads-without-vix]`: skip trivia,
//! expect `manifest`, read the fence, slice to the matching close, hand the
//! bytes to styx. A registry indexer, a pin-bumping bot, and an editor must
//! all be able to read a package's word without an evaluator, which is why
//! the manifest is a *fenced literal* and not a vix value written in vix
//! syntax.
//!
//! Deciding what the sliced bytes *mean* is still vix-side code. This module
//! only answers "which bytes, and where did they start".

/// A manifest found in a `.vix` source, plus where it started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frontmatter<'a> {
    /// The styx document, fence excluded.
    pub styx: &'a str,
    /// Byte offset of `styx` within the original source. Carried so a styx
    /// diagnostic inside the fence reports at its true position in the file,
    /// never at line 1 of a document that does not exist on disk.
    pub offset: usize,
}

/// Why a source carries no readable manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontmatterError {
    /// No `manifest` item. Not every `.vix` file has one — only a package's
    /// `main.vix` does — so this is not always an error to the caller.
    Missing,
    /// `manifest` was found, but the fence never opened.
    ExpectedFence { at: usize },
    /// The fence opened and never closed.
    Unterminated { opened_at: usize },
}

/// The fence: a quote-family block literal (`r[lang.literal.block]`). The
/// quote family admits no holes at all, which is what keeps the manifest dead
/// data by grammar rather than by doctrine.
const FENCE: &str = "\"\"\"";

/// Extract the manifest from a `.vix` source.
///
/// The item appears at most once, as the first item of the file; comments and
/// whitespace may precede it. Anything else before it means this file has no
/// frontmatter, which reads as [`FrontmatterError::Missing`] — a file whose
/// first item is a `fn` is an ordinary module, not a malformed package.
pub fn extract(source: &str) -> Result<Frontmatter<'_>, FrontmatterError> {
    let after_trivia = skip_trivia(source, 0);

    let rest = &source[after_trivia..];
    let Some(after_kw) = rest.strip_prefix("manifest") else {
        return Err(FrontmatterError::Missing);
    };
    // `manifestable` must not read as `manifest` followed by junk.
    if after_kw
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
    {
        return Err(FrontmatterError::Missing);
    }

    let fence_start = skip_trivia(source, after_trivia + "manifest".len());
    let Some(body_start) = source[fence_start..]
        .strip_prefix(FENCE)
        .map(|_| fence_start + FENCE.len())
    else {
        return Err(FrontmatterError::ExpectedFence { at: fence_start });
    };

    let Some(end) = source[body_start..].find(FENCE) else {
        return Err(FrontmatterError::Unterminated {
            opened_at: fence_start,
        });
    };

    Ok(Frontmatter {
        styx: &source[body_start..body_start + end],
        offset: body_start,
    })
}

/// Whitespace and `//` line comments. Deliberately not a vix lexer: the
/// frontmatter is the first item, so the only thing that can precede it is
/// trivia, and anything else means there is no frontmatter here.
fn skip_trivia(source: &str, mut at: usize) -> usize {
    loop {
        let rest = &source[at..];
        let trimmed = rest.trim_start();
        at = source.len() - trimmed.len();

        if source[at..].starts_with("//") {
            match source[at..].find('\n') {
                Some(nl) => at += nl + 1,
                None => return source.len(),
            }
        } else {
            return at;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_a_bare_manifest() {
        let src = "manifest \"\"\"\npackage { name hello }\n\"\"\"\n";
        let fm = extract(src).unwrap();
        assert_eq!(fm.styx, "\npackage { name hello }\n");
        assert_eq!(&src[fm.offset..fm.offset + fm.styx.len()], fm.styx);
    }

    #[test]
    fn comments_and_whitespace_may_precede_it() {
        let src = "// a package\n// two lines of it\n\nmanifest \"\"\"\nx y\n\"\"\"";
        assert_eq!(extract(src).unwrap().styx, "\nx y\n");
    }

    #[test]
    fn the_offset_is_the_true_position() {
        let src = "// lead\nmanifest \"\"\"BODY\"\"\"";
        let fm = extract(src).unwrap();
        assert_eq!(fm.styx, "BODY");
        assert_eq!(&src[fm.offset..], "BODY\"\"\"");
    }

    #[test]
    fn a_module_without_frontmatter_is_missing_not_malformed() {
        assert_eq!(
            extract("fn build() -> Tree { ... }"),
            Err(FrontmatterError::Missing)
        );
        assert_eq!(extract(""), Err(FrontmatterError::Missing));
    }

    #[test]
    fn manifest_must_be_the_whole_word() {
        assert_eq!(extract("manifesto \"\"\"x\"\"\""), Err(FrontmatterError::Missing));
    }

    #[test]
    fn an_unopened_fence_is_not_a_missing_manifest() {
        assert!(matches!(
            extract("manifest package { }"),
            Err(FrontmatterError::ExpectedFence { .. })
        ));
    }

    #[test]
    fn an_unterminated_fence_says_where_it_opened() {
        let Err(FrontmatterError::Unterminated { opened_at }) = extract("manifest \"\"\"oops")
        else {
            panic!("expected Unterminated");
        };
        assert_eq!(&"manifest \"\"\"oops"[opened_at..], "\"\"\"oops");
    }
}

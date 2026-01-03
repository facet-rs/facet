//! Roundtrip tests for HTML fixtures using datatest-stable.
//!
//! Each HTML fixture file becomes an individual test case, verifying that:
//! 1. The file can be parsed into an Html document
//! 2. The document can be serialized back to HTML
//! 3. The serialized HTML can be re-parsed
//! 4. Re-serialization produces identical output (idempotence)
//!
//! Note: We don't compare the original parsed structure with the reparsed structure
//! because serialization normalizes whitespace. The key property we test is that
//! once normalized, the roundtrip is stable (same output on each serialization).

use facet_html_dom::Html;
use std::path::Path;

/// Files that cause stack overflow due to deep nesting.
/// See: https://github.com/facet-rs/facet/issues/1582
const SKIP_STACK_OVERFLOW: &[&str] = &[
    "https_fasterthanli.me.html",
    "https_nltk.org_howto_corpus.html",
    "https_stackoverflow.com_questions_53390843_creating-corpus-from-multiple-html-text-files.html",
    "https_en.wikipedia.org_wiki_Markup_language.html",
    "https_developer.mozilla.org_en-US_docs_Web_HTML.html",
    "https_markdownguide.org_basic-syntax.html",
    "https_info.arxiv.org_about_accessible_HTML.html",
    "https_w3.org_TR_2010_WD-html-markup-20101019.html",
];

fn html_roundtrip_test(path: &Path) -> datatest_stable::Result<()> {
    // Skip files known to cause stack overflow (issue #1582)
    if let Some(filename) = path.file_name().and_then(|f| f.to_str())
        && SKIP_STACK_OVERFLOW.contains(&filename)
    {
        eprintln!(
            "Skipping {} (causes stack overflow, see issue #1582)",
            filename
        );
        return Ok(());
    }

    let html_str = std::fs::read_to_string(path)?;

    // Step 1: Parse the original HTML
    let parsed: Html = facet_html::from_str(&html_str)
        .map_err(|e| format!("Failed to parse HTML from {}: {}", path.display(), e))?;

    // Step 2: Serialize back to HTML (this normalizes whitespace)
    let serialized =
        facet_html::to_string(&parsed).map_err(|e| format!("Failed to serialize HTML: {}", e))?;

    // Step 3: Re-parse the serialized output
    let _reparsed: Html = facet_html::from_str(&serialized)
        .map_err(|e| format!("Failed to re-parse serialized HTML: {}", e))?;

    // Step 4: Serialize again
    let reserialized = facet_html::to_string(&_reparsed)
        .map_err(|e| format!("Failed to serialize HTML again: {}", e))?;

    // Step 5: Verify serialization is idempotent - the key property we care about
    assert_eq!(
        serialized,
        reserialized,
        "Serialized HTML should be identical after roundtrip for {}",
        path.display()
    );

    Ok(())
}

datatest_stable::harness! {
    { test = html_roundtrip_test, root = "tests/fixtures", pattern = r".*\.html$" },
}

//! Integration tests for facet-miette

#![allow(unused_variables)]

use facet::Facet;
use facet_miette as diagnostic;
use miette::{Diagnostic, SourceSpan};

/// A simple parse error with diagnostic info
#[derive(Facet, Debug)]
#[facet(derive(Error, facet_miette::Diagnostic))]
#[repr(u8)]
pub enum ParseError {
    /// Unexpected token found
    #[facet(diagnostic::code = "parse::unexpected_token")]
    #[facet(diagnostic::help = "Check for typos or missing delimiters")]
    UnexpectedToken {
        #[facet(diagnostic::source_code)]
        src: String,
        #[facet(opaque)]
        #[facet(diagnostic::label = "this token was unexpected")]
        span: SourceSpan,
    },

    /// End of file reached unexpectedly
    #[facet(diagnostic::code = "parse::eof")]
    #[facet(diagnostic::severity = "error")]
    UnexpectedEof,

    /// Non-critical warning
    #[facet(diagnostic::code = "parse::deprecation")]
    #[facet(diagnostic::severity = "warning")]
    #[facet(diagnostic::url = "https://example.com/deprecations")]
    DeprecatedSyntax,

    /// Just advice
    #[facet(diagnostic::severity = "advice")]
    StyleSuggestion,

    /// No diagnostic metadata
    Unknown,
}

#[test]
fn test_code() {
    let err = ParseError::UnexpectedToken {
        src: "let x = ;".to_string(),
        span: (8, 1).into(),
    };

    let code = err.code().map(|c| c.to_string());
    assert_eq!(code, Some("parse::unexpected_token".to_string()));

    let err2 = ParseError::Unknown;
    assert!(err2.code().is_none());
}

#[test]
fn test_help() {
    let err = ParseError::UnexpectedToken {
        src: "let x = ;".to_string(),
        span: (8, 1).into(),
    };

    let help = err.help().map(|h| h.to_string());
    assert_eq!(
        help,
        Some("Check for typos or missing delimiters".to_string())
    );

    let err2 = ParseError::Unknown;
    assert!(err2.help().is_none());
}

#[test]
fn test_severity() {
    let err = ParseError::UnexpectedEof;
    assert_eq!(err.severity(), Some(miette::Severity::Error));

    let warn = ParseError::DeprecatedSyntax;
    assert_eq!(warn.severity(), Some(miette::Severity::Warning));

    let advice = ParseError::StyleSuggestion;
    assert_eq!(advice.severity(), Some(miette::Severity::Advice));

    let unknown = ParseError::Unknown;
    assert!(unknown.severity().is_none());
}

#[test]
fn test_url() {
    let err = ParseError::DeprecatedSyntax;
    let url = err.url().map(|u| u.to_string());
    assert_eq!(url, Some("https://example.com/deprecations".to_string()));

    let err2 = ParseError::Unknown;
    assert!(err2.url().is_none());
}

#[test]
fn test_source_code() {
    let err = ParseError::UnexpectedToken {
        src: "let x = ;".to_string(),
        span: (8, 1).into(),
    };

    // Should have source code
    assert!(err.source_code().is_some());

    let err2 = ParseError::Unknown;
    assert!(err2.source_code().is_none());
}

#[test]
fn test_labels() {
    let err = ParseError::UnexpectedToken {
        src: "let x = ;".to_string(),
        span: (8, 1).into(),
    };

    let labels: Vec<_> = err.labels().unwrap().collect();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].label(), Some("this token was unexpected"));
    assert_eq!(labels[0].offset(), 8);
    assert_eq!(labels[0].len(), 1);

    let err2 = ParseError::Unknown;
    assert!(err2.labels().is_none());
}

#[test]
fn test_display_from_error_plugin() {
    // The Error plugin provides Display
    let err = ParseError::UnexpectedToken {
        src: "let x = ;".to_string(),
        span: (8, 1).into(),
    };
    assert_eq!(format!("{}", err), "Unexpected token found");

    let err2 = ParseError::UnexpectedEof;
    assert_eq!(format!("{}", err2), "End of file reached unexpectedly");
}

#[test]
fn test_error_source_from_error_plugin() {
    // The Error plugin provides std::error::Error
    use std::error::Error;

    let err = ParseError::Unknown;
    assert!(err.source().is_none());
}

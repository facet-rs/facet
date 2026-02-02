//! Parse styx into query schema types.
//!
//! Uses facet-styx for parsing.

use crate::{QError, QErrorKind, QSource};
use camino::Utf8Path;
use dibs_query_schema::{QueryFile, Span};
use std::sync::Arc;

/// Parse a styx source string into a QueryFile.
pub fn parse_query_file(source_path: &Utf8Path, source: &str) -> Result<QueryFile, QError> {
    facet_styx::from_str(source).map_err(|e| {
        let span = e.span.unwrap_or(Span { offset: 0, len: 0 });
        QError {
            source: Arc::new(QSource {
                source: source.to_string(),
                source_path: source_path.to_owned(),
            }),
            span,
            kind: QErrorKind::Parse {
                message: e.kind.to_string(),
            },
        }
    })
}

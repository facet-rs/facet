//! Parse styx into query schema types.
//!
//! Uses facet-styx for parsing.

use crate::{QError, QErrorKind, QSource};
use camino::Utf8Path;
use dibs_query_schema::{QueryFile, Span};
use std::sync::Arc;

/// Parse a styx source string into a QueryFile.
///
/// Returns both the parsed QueryFile and the QSource for error reporting.
/// Validation is deferred to SQL generation phase for proper context.
pub fn parse_query_file(
    source_path: &Utf8Path,
    source: &str,
) -> Result<(QueryFile, Arc<QSource>), QError> {
    let qsource = Arc::new(QSource {
        source: source.to_string(),
        source_path: source_path.to_owned(),
    });

    let query_file: QueryFile = facet_styx::from_str(source).map_err(|e| {
        let span = e.span.unwrap_or(Span { offset: 0, len: 0 });
        QError {
            source: qsource.clone(),
            span,
            kind: QErrorKind::Parse {
                message: e.kind.to_string(),
            },
        }
    })?;

    // Note: Filter validation is now done during SQL generation
    // where we have proper context for rich error messages

    Ok((query_file, qsource))
}

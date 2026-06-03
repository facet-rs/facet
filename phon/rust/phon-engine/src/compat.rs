//! Shared compatibility rule mechanics for the dynamic-value and typed-memory
//! decode paths.
//!
//! The two paths produce different programs, but field and variant matching are
//! the same schema-level decisions. Keep those decisions here so `plan` and
//! `typed` cannot silently diverge.

use std::collections::HashMap;

use phon_schema::{Field, Variant};

use crate::compact::CompactError;

type Result<T> = core::result::Result<T, CompactError>;

pub(crate) enum FieldMatch<'a> {
    Take {
        writer: &'a Field,
        reader_index: usize,
    },
    Skip {
        writer: &'a Field,
    },
    Default {
        reader_index: usize,
    },
}

pub(crate) enum VariantMatch<'a> {
    Take {
        writer: &'a Variant,
        reader_index: usize,
    },
    WriterOnly {
        writer: &'a Variant,
    },
}

pub(crate) fn incompatible(why: impl Into<String>) -> CompactError {
    CompactError::Incompatible(why.into())
}

// r[impl compat.field-matching]
// r[impl compat.skip-writer-only]
// r[impl compat.reader-only-fields]
// r[impl compat.defaults-are-reader-side]
pub(crate) fn match_fields<'a>(
    w_fields: &'a [Field],
    r_fields: &'a [Field],
    mut can_default: impl FnMut(usize, &'a Field) -> bool,
    mut missing_required: impl FnMut(&'a Field) -> CompactError,
) -> Result<Vec<FieldMatch<'a>>> {
    let reader_by_name: HashMap<&str, usize> = r_fields
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();

    let mut matched = vec![false; r_fields.len()];
    let mut steps = Vec::with_capacity(w_fields.len() + r_fields.len());

    for wf in w_fields {
        if let Some(&ri) = reader_by_name.get(wf.name.as_str()) {
            matched[ri] = true;
            steps.push(FieldMatch::Take {
                writer: wf,
                reader_index: ri,
            });
        } else {
            steps.push(FieldMatch::Skip { writer: wf });
        }
    }

    for (ri, rf) in r_fields.iter().enumerate() {
        if matched[ri] {
            continue;
        }
        if !can_default(ri, rf) {
            return Err(missing_required(rf));
        }
        steps.push(FieldMatch::Default { reader_index: ri });
    }

    Ok(steps)
}

// r[impl compat.enum]
pub(crate) fn match_variants<'a>(
    w_variants: &'a [Variant],
    r_variants: &'a [Variant],
) -> Vec<VariantMatch<'a>> {
    let reader_by_name: HashMap<&str, usize> = r_variants
        .iter()
        .enumerate()
        .map(|(i, v)| (v.name.as_str(), i))
        .collect();

    let mut steps = Vec::with_capacity(w_variants.len());
    for wv in w_variants {
        match reader_by_name.get(wv.name.as_str()) {
            Some(&ri) => steps.push(VariantMatch::Take {
                writer: wv,
                reader_index: ri,
            }),
            None => steps.push(VariantMatch::WriterOnly { writer: wv }),
        }
    }
    steps
}

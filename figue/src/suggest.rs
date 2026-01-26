//! String similarity suggestions for typos using Jaro-Winkler distance.

/// Minimum similarity threshold for suggestions (Jaro-Winkler).
const SIMILARITY_THRESHOLD: f64 = 0.8;

/// Find the best matching candidate for a query string.
///
/// Returns the best match if:
/// - There is at least one candidate with similarity >= `SIMILARITY_THRESHOLD`
/// - Case-insensitive matching is used for comparison
///
/// Returns `None` if no good match is found.
pub fn find_best_match<'a>(
    query: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    let query_lower = query.to_lowercase();

    candidates
        .into_iter()
        .filter_map(|candidate| {
            let candidate_lower = candidate.to_lowercase();
            let similarity = strsim::jaro_winkler(&query_lower, &candidate_lower);
            if similarity >= SIMILARITY_THRESHOLD {
                Some((candidate, similarity))
            } else {
                None
            }
        })
        .max_by(|(_, sim_a), (_, sim_b)| {
            sim_a
                .partial_cmp(sim_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(candidate, _)| candidate)
}

/// Format a "did you mean...?" suggestion if a good match is found.
///
/// Returns an empty string if no good match exists.
pub fn format_suggestion<'a>(query: &str, candidates: impl IntoIterator<Item = &'a str>) -> String {
    match find_best_match(query, candidates) {
        Some(suggestion) => format!(". Did you mean '{suggestion}'?"),
        None => String::new(),
    }
}

/// Try to suggest a similar path for an unknown config key.
///
/// Walks through the path segments and attempts to find which segment
/// doesn't match the schema. Returns a suggestion for the failing segment
/// if one can be found.
pub fn suggest_config_path(schema: &crate::schema::ConfigStructSchema, path: &[String]) -> String {
    use crate::schema::ConfigValueSchema;

    if path.is_empty() {
        return String::new();
    }

    // We need to track the current struct schema we're navigating
    let mut current_struct = Some(schema);
    let mut current_enum_variants: Option<Vec<&str>> = None;

    for (i, segment) in path.iter().enumerate() {
        let segment_lower = segment.to_lowercase();

        // Determine what we're matching against
        if let Some(enum_variants) = &current_enum_variants {
            // We're looking for an enum variant
            let found = enum_variants
                .iter()
                .any(|v| v.to_lowercase() == segment_lower);
            if !found {
                return format_suggestion(segment, enum_variants.iter().copied());
            }
            // TODO: Handle struct variants - for now, stop here
            current_enum_variants = None;
            current_struct = None;
            continue;
        }

        if let Some(struct_schema) = current_struct {
            let fields = struct_schema.fields();
            let field_names: Vec<&str> = fields.keys().map(|s| s.as_str()).collect();

            // Check if this segment exists (case-insensitive)
            let matching_field = fields
                .iter()
                .find(|(k, _)| k.to_lowercase() == segment_lower);

            if matching_field.is_none() {
                // This is the failing segment - try to suggest an alternative
                return format_suggestion(segment, field_names.iter().copied());
            }

            // Get the next level of fields if there are more segments
            if i + 1 < path.len() {
                let (_, field_schema) = matching_field.unwrap();
                let value_schema = unwrap_option(field_schema.value());

                match value_schema {
                    ConfigValueSchema::Struct(s) => {
                        current_struct = Some(s);
                        current_enum_variants = None;
                    }
                    ConfigValueSchema::Enum(e) => {
                        current_struct = None;
                        current_enum_variants =
                            Some(e.variants().keys().map(|s| s.as_str()).collect());
                    }
                    _ => {
                        // Can't navigate further (leaf or array)
                        return String::new();
                    }
                }
            }
        } else {
            // We're in an unknown state, can't suggest
            return String::new();
        }
    }

    String::new()
}

/// Helper to unwrap Option wrapper in ConfigValueSchema
fn unwrap_option(schema: &crate::schema::ConfigValueSchema) -> &crate::schema::ConfigValueSchema {
    use crate::schema::ConfigValueSchema;
    match schema {
        ConfigValueSchema::Option { value, .. } => unwrap_option(value),
        other => other,
    }
}

/// Suggest a similar CLI flag name.
///
/// Takes a flag name (in kebab-case from the CLI) and all available flag names
/// (will be converted to kebab-case for comparison).
pub fn suggest_flag<'a>(query: &str, flag_names: impl IntoIterator<Item = &'a str>) -> String {
    use heck::ToKebabCase as _;

    // Convert all flag names to kebab-case for comparison
    let candidates: Vec<(String, &'a str)> = flag_names
        .into_iter()
        .map(|name| (name.to_kebab_case(), name))
        .collect();

    // Find best match
    let query_lower = query.to_lowercase();
    let best_match = candidates
        .iter()
        .filter_map(|(kebab, original)| {
            let similarity = strsim::jaro_winkler(&query_lower, &kebab.to_lowercase());
            if similarity >= SIMILARITY_THRESHOLD {
                Some((kebab.as_str(), *original, similarity))
            } else {
                None
            }
        })
        .max_by(|(_, _, sim_a), (_, _, sim_b)| {
            sim_a
                .partial_cmp(sim_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(kebab, _, _)| kebab);

    match best_match {
        Some(suggestion) => format!(". Did you mean '--{suggestion}'?"),
        None => String::new(),
    }
}

/// Suggest a similar subcommand name.
pub fn suggest_subcommand<'a>(
    query: &str,
    subcommand_names: impl IntoIterator<Item = &'a str>,
) -> String {
    match find_best_match(query, subcommand_names) {
        Some(suggestion) => format!(". Did you mean '{suggestion}'?"),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_best_match_exact() {
        let candidates = ["Debug", "Info", "Warn", "Error"];
        assert_eq!(find_best_match("Debug", candidates), Some("Debug"));
    }

    #[test]
    fn test_find_best_match_typo() {
        let candidates = ["Debug", "Info", "Warn", "Error"];
        // "Debugg" should match "Debug"
        assert_eq!(find_best_match("Debugg", candidates), Some("Debug"));
        // "Errror" should match "Error"
        assert_eq!(find_best_match("Errror", candidates), Some("Error"));
    }

    #[test]
    fn test_find_best_match_case_insensitive() {
        let candidates = ["Debug", "Info", "Warn", "Error"];
        assert_eq!(find_best_match("debug", candidates), Some("Debug"));
        assert_eq!(find_best_match("DEBUG", candidates), Some("Debug"));
    }

    #[test]
    fn test_find_best_match_no_match() {
        let candidates = ["Debug", "Info", "Warn", "Error"];
        // Completely different string
        assert_eq!(find_best_match("XYZ123", candidates), None);
    }

    #[test]
    fn test_format_suggestion_with_match() {
        let candidates = ["port", "host", "timeout"];
        assert_eq!(
            format_suggestion("portt", candidates),
            ". Did you mean 'port'?"
        );
    }

    #[test]
    fn test_format_suggestion_no_match() {
        let candidates = ["port", "host", "timeout"];
        assert_eq!(format_suggestion("completely_different", candidates), "");
    }
}

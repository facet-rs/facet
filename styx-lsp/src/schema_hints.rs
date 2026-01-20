//! Schema hints registry for suggesting schemas based on file paths.
//!
//! When a .styx file matches a known pattern but has no `@schema` declaration,
//! the LSP shows a warning and offers a code action to add it.

use std::collections::HashMap;
use std::path::Path;

use facet::Facet;
use tower_lsp::lsp_types::Url;

/// The embedded schema hints registry.
const SCHEMA_HINTS_SOURCE: &str = include_str!("../../../registry/schema-hints.styx");

/// A schema hint entry for a tool.
#[derive(Debug, Clone, Facet)]
pub struct Hint {
    /// Human-readable name of the tool.
    #[facet(default)]
    pub title: Option<String>,
    /// Brief description of what the tool does.
    #[facet(default)]
    pub description: Option<String>,
    /// Homepage or documentation URL.
    #[facet(default)]
    pub homepage: Option<String>,
    /// File path patterns that should use this schema.
    pub patterns: Vec<String>,
    /// Schema reference to suggest.
    pub schema: SchemaRef,
}

/// Reference to a schema for the @schema declaration.
#[derive(Debug, Clone, Facet)]
pub struct SchemaRef {
    /// Schema identifier (e.g., "crate:myapp-config@1").
    pub id: String,
    /// CLI binary name for embedded schema extraction.
    #[facet(default)]
    pub cli: Option<String>,
}

/// The root structure of the schema hints registry.
#[derive(Debug, Clone, Facet)]
struct HintsRegistry {
    hints: HashMap<String, Hint>,
}

/// A matched schema hint with context.
#[derive(Debug, Clone)]
pub struct SchemaMatch {
    /// The tool name (key in the registry).
    pub tool_name: String,
    /// The hint entry.
    pub hint: Hint,
    /// The pattern that matched.
    pub matched_pattern: String,
}

impl SchemaMatch {
    /// Generate the @schema declaration text.
    pub fn schema_declaration(&self) -> String {
        let ref_ = &self.hint.schema;
        if let Some(cli) = &ref_.cli {
            format!("@schema {{id {}, cli {}}}", ref_.id, cli)
        } else {
            format!("@schema {{id {}}}", ref_.id)
        }
    }

    /// Generate a human-readable description for the diagnostic.
    pub fn description(&self) -> String {
        if let Some(title) = &self.hint.title {
            if let Some(desc) = &self.hint.description {
                format!("{}: {}", title, desc)
            } else {
                title.clone()
            }
        } else {
            format!("schema for {}", self.tool_name)
        }
    }
}

/// Load and parse the schema hints registry.
fn load_registry() -> Option<HintsRegistry> {
    facet_styx::from_str(SCHEMA_HINTS_SOURCE).ok()
}

/// Find the git root directory for a given path.
fn find_git_root(path: &Path) -> Option<std::path::PathBuf> {
    let mut current = path;
    loop {
        if current.join(".git").exists() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

/// Get the user config directory.
fn get_user_config_dir() -> Option<std::path::PathBuf> {
    dirs::config_dir()
}

/// Get the user home directory.
fn get_home_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir()
}

/// Check if a file path matches a pattern with variable substitution.
fn matches_pattern(file_path: &Path, pattern: &str, git_root: Option<&Path>) -> bool {
    // Expand variables in the pattern
    let expanded = expand_pattern(pattern, git_root);

    // Handle glob patterns
    if expanded.contains('*') {
        // Use glob matching
        if let Ok(glob_pattern) = glob::Pattern::new(&expanded) {
            return glob_pattern.matches_path(file_path);
        }
        return false;
    }

    // Exact match
    file_path == Path::new(&expanded)
}

/// Expand pattern variables like {git_root}, {userconfig}, {home}.
fn expand_pattern(pattern: &str, git_root: Option<&Path>) -> String {
    let mut result = pattern.to_string();

    // Replace {git_root}
    if let Some(root) = git_root {
        result = result.replace("{git_root}", &root.to_string_lossy());
    } else {
        // If no git root, patterns with {git_root} can't match
        if result.contains("{git_root}") {
            return String::new(); // Return empty string to prevent matching
        }
    }

    // Replace {userconfig}
    if let Some(config_dir) = get_user_config_dir() {
        result = result.replace("{userconfig}", &config_dir.to_string_lossy());
    }

    // Replace {home}
    if let Some(home_dir) = get_home_dir() {
        result = result.replace("{home}", &home_dir.to_string_lossy());
    }

    result
}

/// Find a matching schema hint for a document URI.
///
/// Returns the first matching hint, or None if no pattern matches.
pub fn find_matching_hint(document_uri: &Url) -> Option<SchemaMatch> {
    let file_path = document_uri.to_file_path().ok()?;

    // Find git root for this file
    let git_root = find_git_root(&file_path);

    // Load the registry
    let registry = load_registry()?;

    // Check each hint's patterns
    for (tool_name, hint) in registry.hints {
        for pattern in &hint.patterns {
            if matches_pattern(&file_path, pattern, git_root.as_deref()) {
                return Some(SchemaMatch {
                    tool_name,
                    hint: hint.clone(),
                    matched_pattern: pattern.clone(),
                });
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_registry() {
        let registry = load_registry().expect("should load registry");
        assert!(!registry.hints.is_empty(), "registry should have hints");

        // Check that tracey is present
        let tracey = registry.hints.get("tracey").expect("tracey should exist");
        assert_eq!(tracey.schema.id, "crate:tracey-config@1");
        assert_eq!(tracey.schema.cli, Some("tracey".to_string()));
    }

    #[test]
    fn test_expand_pattern() {
        let git_root = Path::new("/home/user/project");

        // Test {git_root} expansion (still supported for custom patterns)
        let expanded = expand_pattern("{git_root}/.config/tracey/config.styx", Some(git_root));
        assert_eq!(expanded, "/home/user/project/.config/tracey/config.styx");

        // Glob patterns pass through unchanged
        let expanded = expand_pattern("**/.config/tracey/config.styx", Some(git_root));
        assert_eq!(expanded, "**/.config/tracey/config.styx");
    }

    #[test]
    fn test_schema_declaration() {
        let hint = Hint {
            title: Some("Test".to_string()),
            description: None,
            homepage: None,
            patterns: vec![],
            schema: SchemaRef {
                id: "crate:test@1".to_string(),
                cli: Some("test-cli".to_string()),
            },
        };

        let match_ = SchemaMatch {
            tool_name: "test".to_string(),
            hint,
            matched_pattern: "test".to_string(),
        };

        assert_eq!(
            match_.schema_declaration(),
            "@schema {id crate:test@1, cli test-cli}"
        );
    }
}

//! Typed raw `tree-sitter.json` package configuration.

use facet::Facet;

#[cfg(feature = "json-import")]
use crate::{
    diagnostic::{ImportError, JsonDocumentKind},
    source::{PackageRoot, SourceFile},
};

/// Raw `tree-sitter.json` source plus decoded package configuration.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct TreeSitterConfigJson {
    /// Original JSON source.
    pub raw: String,
    /// Decoded package configuration.
    pub config: TreeSitterConfig,
}

impl TreeSitterConfigJson {
    /// Import a `tree-sitter.json` source file.
    #[cfg(feature = "json-import")]
    pub fn from_source_file(
        root: &PackageRoot,
        source_file: SourceFile<String>,
    ) -> Result<SourceFile<Self>, ImportError> {
        let path = root.join(&source_file.path);
        let source_id = source_file.id;
        let package_path = source_file.path.clone();
        let config =
            facet_json::from_str(&source_file.body).map_err(|source| ImportError::Json {
                package_root: Some(root.as_path().to_owned()),
                path: Some(path),
                source_id: Some(source_id),
                package_path: Some(package_path),
                document: JsonDocumentKind::TreeSitterConfig,
                phase: "decode raw tree-sitter.json",
                source,
            })?;
        Ok(source_file.map(|raw| Self { raw, config }))
    }
}

/// Tree-sitter package configuration.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct TreeSitterConfig {
    /// Optional schema URI.
    #[facet(rename = "$schema")]
    pub schema: Option<String>,
    /// Grammar entries declared by this package.
    pub grammars: Vec<TreeSitterGrammarConfig>,
    /// Package metadata used by generators and bindings.
    pub metadata: TreeSitterPackageMetadata,
    /// Optional generated binding flags.
    pub bindings: Option<TreeSitterBindings>,
}

/// One grammar entry in `tree-sitter.json`.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct TreeSitterGrammarConfig {
    /// Grammar name.
    pub name: String,
    /// Optional CamelCase grammar name.
    pub camelcase: Option<String>,
    /// Optional display title.
    pub title: Option<String>,
    /// TextMate scope.
    pub scope: String,
    /// Relative grammar directory path.
    pub path: Option<String>,
    /// External files that affect regeneration.
    #[facet(rename = "external-files")]
    pub external_files: Option<Vec<String>>,
    /// File type suffixes.
    #[facet(rename = "file-types")]
    pub file_types: Option<Vec<String>>,
    /// Highlight query paths.
    pub highlights: Option<QueryPaths>,
    /// Injection query paths.
    pub injections: Option<QueryPaths>,
    /// Locals query paths.
    pub locals: Option<QueryPaths>,
    /// Tags query paths.
    pub tags: Option<QueryPaths>,
    /// Language-injection regex.
    #[facet(rename = "injection-regex")]
    pub injection_regex: Option<String>,
    /// First-line detection regex.
    #[facet(rename = "first-line-regex")]
    pub first_line_regex: Option<String>,
    /// Content detection regex.
    #[facet(rename = "content-regex")]
    pub content_regex: Option<String>,
    /// Swift/Java/Kotlin binding class name.
    #[facet(rename = "class-name")]
    pub class_name: Option<String>,
}

/// Query paths can be a single string or an ordered list.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
#[facet(untagged)]
#[repr(u8)]
pub enum QueryPaths {
    /// Single query file path.
    One(String),
    /// Ordered query file paths.
    Many(Vec<String>),
}

impl QueryPaths {
    /// Return query paths in configured order.
    pub fn as_slice(&self) -> &[String] {
        match self {
            Self::One(path) => std::slice::from_ref(path),
            Self::Many(paths) => paths.as_slice(),
        }
    }

    /// Iterate query paths in configured order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.as_slice().iter().map(String::as_str)
    }
}

/// Package metadata from `tree-sitter.json`.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct TreeSitterPackageMetadata {
    /// Package version.
    pub version: String,
    /// Package license.
    pub license: Option<String>,
    /// Package description.
    pub description: Option<String>,
    /// Package authors.
    #[facet(default)]
    pub authors: Vec<TreeSitterAuthor>,
    /// Repository and funding links.
    pub links: TreeSitterLinks,
    /// Java/Kotlin package namespace.
    pub namespace: Option<String>,
}

/// Package author entry.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct TreeSitterAuthor {
    /// Author name.
    pub name: String,
    /// Author email.
    pub email: Option<String>,
    /// Author URL.
    pub url: Option<String>,
}

/// Package links.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct TreeSitterLinks {
    /// Repository URL.
    pub repository: String,
    /// Funding URL.
    pub funding: Option<String>,
}

/// Generated binding flags.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct TreeSitterBindings {
    /// C binding.
    pub c: Option<bool>,
    /// Go binding.
    pub go: Option<bool>,
    /// Java binding.
    pub java: Option<bool>,
    /// Kotlin binding.
    pub kotlin: Option<bool>,
    /// Node binding.
    pub node: Option<bool>,
    /// Python binding.
    pub python: Option<bool>,
    /// Rust binding.
    pub rust: Option<bool>,
    /// Swift binding.
    pub swift: Option<bool>,
    /// Zig binding.
    pub zig: Option<bool>,
}

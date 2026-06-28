//! Typed raw `node-types.json` metadata.
//!
//! This is observed Tree-sitter package metadata for compatibility checks and
//! diagnostics. It is not the source of Snark parser shape; validated grammar
//! facts own parser symbols, fields, aliases, and productions.

use facet::Facet;
use indexmap::IndexMap;

#[cfg(feature = "json-import")]
use crate::{
    diagnostic::{ImportError, JsonDocumentKind},
    source::{PackageRoot, SourceFile},
};

type FieldMap = IndexMap<String, NodeFieldInfo, std::hash::RandomState>;

/// Raw `node-types.json` source plus decoded compatibility metadata.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct NodeTypesJson {
    /// Original JSON source.
    pub raw: String,
    /// Decoded node metadata.
    pub node_types: Vec<NodeInfo>,
}

impl NodeTypesJson {
    /// Import a `src/node-types.json` source file.
    #[cfg(feature = "json-import")]
    pub fn from_source_file(
        root: &PackageRoot,
        source_file: SourceFile<String>,
    ) -> Result<SourceFile<Self>, ImportError> {
        let path = root.join(&source_file.path);
        let source_id = source_file.id;
        let package_path = source_file.path.clone();
        let node_types =
            facet_json::from_str(&source_file.body).map_err(|source| ImportError::Json {
                package_root: Some(root.as_path().to_owned()),
                path: Some(path),
                source_id: Some(source_id),
                package_path: Some(package_path),
                document: JsonDocumentKind::NodeTypes,
                phase: "decode raw node-types.json",
                source,
            })?;
        Ok(source_file.map(|raw| Self { raw, node_types }))
    }
}

/// Tree-sitter node type metadata entry.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct NodeInfo {
    /// Node type name.
    #[facet(rename = "type")]
    pub kind: String,
    /// Whether the node type is named.
    pub named: bool,
    /// Whether this node is the grammar root.
    #[facet(default)]
    pub root: bool,
    /// Whether this node is an extra token.
    #[facet(default)]
    pub extra: bool,
    /// Named fields for regular nodes.
    #[facet(default)]
    pub fields: NodeFieldTable,
    /// Children for regular nodes.
    pub children: Option<NodeFieldInfo>,
    /// Subtypes for supertypes.
    #[facet(default)]
    pub subtypes: Vec<NodeTypeRef>,
}

/// Ordered field metadata table.
#[derive(Debug, Clone, Default, Facet, PartialEq, Eq)]
#[facet(transparent)]
pub struct NodeFieldTable(FieldMap);

impl NodeFieldTable {
    /// Number of fields.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no fields.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get a field by name.
    pub fn get(&self, name: &str) -> Option<&NodeFieldInfo> {
        self.0.get(name)
    }

    /// Iterate fields in source order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &NodeFieldInfo)> {
        self.0.iter().map(|(name, info)| (name.as_str(), info))
    }
}

/// Field or children metadata.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct NodeFieldInfo {
    /// Whether this field can appear multiple times.
    pub multiple: bool,
    /// Whether this field is required.
    pub required: bool,
    /// Accepted node types.
    #[facet(default)]
    pub types: Vec<NodeTypeRef>,
}

/// Reference to a node type.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct NodeTypeRef {
    /// Node type name.
    #[facet(rename = "type")]
    pub kind: String,
    /// Whether the node type is named.
    pub named: bool,
}

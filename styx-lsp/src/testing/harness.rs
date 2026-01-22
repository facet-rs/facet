//! Integration test harness for LSP extensions.
//!
//! This harness spawns extension binaries as subprocesses and communicates
//! with them over roam (COBS-framed protocol over stdio), exactly like the
//! real Styx LSP does.
//!
//! # Example
//!
//! ```ignore
//! use styx_lsp::testing::{TestHarness, TestDocument};
//!
//! #[tokio::test]
//! async fn test_completions() {
//!     let harness = TestHarness::spawn(&["dibs", "lsp-extension"]).await.unwrap();
//!     harness.initialize("file:///test.styx", "crate:dibs-queries@1").await.unwrap();
//!
//!     let doc = TestDocument::new("file:///test.styx", "from |");
//!     harness.load_document(doc).await;
//!
//!     let completions = harness.completions("file:///test.styx").await.unwrap();
//!     assert!(completions.iter().any(|c| c.label == "product"));
//! }
//! ```

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;

use roam_session::HandshakeConfig;
use roam_stream::CobsFramed;
use styx_cst::parse;
use styx_tree::Value;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tower_lsp::lsp_types::Url;

use crate::extensions::{ChildStdio, StyxLspHostImpl};
use crate::server::{DocumentMap, DocumentState};
use styx_lsp_ext::{
    CompletionItem, CompletionParams, Cursor, DefinitionParams, Diagnostic, DiagnosticParams,
    HoverParams, HoverResult, InitializeParams, InlayHint, InlayHintParams, Location, Position,
    Range, StyxLspExtensionClient, StyxLspHostDispatcher,
};

/// A document for testing, with optional cursor position.
#[derive(Debug, Clone)]
pub struct TestDocument {
    /// Document URI.
    pub uri: String,
    /// Source text (with cursor marker removed).
    pub source: String,
    /// Cursor position (from `|` marker).
    pub cursor: Option<CursorInfo>,
}

/// Cursor position information.
#[derive(Debug, Clone, Copy)]
pub struct CursorInfo {
    pub offset: usize,
    pub line: u32,
    pub character: u32,
}

impl TestDocument {
    /// Create a new test document from source.
    ///
    /// If the source contains `|`, it marks the cursor position and is removed.
    pub fn new(uri: impl Into<String>, source: impl Into<String>) -> Self {
        let uri = uri.into();
        let mut source = source.into();

        let cursor = if let Some(pos) = source.find('|') {
            source.remove(pos);

            let before = &source[..pos];
            let line = before.chars().filter(|&c| c == '\n').count() as u32;
            let last_nl = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
            let character = (pos - last_nl) as u32;

            Some(CursorInfo {
                offset: pos,
                line,
                character,
            })
        } else {
            None
        };

        Self {
            uri,
            source,
            cursor,
        }
    }
}

/// Cursor map type - tracks cursor positions for loaded documents.
type CursorMap = Arc<RwLock<HashMap<String, CursorInfo>>>;

/// Integration test harness for LSP extensions.
///
/// Spawns an extension binary and communicates via roam over stdio,
/// using the real `StyxLspHostImpl` from the LSP server.
pub struct TestHarness {
    /// The spawned child process.
    #[allow(dead_code)]
    process: Child,
    /// Extension client for making calls.
    client: StyxLspExtensionClient,
    /// Driver task.
    driver_handle: JoinHandle<()>,
    /// Documents loaded in the harness (for StyxLspHost callbacks).
    documents: DocumentMap,
    /// Cursor positions for loaded documents.
    cursors: CursorMap,
}

impl TestHarness {
    /// Spawn an extension and establish connection.
    ///
    /// `command` is the full command to run, e.g., `&["dibs", "lsp-extension"]`.
    pub async fn spawn(command: &[&str]) -> Result<Self, HarnessError> {
        if command.is_empty() {
            return Err(HarnessError::EmptyCommand);
        }

        let mut process = Command::new(command[0])
            .args(&command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| HarnessError::SpawnFailed(e.to_string()))?;

        let stdin = process.stdin.take().ok_or(HarnessError::NoStdin)?;
        let stdout = process.stdout.take().ok_or(HarnessError::NoStdout)?;

        let stdio = ChildStdio::new(stdin, stdout);
        let framed = CobsFramed::new(stdio);

        let documents: DocumentMap = Arc::new(RwLock::new(HashMap::new()));
        let cursors: CursorMap = Arc::new(RwLock::new(HashMap::new()));

        // Create host implementation - reuse the real one from extensions.rs
        let host = StyxLspHostImpl::new(documents.clone());
        let dispatcher = StyxLspHostDispatcher::new(host);

        // Initiate roam handshake (we're the initiator, like the real LSP)
        let (handle, _incoming, driver) =
            roam_session::initiate_framed(framed, HandshakeConfig::default(), dispatcher)
                .await
                .map_err(|e| HarnessError::HandshakeFailed(e.to_string()))?;

        let client = StyxLspExtensionClient::new(handle);

        // Spawn the driver
        let driver_handle = tokio::spawn(async move {
            let _ = driver.run().await;
        });

        Ok(Self {
            process,
            client,
            driver_handle,
            documents,
            cursors,
        })
    }

    /// Initialize the extension.
    pub async fn initialize(
        &self,
        document_uri: &str,
        schema_id: &str,
    ) -> Result<styx_lsp_ext::InitializeResult, HarnessError> {
        self.client
            .initialize(InitializeParams {
                styx_version: env!("CARGO_PKG_VERSION").to_string(),
                document_uri: document_uri.to_string(),
                schema_id: schema_id.to_string(),
            })
            .await
            .map_err(|e| HarnessError::CallFailed(e.to_string()))
    }

    /// Load a document into the harness.
    pub async fn load_document(&self, doc: TestDocument) {
        let uri = Url::parse(&doc.uri).expect("invalid URI");
        let parsed = parse(&doc.source);
        let tree = styx_tree::parse(&doc.source).ok();

        // Store cursor position
        if let Some(cursor) = doc.cursor {
            let mut cursors = self.cursors.write().await;
            cursors.insert(doc.uri.clone(), cursor);
        }

        let state = DocumentState {
            content: doc.source,
            parse: parsed,
            tree,
            version: 1,
        };

        let mut docs = self.documents.write().await;
        docs.insert(uri, state);
    }

    /// Get the cursor position for a document.
    async fn get_cursor(&self, document_uri: &str) -> Option<CursorInfo> {
        let cursors = self.cursors.read().await;
        cursors.get(document_uri).copied()
    }

    /// Get completions at the cursor position.
    pub async fn completions(
        &self,
        document_uri: &str,
    ) -> Result<Vec<CompletionItem>, HarnessError> {
        let uri = Url::parse(document_uri).map_err(|e| HarnessError::InvalidUri(e.to_string()))?;

        let cursor_info = self
            .get_cursor(document_uri)
            .await
            .ok_or_else(|| HarnessError::NoCursor(document_uri.to_string()))?;

        let docs = self.documents.read().await;
        let doc = docs
            .get(&uri)
            .ok_or_else(|| HarnessError::DocumentNotFound(document_uri.to_string()))?;

        let path = doc
            .tree
            .as_ref()
            .and_then(|t| find_path_at_offset(t, cursor_info.offset))
            .unwrap_or_default();

        let context = doc
            .tree
            .as_ref()
            .and_then(|t| find_context_at_offset(t, cursor_info.offset));

        let tagged_context = doc
            .tree
            .as_ref()
            .and_then(|t| find_tagged_context_at_offset(t, cursor_info.offset));

        let prefix = get_prefix(&doc.content, cursor_info.offset);

        drop(docs);

        self.client
            .completions(CompletionParams {
                document_uri: document_uri.to_string(),
                cursor: Cursor {
                    line: cursor_info.line,
                    character: cursor_info.character,
                    offset: cursor_info.offset as u32,
                },
                path,
                prefix,
                context,
                tagged_context,
            })
            .await
            .map_err(|e| HarnessError::CallFailed(e.to_string()))
    }

    /// Get hover info at the cursor position.
    pub async fn hover(&self, document_uri: &str) -> Result<Option<HoverResult>, HarnessError> {
        let uri = Url::parse(document_uri).map_err(|e| HarnessError::InvalidUri(e.to_string()))?;

        let cursor_info = self
            .get_cursor(document_uri)
            .await
            .ok_or_else(|| HarnessError::NoCursor(document_uri.to_string()))?;

        let docs = self.documents.read().await;
        let doc = docs
            .get(&uri)
            .ok_or_else(|| HarnessError::DocumentNotFound(document_uri.to_string()))?;

        let path = doc
            .tree
            .as_ref()
            .and_then(|t| find_path_at_offset(t, cursor_info.offset))
            .unwrap_or_default();

        let context = doc
            .tree
            .as_ref()
            .and_then(|t| find_context_at_offset(t, cursor_info.offset));

        let tagged_context = doc
            .tree
            .as_ref()
            .and_then(|t| find_tagged_context_at_offset(t, cursor_info.offset));

        drop(docs);

        self.client
            .hover(HoverParams {
                document_uri: document_uri.to_string(),
                cursor: Cursor {
                    line: cursor_info.line,
                    character: cursor_info.character,
                    offset: cursor_info.offset as u32,
                },
                path,
                context,
                tagged_context,
            })
            .await
            .map_err(|e| HarnessError::CallFailed(e.to_string()))
    }

    /// Get inlay hints for the document.
    pub async fn inlay_hints(&self, document_uri: &str) -> Result<Vec<InlayHint>, HarnessError> {
        let uri = Url::parse(document_uri).map_err(|e| HarnessError::InvalidUri(e.to_string()))?;

        let docs = self.documents.read().await;
        let doc = docs
            .get(&uri)
            .ok_or_else(|| HarnessError::DocumentNotFound(document_uri.to_string()))?;

        let line_count = doc.content.lines().count() as u32;
        let context = doc.tree.clone();

        drop(docs);

        self.client
            .inlay_hints(InlayHintParams {
                document_uri: document_uri.to_string(),
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: line_count,
                        character: 0,
                    },
                },
                context,
            })
            .await
            .map_err(|e| HarnessError::CallFailed(e.to_string()))
    }

    /// Get diagnostics for the document.
    pub async fn diagnostics(&self, document_uri: &str) -> Result<Vec<Diagnostic>, HarnessError> {
        let uri = Url::parse(document_uri).map_err(|e| HarnessError::InvalidUri(e.to_string()))?;

        let docs = self.documents.read().await;
        let doc = docs
            .get(&uri)
            .ok_or_else(|| HarnessError::DocumentNotFound(document_uri.to_string()))?;

        let tree = doc.tree.clone().unwrap_or(Value {
            tag: None,
            payload: None,
            span: None,
        });

        drop(docs);

        self.client
            .diagnostics(DiagnosticParams {
                document_uri: document_uri.to_string(),
                tree,
            })
            .await
            .map_err(|e| HarnessError::CallFailed(e.to_string()))
    }

    /// Get definition locations for the symbol at cursor.
    pub async fn definition(&self, document_uri: &str) -> Result<Vec<Location>, HarnessError> {
        let uri = Url::parse(document_uri).map_err(|e| HarnessError::InvalidUri(e.to_string()))?;

        let cursors = self.cursors.read().await;
        let cursor_info = cursors
            .get(document_uri)
            .copied()
            .ok_or_else(|| HarnessError::NoCursor(document_uri.to_string()))?;
        drop(cursors);

        let docs = self.documents.read().await;
        let doc = docs
            .get(&uri)
            .ok_or_else(|| HarnessError::DocumentNotFound(document_uri.to_string()))?;

        let path = doc
            .tree
            .as_ref()
            .and_then(|t| find_path_at_offset(t, cursor_info.offset))
            .unwrap_or_default();

        let context = doc
            .tree
            .as_ref()
            .and_then(|t| find_context_at_offset(t, cursor_info.offset));

        let tagged_context = doc
            .tree
            .as_ref()
            .and_then(|t| find_tagged_context_at_offset(t, cursor_info.offset));

        drop(docs);

        self.client
            .definition(DefinitionParams {
                document_uri: document_uri.to_string(),
                cursor: Cursor {
                    line: cursor_info.line,
                    character: cursor_info.character,
                    offset: cursor_info.offset as u32,
                },
                path,
                context,
                tagged_context,
            })
            .await
            .map_err(|e| HarnessError::CallFailed(e.to_string()))
    }

    /// Shutdown the extension gracefully.
    pub async fn shutdown(self) -> Result<(), HarnessError> {
        self.client
            .shutdown()
            .await
            .map_err(|e| HarnessError::CallFailed(e.to_string()))?;

        self.driver_handle.abort();
        Ok(())
    }
}

/// Errors from the test harness.
#[derive(Debug)]
pub enum HarnessError {
    EmptyCommand,
    SpawnFailed(String),
    NoStdin,
    NoStdout,
    HandshakeFailed(String),
    CallFailed(String),
    InvalidUri(String),
    DocumentNotFound(String),
    NoCursor(String),
}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyCommand => write!(f, "empty command"),
            Self::SpawnFailed(e) => write!(f, "spawn failed: {}", e),
            Self::NoStdin => write!(f, "no stdin"),
            Self::NoStdout => write!(f, "no stdout"),
            Self::HandshakeFailed(e) => write!(f, "handshake failed: {}", e),
            Self::CallFailed(e) => write!(f, "call failed: {}", e),
            Self::InvalidUri(e) => write!(f, "invalid URI: {}", e),
            Self::DocumentNotFound(uri) => write!(f, "document not found: {}", uri),
            Self::NoCursor(uri) => write!(f, "no cursor in document: {}", uri),
        }
    }
}

impl std::error::Error for HarnessError {}

// =============================================================================
// Helper functions
// =============================================================================

fn find_path_at_offset(tree: &Value, offset: usize) -> Option<Vec<String>> {
    fn recurse(value: &Value, offset: usize, path: &mut Vec<String>) -> bool {
        if let Some(span) = &value.span {
            let start = span.start as usize;
            let end = span.end as usize;
            if offset < start || offset > end {
                return false;
            }
        }

        if let Some(obj) = value.as_object() {
            for entry in &obj.entries {
                if let Some(key) = entry.key.as_str() {
                    // Check if cursor is in the value position (after key, potentially empty value)
                    let key_end = entry.key.span.as_ref().map(|s| s.end as usize).unwrap_or(0);
                    let value_has_content = entry.value.payload.is_some();

                    // If value is empty and cursor is right after the key (with some whitespace),
                    // consider we're in this entry's value position
                    if !value_has_content && offset > key_end && offset <= key_end + 10 {
                        // cursor is shortly after key with no value - we're in value position
                        path.push(key.to_string());
                        return true;
                    }

                    path.push(key.to_string());
                    if recurse(&entry.value, offset, path) {
                        return true;
                    }
                    path.pop();
                }
            }
        }

        if let Some(tag) = &value.tag
            && let Some(styx_tree::Payload::Object(obj)) = &value.payload
        {
            path.push(format!("@{}", tag.name));
            for entry in &obj.entries {
                if let Some(key) = entry.key.as_str() {
                    // Same logic for tagged objects
                    let key_end = entry.key.span.as_ref().map(|s| s.end as usize).unwrap_or(0);
                    let value_has_content = entry.value.payload.is_some();

                    if !value_has_content && offset > key_end && offset <= key_end + 10 {
                        path.push(key.to_string());
                        return true;
                    }

                    path.push(key.to_string());
                    if recurse(&entry.value, offset, path) {
                        return true;
                    }
                    path.pop();
                }
            }
            return true;
        }

        true
    }

    let mut path = Vec::new();
    if recurse(tree, offset, &mut path) {
        Some(path)
    } else {
        None
    }
}

fn find_context_at_offset(tree: &Value, offset: usize) -> Option<Value> {
    fn find_obj(value: &Value, offset: usize, parent: Option<&Value>) -> Option<Value> {
        // Check if this value's span contains the offset
        // If there's no span, assume we might be in it (root case)
        let in_span = value
            .span
            .as_ref()
            .map(|s| offset >= s.start as usize && offset <= s.end as usize)
            .unwrap_or(true); // Default to true for root without span

        // For tagged objects (like @query{...}), check if we're inside
        if let Some(styx_tree::Payload::Object(obj)) = &value.payload {
            // Check if cursor is in any child first
            for entry in &obj.entries {
                if let Some(nested) = find_obj(&entry.value, offset, Some(value)) {
                    return Some(nested);
                }
                // Also check if we're on the key or value
                if let Some(key_span) = &entry.key.span
                    && offset >= key_span.start as usize
                    && offset <= key_span.end as usize
                {
                    return Some(value.clone());
                }
                if let Some(val_span) = &entry.value.span
                    && offset >= val_span.start as usize
                    && offset <= val_span.end as usize
                {
                    return Some(value.clone());
                }
            }
            if in_span {
                return Some(value.clone());
            }
        }

        // For untagged objects
        if let Some(obj) = value.as_object() {
            for entry in &obj.entries {
                if let Some(nested) = find_obj(&entry.value, offset, Some(value)) {
                    return Some(nested);
                }
                // Check if we're on key or value
                if let Some(key_span) = &entry.key.span
                    && offset >= key_span.start as usize
                    && offset <= key_span.end as usize
                {
                    return Some(value.clone());
                }
                if let Some(val_span) = &entry.value.span
                    && offset >= val_span.start as usize
                    && offset <= val_span.end as usize
                {
                    return Some(value.clone());
                }
            }
            if in_span {
                return Some(value.clone());
            }
        }

        // For sequences
        if let Some(styx_tree::Payload::Sequence(seq)) = &value.payload {
            for item in &seq.items {
                if let Some(nested) = find_obj(item, offset, Some(value)) {
                    return Some(nested);
                }
            }
        }

        // If we're on a scalar value, return the parent object
        if in_span && parent.is_some() {
            return parent.cloned();
        }

        None
    }

    find_obj(tree, offset, None)
}

/// Find the closest enclosing tagged value containing the given offset.
fn find_tagged_context_at_offset(tree: &Value, offset: usize) -> Option<Value> {
    fn find_tagged(value: &Value, offset: usize, current_tagged: Option<&Value>) -> Option<Value> {
        // Check if this value's span contains the offset
        let in_span = value
            .span
            .as_ref()
            .map(|s| offset >= s.start as usize && offset <= s.end as usize)
            .unwrap_or(true); // Root might not have span

        if !in_span {
            return None;
        }

        // If this value is tagged, it becomes the new candidate
        let new_tagged = if value.tag.is_some() {
            Some(value)
        } else {
            current_tagged
        };

        // Check payload for nested values
        if let Some(styx_tree::Payload::Object(obj)) = &value.payload {
            for entry in &obj.entries {
                if let Some(result) = find_tagged(&entry.value, offset, new_tagged) {
                    return Some(result);
                }
            }
        }

        // Also check if the value itself is an object (for root objects without payload)
        if let Some(obj) = value.as_object() {
            for entry in &obj.entries {
                if let Some(result) = find_tagged(&entry.value, offset, new_tagged) {
                    return Some(result);
                }
            }
        }

        // Check sequences
        if let Some(styx_tree::Payload::Sequence(seq)) = &value.payload {
            for item in &seq.items {
                if let Some(result) = find_tagged(item, offset, new_tagged) {
                    return Some(result);
                }
            }
        }

        // If we're in this value's span, return the current tagged context
        new_tagged.cloned()
    }

    find_tagged(tree, offset, None)
}

fn get_prefix(source: &str, offset: usize) -> String {
    let before = &source[..offset];
    let start = before
        .rfind(|c: char| c.is_whitespace() || c == '{' || c == '(' || c == ',')
        .map(|i| i + 1)
        .unwrap_or(0);
    before[start..].to_string()
}

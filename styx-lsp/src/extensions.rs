//! LSP extension management.
//!
//! This module handles spawning and communicating with LSP extensions
//! that provide domain-specific intelligence (completions, hover, etc.).

/// Result of attempting to get or spawn an extension.
#[derive(Debug, Clone)]
pub enum ExtensionResult {
    /// Extension is already running or was successfully spawned.
    Running,
    /// Extension is not in the allowlist and needs user approval.
    NotAllowed {
        /// The command that needs to be allowed.
        command: String,
    },
    /// Extension failed to spawn for another reason.
    Failed,
}

use std::collections::HashMap;
use std::io;
use std::pin::Pin;
use std::process::Stdio;
use std::task::{Context, Poll};

use facet_styx::LspExtensionConfig;
use roam_session::{ConnectionHandle, HandshakeConfig};
use roam_stream::CobsFramed;
pub use styx_lsp_ext::StyxLspExtensionClient;
use styx_lsp_ext::{
    GetDocumentParams, GetSchemaParams, GetSourceParams, GetSubtreeParams, OffsetToPositionParams,
    PositionToOffsetParams, SchemaInfo, StyxLspHost, StyxLspHostDispatcher,
};
use styx_tree::Value;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tower_lsp::lsp_types::Url;
use tracing::{debug, info, warn};

use crate::schema_validation::resolve_schema;
use crate::server::DocumentMap;

/// A duplex stream combining a child process's stdin and stdout.
///
/// Write goes to stdin (child reads), read comes from stdout (child writes).
pub struct ChildStdio {
    stdin: ChildStdin,
    stdout: ChildStdout,
}

impl ChildStdio {
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self { stdin, stdout }
    }
}

impl AsyncRead for ChildStdio {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stdout).poll_read(cx, buf)
    }
}

impl AsyncWrite for ChildStdio {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stdin).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stdin).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stdin).poll_shutdown(cx)
    }
}

/// Manages LSP extension processes.
pub struct ExtensionManager {
    /// Spawned extensions, keyed by schema ID.
    extensions: RwLock<HashMap<String, Extension>>,
    /// Allowed extensions (from user config).
    allowlist: RwLock<Vec<String>>,
    /// Shared document state for host callbacks.
    documents: DocumentMap,
}

/// A spawned extension process with roam connection.
struct Extension {
    /// The child process.
    #[allow(dead_code)]
    process: Child,
    /// Extension config from the schema.
    #[allow(dead_code)]
    config: LspExtensionConfig,
    /// Roam connection handle for making calls.
    #[allow(dead_code)]
    handle: ConnectionHandle,
    /// Driver task handle.
    #[allow(dead_code)]
    driver_handle: JoinHandle<()>,
}

impl ExtensionManager {
    /// Create a new extension manager.
    pub(crate) fn new(documents: DocumentMap) -> Self {
        Self {
            extensions: RwLock::new(HashMap::new()),
            allowlist: RwLock::new(Vec::new()),
            documents,
        }
    }

    /// Check if an extension is allowed.
    pub async fn is_allowed(&self, command: &str) -> bool {
        let allowlist = self.allowlist.read().await;
        // For now, allow if the first component of the command is in the allowlist
        allowlist.iter().any(|allowed| command.starts_with(allowed))
    }

    /// Add a command to the allowlist.
    pub async fn allow(&self, command: String) {
        let mut allowlist = self.allowlist.write().await;
        if !allowlist.contains(&command) {
            allowlist.push(command);
        }
    }

    /// Get or spawn an extension for a schema.
    ///
    /// Returns the result of the operation, indicating whether the extension
    /// is running, not allowed, or failed to spawn.
    pub async fn get_or_spawn(
        &self,
        schema_id: &str,
        config: &LspExtensionConfig,
        document_uri: &str,
    ) -> ExtensionResult {
        // Check if already spawned
        {
            let extensions = self.extensions.read().await;
            if extensions.contains_key(schema_id) {
                return ExtensionResult::Running;
            }
        }

        // Check if allowed
        let Some(command) = config.launch.first() else {
            return ExtensionResult::Failed;
        };
        if !self.is_allowed(command).await {
            info!(schema_id, command, "Extension not in allowlist, skipping");
            return ExtensionResult::NotAllowed {
                command: command.clone(),
            };
        }

        // Spawn the extension
        let Some(extension) = self.spawn_extension(schema_id, config, document_uri).await else {
            return ExtensionResult::Failed;
        };

        // Store it
        let mut extensions = self.extensions.write().await;
        extensions.insert(schema_id.to_string(), extension);

        ExtensionResult::Running
    }

    /// Get the connection handle for a schema's extension.
    pub async fn get_handle(&self, schema_id: &str) -> Option<ConnectionHandle> {
        let extensions = self.extensions.read().await;
        extensions.get(schema_id).map(|ext| ext.handle.clone())
    }

    /// Get an extension client for a schema.
    ///
    /// Returns `None` if no extension is spawned for this schema.
    pub async fn get_client(&self, schema_id: &str) -> Option<StyxLspExtensionClient> {
        self.get_handle(schema_id)
            .await
            .map(StyxLspExtensionClient::new)
    }

    /// Spawn an extension process, establish roam connection, and initialize it.
    async fn spawn_extension(
        &self,
        schema_id: &str,
        config: &LspExtensionConfig,
        document_uri: &str,
    ) -> Option<Extension> {
        let launch = &config.launch;
        if launch.is_empty() {
            warn!("Empty launch command");
            return None;
        }

        let command = &launch[0];
        let args = &launch[1..];

        info!(command, ?args, "Spawning LSP extension");

        let mut process = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                warn!(command, error = %e, "Failed to spawn extension");
                e
            })
            .ok()?;

        // Take stdin/stdout for roam communication
        let stdin = process.stdin.take()?;
        let stdout = process.stdout.take()?;
        let stdio = ChildStdio::new(stdin, stdout);

        // Wrap in COBS framing for roam
        let framed = CobsFramed::new(stdio);

        // Initiate roam handshake (LSP is the initiator)
        let handshake_config = HandshakeConfig::default();

        // Create host dispatcher for extension callbacks
        let host_impl = StyxLspHostImpl {
            documents: self.documents.clone(),
        };
        let dispatcher = StyxLspHostDispatcher::new(host_impl);

        // Use initiate_framed for the initiating side
        let (handle, _incoming, driver) =
            roam_session::initiate_framed(framed, handshake_config, dispatcher)
                .await
                .map_err(|e| {
                    warn!(command, error = %e, "Failed roam handshake with extension");
                    e
                })
                .ok()?;

        debug!(command, "Roam session established with extension");

        // Spawn the driver
        let driver_handle = tokio::spawn(async move {
            if let Err(e) = driver.run().await {
                warn!(error = %e, "Extension driver error");
            }
        });

        // Initialize the extension
        let client = StyxLspExtensionClient::new(handle.clone());
        let init_result = client
            .initialize(styx_lsp_ext::InitializeParams {
                styx_version: env!("CARGO_PKG_VERSION").to_string(),
                document_uri: document_uri.to_string(),
                schema_id: schema_id.to_string(),
            })
            .await;

        match init_result {
            Ok(result) => {
                info!(
                    name = %result.name,
                    version = %result.version,
                    capabilities = ?result.capabilities,
                    "Extension initialized"
                );
            }
            Err(e) => {
                warn!(command, error = %e, "Failed to initialize extension");
                return None;
            }
        }

        Some(Extension {
            process,
            config: config.clone(),
            handle,
            driver_handle,
        })
    }

    /// Shutdown all extensions.
    pub async fn shutdown_all(&self) {
        let mut extensions = self.extensions.write().await;
        for (schema_id, mut ext) in extensions.drain() {
            debug!(schema_id, "Shutting down extension");
            // Abort the driver task
            ext.driver_handle.abort();
            // Kill the process
            let _ = ext.process.kill().await;
        }
    }
}

/// Extension-related information extracted from a schema.
#[derive(Debug, Clone)]
pub struct ExtensionInfo {
    /// The schema ID.
    pub schema_id: String,
    /// The extension config.
    pub config: LspExtensionConfig,
}

/// Extract extension info from a schema file if it has one.
pub fn get_extension_info(schema: &facet_styx::SchemaFile) -> Option<ExtensionInfo> {
    let lsp_config = schema.meta.lsp.as_ref()?;
    Some(ExtensionInfo {
        schema_id: schema.meta.id.clone(),
        config: lsp_config.clone(),
    })
}

// =============================================================================
// StyxLspHost implementation
// =============================================================================

/// Implementation of StyxLspHost for extension callbacks.
#[derive(Clone)]
pub struct StyxLspHostImpl {
    documents: DocumentMap,
}

impl StyxLspHostImpl {
    /// Create a new host implementation with the given document map.
    pub fn new(documents: DocumentMap) -> Self {
        Self { documents }
    }
}

impl StyxLspHost for StyxLspHostImpl {
    async fn get_subtree(&self, params: GetSubtreeParams) -> Option<Value> {
        let uri = Url::parse(&params.document_uri).ok()?;
        let docs = self.documents.read().await;
        let doc = docs.get(&uri)?;
        let tree = doc.tree.as_ref()?;

        // Navigate to the subtree at the given path
        let mut current = tree;
        for key in &params.path {
            let obj = current.as_object()?;
            let entry = obj
                .entries
                .iter()
                .find(|e| e.key.as_str() == Some(key.as_str()))?;
            current = &entry.value;
        }

        Some(current.clone())
    }

    async fn get_document(&self, params: GetDocumentParams) -> Option<Value> {
        let uri = Url::parse(&params.document_uri).ok()?;
        let docs = self.documents.read().await;
        let doc = docs.get(&uri)?;
        doc.tree.clone()
    }

    async fn get_source(&self, params: GetSourceParams) -> Option<String> {
        let uri = Url::parse(&params.document_uri).ok()?;
        let docs = self.documents.read().await;
        let doc = docs.get(&uri)?;
        Some(doc.content.clone())
    }

    async fn get_schema(&self, params: GetSchemaParams) -> Option<SchemaInfo> {
        let uri = Url::parse(&params.document_uri).ok()?;
        let docs = self.documents.read().await;
        let doc = docs.get(&uri)?;
        let tree = doc.tree.as_ref()?;

        let resolved = resolve_schema(tree, &uri).ok()?;
        Some(SchemaInfo {
            source: resolved.source,
            uri: resolved.uri.to_string(),
        })
    }

    async fn offset_to_position(
        &self,
        params: OffsetToPositionParams,
    ) -> Option<styx_lsp_ext::Position> {
        let uri = Url::parse(&params.document_uri).ok()?;
        let docs = self.documents.read().await;
        let doc = docs.get(&uri)?;

        let offset = params.offset as usize;
        if offset > doc.content.len() {
            return None;
        }

        let content = &doc.content[..offset];
        let line = content.chars().filter(|&c| c == '\n').count() as u32;
        let last_newline = content.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let character = (offset - last_newline) as u32;

        Some(styx_lsp_ext::Position { line, character })
    }

    async fn position_to_offset(&self, params: PositionToOffsetParams) -> Option<u32> {
        let uri = Url::parse(&params.document_uri).ok()?;
        let docs = self.documents.read().await;
        let doc = docs.get(&uri)?;

        let mut offset = 0;
        let mut current_line = 0;

        for ch in doc.content.chars() {
            if current_line == params.position.line {
                for (char_offset, c) in doc.content[offset..].chars().enumerate() {
                    if char_offset == params.position.character as usize {
                        return Some(offset as u32);
                    }
                    if c == '\n' {
                        break;
                    }
                    offset += c.len_utf8();
                }
                return Some(offset as u32);
            }
            if ch == '\n' {
                current_line += 1;
            }
            offset += ch.len_utf8();
        }

        None
    }
}

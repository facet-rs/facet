use std::collections::BTreeMap;

use facet::Facet;

pub const RUNNER_PROTOCOL_VERSION: RunnerProtocolVersion = RunnerProtocolVersion(23);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
pub struct Blake3Hash(pub [u8; 32]);

impl Blake3Hash {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    pub fn to_hex(&self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    pub fn short_hex(&self) -> String {
        self.0[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    pub fn from_hex(value: &str) -> Option<Self> {
        if value.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
            let hex = std::str::from_utf8(chunk).ok()?;
            bytes[index] = u8::from_str_radix(hex, 16).ok()?;
        }
        Some(Self(bytes))
    }
}

impl std::fmt::Display for Blake3Hash {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(transparent)]
pub struct ContentHash(pub Blake3Hash);

impl ContentHash {
    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }

    pub fn short_hex(&self) -> String {
        self.0.short_hex()
    }

    pub fn from_hex(value: &str) -> Option<Self> {
        Blake3Hash::from_hex(value).map(Self)
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(transparent)]
pub struct NodeHash(pub Blake3Hash);

impl NodeHash {
    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }

    pub fn short_hex(&self) -> String {
        self.0.short_hex()
    }

    pub fn from_hex(value: &str) -> Option<Self> {
        Blake3Hash::from_hex(value).map(Self)
    }
}

impl std::fmt::Display for NodeHash {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(transparent)]
pub struct RunnerProtocolVersion(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(transparent)]
pub struct ExecPath(pub String);

impl ExecPath {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ExecPath {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for ExecPath {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl std::borrow::Borrow<str> for ExecPath {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ExecPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(transparent)]
pub struct ExecProgram(pub String);

impl ExecProgram {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ExecProgram {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for ExecProgram {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for ExecProgram {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(transparent)]
pub struct ExecArgValue(pub String);

impl ExecArgValue {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ExecArgValue {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for ExecArgValue {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for ExecArgValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(transparent)]
pub struct ExecText(pub String);

impl ExecText {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ExecText {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for ExecText {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(transparent)]
pub struct ByteLen(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
pub struct ExecExitCode(pub i32);

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(transparent)]
pub struct ExecDiagnostic(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(transparent)]
pub struct PlatformName(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(transparent)]
pub struct PlatformArch(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct RunnerPlatform {
    pub os: PlatformName,
    pub arch: PlatformArch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum RunnerTransport {
    VoxWebsocket,
    VoxLocal,
    VoxTcp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum SandboxCapability {
    Seatbelt,
    Landlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ToolchainKind {
    Shell,
    CCompiler,
    Rustc,
    SandboxExec,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ToolchainCapability {
    pub kind: ToolchainKind,
    pub executable: ExecPath,
    pub content_hash: Option<ContentHash>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct RunnerCapabilities {
    pub protocol_version: RunnerProtocolVersion,
    pub platform: RunnerPlatform,
    pub transports: Vec<RunnerTransport>,
    pub sandboxes: Vec<SandboxCapability>,
    pub toolchains: Vec<ToolchainCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct CapabilityProbeRequest {}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct CapabilityProbeResult {
    pub capabilities: RunnerCapabilities,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Facet)]
pub struct ExecTree {
    pub entries: BTreeMap<ExecPath, ExecText>,
    pub blobs: BTreeMap<ExecPath, Vec<u8>>,
}

impl ExecTree {
    pub fn insert_bytes(&mut self, path: impl Into<String>, contents: Vec<u8>) {
        let path = ExecPath(path.into());
        match String::from_utf8(contents) {
            Ok(text) => {
                self.entries.insert(path, ExecText(text));
            }
            Err(err) => {
                self.blobs.insert(path, err.into_bytes());
            }
        }
    }

    pub fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.entries
            .get(path)
            .map(|contents| contents.0.as_bytes().to_vec())
            .or_else(|| self.blobs.get(path).cloned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ExecMount {
    pub at: ExecPath,
    pub tree: ExecTree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ExecRole {
    Executable,
    Input,
    InputFlag,
    Output,
    OutputFlag,
    OutputDir,
    Stdout,
    Env,
    SearchDir,
    SearchDirFlag,
    Flag,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ExecArg {
    pub value: ExecArgValue,
    pub role: ExecRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ExecPlan {
    pub argv: Vec<ExecArg>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ExecRequest {
    pub program: ExecProgram,
    pub plan: ExecPlan,
    pub mounts: Vec<ExecMount>,
    pub toolchain_roots: Vec<ExecPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ExecReadObservation {
    File {
        content_hash: ContentHash,
        blob_node: Option<NodeHash>,
        size: ByteLen,
    },
    Directory {
        directory_node: NodeHash,
    },
    LookupMiss {
        parent_path: ExecPath,
        directory_node: NodeHash,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(transparent)]
pub struct ExecPrefixId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ExecObservationScope {
    pub prefix_id: ExecPrefixId,
    pub roots: Vec<ExecPath>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Facet)]
pub struct ExecReadSet {
    pub entries: BTreeMap<ExecPath, ExecReadObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ExecSubfileCompletion {
    pub path: ExecPath,
    pub content_hash: ContentHash,
    pub size: ByteLen,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ExecTreeFinalization {
    pub files: Vec<ExecSubfileCompletion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ExecTreeEvent {
    SubfileCompleted(ExecSubfileCompletion),
    TreeFinalized(ExecTreeFinalization),
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ExecControlSignal {
    DemandsSatisfied(ExecDemandsSatisfied),
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ExecDemandsSatisfied {
    pub completed: Vec<ExecPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ExecOutcome {
    pub outputs: ExecTree,
    pub read_set: ExecReadSet,
    pub observation_scopes: Vec<ExecObservationScope>,
    pub tree_events: Vec<ExecTreeEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ExecErrorKind {
    Staging,
    Spawn,
    UnsupportedPlatform,
    ProcessExit,
    Harvest,
    Verification,
    Protocol,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ExecError {
    pub kind: ExecErrorKind,
    pub diagnostic: ExecDiagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ExecCompletion {
    Succeeded(ExecOutcome),
    Failed(ExecError),
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ExecResult {
    pub exit_code: ExecExitCode,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub completion: ExecCompletion,
}

#[vox::service]
#[allow(async_fn_in_trait)]
pub trait Runner {
    async fn exec(&self, request: ExecRequest) -> ExecResult;
    async fn capabilities(&self, request: CapabilityProbeRequest) -> CapabilityProbeResult;
}

#[cfg(test)]
mod tests {
    use super::{Blake3Hash, ContentHash, ExecTree, NodeHash};

    #[test]
    fn blake3_hash_hex_round_trips() {
        let hash = Blake3Hash::from_bytes(b"exec protocol");
        assert_eq!(Blake3Hash::from_hex(&hash.to_hex()), Some(hash));
        assert_eq!(hash.short_hex(), &hash.to_hex()[..16]);
    }

    #[test]
    fn typed_hashes_decode_from_hex() {
        let hash = Blake3Hash::from_bytes(b"typed");
        assert_eq!(
            ContentHash::from_hex(&hash.to_hex()),
            Some(ContentHash(hash))
        );
        assert_eq!(NodeHash::from_hex(&hash.to_hex()), Some(NodeHash(hash)));
        assert_eq!(Blake3Hash::from_hex("not hex"), None);
    }

    #[test]
    fn exec_tree_preserves_text_and_binary_bytes() {
        let mut tree = ExecTree::default();
        tree.insert_bytes("text.txt", b"hello".to_vec());
        tree.insert_bytes("binary.bin", vec![0xff, 0x00]);
        assert_eq!(tree.bytes("text.txt"), Some(b"hello".to_vec()));
        assert_eq!(tree.bytes("binary.bin"), Some(vec![0xff, 0x00]));
    }
}

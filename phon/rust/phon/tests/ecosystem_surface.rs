use std::collections::{BTreeMap, HashMap, HashSet};

use facet::Facet;
use facet_value::{VObject, VString, Value};
use phon::api::Codec;
use phon_engine::{CompactError, Registry, compact, plan};
use phon_schema::{ChannelDirection, Field, Primitive, Schema, SchemaId, SchemaKind, SchemaRef};

#[derive(Debug, Clone, PartialEq, Facet)]
struct CodeExecutionMetadata {
    language: String,
    dependencies: Vec<ResolvedDependency>,
    duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct ResolvedDependency {
    name: String,
    version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct Injection {
    location: InjectionLocation,
    content: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum InjectionLocation {
    Head,
    Body,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct ResponsiveImageInfo {
    jxl_srcset: Vec<(String, u32)>,
    webp_srcset: Vec<(String, u32)>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct MountLocalization {
    segment: String,
    routes: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaHtmlProcessInput {
    html: String,
    path_map: Option<HashMap<String, String>>,
    known_routes: Option<HashSet<String>>,
    code_metadata: Option<HashMap<String, CodeExecutionMetadata>>,
    injections: Vec<Injection>,
    image_variants: Option<HashMap<String, ResponsiveImageInfo>>,
    vite_css_map: Option<HashMap<String, Vec<String>>>,
    mount: Option<MountLocalization>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaTemplateCall {
    context_id: String,
    name: String,
    args: Vec<Value>,
    kwargs: Vec<(String, Value)>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaLoadDataResult {
    Success { value: Value },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaMarkdownHeading {
    title: String,
    id: String,
    level: u8,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaReqDefinition {
    id: String,
    anchor_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
enum DodecaSourceKind {
    Heading,
    Paragraph,
    BlockQuote,
    List,
    ListItem,
    DefinitionList,
    DefinitionListTitle,
    DefinitionListDefinition,
    ThematicBreak,
    Table,
    TableHead,
    TableRow,
    TableCell,
    Image,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaSourceMapEntry {
    id: String,
    kind: DodecaSourceKind,
    line_start: u32,
    line_end: u32,
    byte_start: u64,
    byte_end: u64,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaSourceMap {
    source_path: Option<String>,
    entries: Vec<DodecaSourceMapEntry>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaFrontmatter {
    title: String,
    weight: i32,
    description: Option<String>,
    template: Option<String>,
    extra: Value,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaParseResult {
    Success {
        frontmatter: DodecaFrontmatter,
        html: String,
        headings: Vec<DodecaMarkdownHeading>,
        reqs: Vec<DodecaReqDefinition>,
        head_injections: Vec<String>,
        source_map: Box<DodecaSourceMap>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaDecodedImage {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    channels: u8,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaImageResult {
    Success { image: DodecaDecodedImage },
    ThumbhashSuccess { data_url: String },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaResizeInput {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    channels: u8,
    target_width: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaThumbhashInput {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaImageProcessorFixture {
    png_data: Vec<u8>,
    decoded_result: DodecaImageResult,
    resize_input: DodecaResizeInput,
    resize_result: DodecaImageResult,
    thumbhash_input: DodecaThumbhashInput,
    thumbhash_result: DodecaImageResult,
    error_result: DodecaImageResult,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaSearchPage {
    url: String,
    source: String,
    html: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaSearchFile {
    path: String,
    contents: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaSearchIndexResult {
    Success { files: Vec<DodecaSearchFile> },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaSearchIndexerFixture {
    pages: Vec<DodecaSearchPage>,
    result: DodecaSearchIndexResult,
    error_result: DodecaSearchIndexResult,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaCssResult {
    Success { css: String },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaSassResult {
    Success { css: String },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaSvgoResult {
    Success { svg: String },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaAssetProcessingFixture {
    css_source: String,
    css_path_map: HashMap<String, String>,
    css_result: DodecaCssResult,
    sass_entrypoint: String,
    sass_files: HashMap<String, String>,
    sass_load_paths: Vec<String>,
    sass_result: DodecaSassResult,
    svg_source: String,
    svgo_result: DodecaSvgoResult,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaReadyMsg {
    peer_id: u16,
    cell_name: String,
    pid: Option<u32>,
    version: Option<String>,
    features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaReadyAck {
    ok: bool,
    host_time_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaMinifyResult {
    Success { content: String },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaJsRewriteInput {
    js: String,
    path_map: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaHtmlDiffInput {
    old_html: String,
    new_html: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaHtmlDiffOutcome {
    patches_blob: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaHtmlDiffError {
    Generic(String),
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaSubsetFontInput {
    data: Vec<u8>,
    chars: Vec<char>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaFontResult {
    DecompressSuccess { data: Vec<u8> },
    SubsetSuccess { data: Vec<u8> },
    CompressSuccess { data: Vec<u8> },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaWebpEncodeInput {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    quality: u8,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaWebpResult {
    DecodeSuccess {
        pixels: Vec<u8>,
        width: u32,
        height: u32,
        channels: u8,
    },
    EncodeSuccess {
        data: Vec<u8>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaJxlEncodeInput {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    quality: u8,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaJxlResult {
    DecodeSuccess {
        pixels: Vec<u8>,
        width: u32,
        height: u32,
        channels: u8,
    },
    EncodeSuccess {
        data: Vec<u8>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaSelectResult {
    Selected { index: usize },
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaConfirmResult {
    Yes,
    No,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaRecordConfig {
    shell: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaTermResult {
    Success { html: String },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaStartDevServerResult {
    Success { port: u16 },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaRunBuildResult {
    Success,
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct DodecaLinkDiagnostics {
    request_headers: Vec<(String, String)>,
    response_headers: Vec<(String, String)>,
    response_body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
enum DodecaLinkStatus {
    Ok,
    HttpError {
        code: u16,
        diagnostics: DodecaLinkDiagnostics,
    },
    Failed {
        message: String,
    },
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaLinkCheckInput {
    urls: Vec<String>,
    delay_ms: u64,
    timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaLinkCheckOutput {
    results: HashMap<String, DodecaLinkStatus>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaLinkCheckResult {
    Success { output: DodecaLinkCheckOutput },
    Error { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
enum DodecaTaskStatus {
    Pending,
    Running,
    Done,
    Error,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaTaskProgress {
    name: String,
    total: u32,
    completed: u32,
    status: DodecaTaskStatus,
    message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaBuildProgress {
    parse: DodecaTaskProgress,
    render: DodecaTaskProgress,
    sass: DodecaTaskProgress,
    links: DodecaTaskProgress,
    search: DodecaTaskProgress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
enum DodecaLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
enum DodecaEventKind {
    Http { status: u16 },
    FileChange,
    Reload,
    Patch,
    Search,
    Server,
    Build,
    Generic,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaLogEvent {
    level: DodecaLogLevel,
    kind: DodecaEventKind,
    message: String,
    fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
enum DodecaBindMode {
    Local,
    Lan,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaServerStatus {
    urls: Vec<String>,
    is_running: bool,
    bind_mode: DodecaBindMode,
    picante_cache_size: u64,
    cas_cache_size: u64,
    code_exec_cache_size: u64,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaServerCommand {
    GoPublic,
    GoLocal,
    TogglePicanteDebug,
    CycleLogLevel,
    SetLogFilter { filter: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DodecaCommandResult {
    Ok,
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DodecaSmallCellServicesFixture {
    ready_msg: DodecaReadyMsg,
    ready_ack: DodecaReadyAck,
    minify_result: DodecaMinifyResult,
    js_input: DodecaJsRewriteInput,
    js_result: Result<String, String>,
    html_diff_input: DodecaHtmlDiffInput,
    html_diff_result: Result<DodecaHtmlDiffOutcome, DodecaHtmlDiffError>,
    subset_font_input: DodecaSubsetFontInput,
    font_results: Vec<DodecaFontResult>,
    webp_encode_input: DodecaWebpEncodeInput,
    webp_results: Vec<DodecaWebpResult>,
    jxl_encode_input: DodecaJxlEncodeInput,
    jxl_results: Vec<DodecaJxlResult>,
    select_result: DodecaSelectResult,
    confirm_result: DodecaConfirmResult,
    record_config: DodecaRecordConfig,
    term_result: DodecaTermResult,
    start_dev_server_result: DodecaStartDevServerResult,
    run_build_result: DodecaRunBuildResult,
    link_check_input: DodecaLinkCheckInput,
    link_check_result: DodecaLinkCheckResult,
    build_progress: DodecaBuildProgress,
    log_event: DodecaLogEvent,
    server_status: DodecaServerStatus,
    server_command: DodecaServerCommand,
    command_result: DodecaCommandResult,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum SqlValue {
    Null = 0,
    Bool(bool) = 1,
    I16(i16) = 2,
    I32(i32) = 3,
    I64(i64) = 4,
    F32(f32) = 5,
    F64(f64) = 6,
    String(String) = 7,
    Bytes(Vec<u8>) = 8,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct RowField {
    name: String,
    value: SqlValue,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsListResponse {
    rows: Vec<Vec<RowField>>,
    total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsColumnInfo {
    name: String,
    sql_type: String,
    rust_type: Option<String>,
    nullable: bool,
    default: Option<String>,
    primary_key: bool,
    unique: bool,
    auto_generated: bool,
    long: bool,
    label: bool,
    enum_variants: Vec<String>,
    doc: Option<String>,
    lang: Option<String>,
    icon: Option<String>,
    subtype: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsForeignKeyInfo {
    columns: Vec<String>,
    references_table: String,
    references_columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsIndexColumnInfo {
    name: String,
    order: String,
    nulls: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsIndexInfo {
    name: String,
    columns: Vec<DibsIndexColumnInfo>,
    unique: bool,
    where_clause: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsTableInfo {
    name: String,
    columns: Vec<DibsColumnInfo>,
    foreign_keys: Vec<DibsForeignKeyInfo>,
    indices: Vec<DibsIndexInfo>,
    source_file: Option<String>,
    source_line: Option<u32>,
    doc: Option<String>,
    icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsSchemaInfo {
    tables: Vec<DibsTableInfo>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DibsFilterOp {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
    Like,
    ILike,
    IsNull,
    IsNotNull,
    In,
    JsonGet,
    JsonGetText,
    Contains,
    KeyExists,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsFilter {
    field: String,
    op: DibsFilterOp,
    value: SqlValue,
    values: Vec<SqlValue>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DibsSortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsSort {
    field: String,
    dir: DibsSortDir,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsListRequest {
    table: String,
    filters: Vec<DibsFilter>,
    sort: Vec<DibsSort>,
    limit: Option<u32>,
    offset: Option<u32>,
    select: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsRow {
    fields: Vec<RowField>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsServiceListResponse {
    rows: Vec<DibsRow>,
    total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsSqlError {
    message: String,
    sql: Option<String>,
    position: Option<u32>,
    hint: Option<String>,
    detail: Option<String>,
    caller: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DibsError {
    ConnectionFailed(String),
    MigrationFailed(DibsSqlError),
    InvalidRequest(String),
    UnknownTable(String),
    UnknownColumn(String),
    QueryError(String),
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsGetRequest {
    table: String,
    pk: SqlValue,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsCreateRequest {
    table: String,
    data: DibsRow,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsUpdateRequest {
    table: String,
    pk: SqlValue,
    data: DibsRow,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsDeleteRequest {
    table: String,
    pk: SqlValue,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DibsListResult {
    Ok(DibsServiceListResponse),
    Err(DibsError),
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DibsGetResult {
    Ok(Option<DibsRow>),
    Err(DibsError),
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DibsRowResult {
    Ok(DibsRow),
    Err(DibsError),
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DibsDeleteResult {
    Ok(u64),
    Err(DibsError),
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsSquelServiceFixture {
    schema: DibsSchemaInfo,
    list_request: DibsListRequest,
    list_response: DibsListResult,
    get_request: DibsGetRequest,
    get_response: DibsGetResult,
    create_request: DibsCreateRequest,
    create_response: DibsRowResult,
    update_request: DibsUpdateRequest,
    update_response: DibsRowResult,
    delete_request: DibsDeleteRequest,
    delete_response: DibsDeleteResult,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsMigrationInfo {
    version: String,
    name: String,
    applied: bool,
    applied_at: Option<String>,
    source_file: Option<String>,
    source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsMigrationStatusRequest {
    database_url: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsMigrateRequest {
    database_url: String,
    migration: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsAppliedMigration {
    version: String,
    applied_at: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsRanMigration {
    version: String,
    duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsMigrateResult {
    total_defined: u32,
    already_applied: Vec<DibsAppliedMigration>,
    applied: Vec<DibsRanMigration>,
    setup_ms: u64,
    total_time_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DibsMigrationStatusResult {
    Ok(Vec<DibsMigrationInfo>),
    Err(DibsError),
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum DibsMigrateCallResult {
    Ok(DibsMigrateResult),
    Err(DibsError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
enum DibsLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsMigrationLog {
    level: DibsLogLevel,
    message: String,
    migration: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct DibsMigrationServiceFixture {
    status_request: DibsMigrationStatusRequest,
    status_response: DibsMigrationStatusResult,
    migrate_request: DibsMigrateRequest,
    migrate_response: DibsMigrateCallResult,
    log_item: DibsMigrationLog,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxValue {
    tag: Option<StyxTag>,
    payload: Option<StyxPayload>,
    span: Option<StyxSpan>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxTag {
    name: String,
    span: Option<StyxSpan>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum StyxPayload {
    Scalar(StyxScalar),
    Sequence(StyxSequence),
    Object(StyxObject),
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxScalar {
    text: String,
    kind: StyxScalarKind,
    span: Option<StyxSpan>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum StyxScalarKind {
    Bare,
    Quoted,
    Raw,
    Heredoc,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxSequence {
    items: Vec<StyxValue>,
    span: Option<StyxSpan>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxEntry {
    key: StyxValue,
    value: StyxValue,
    doc_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxObject {
    entries: Vec<StyxEntry>,
    span: Option<StyxSpan>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxSpan {
    start: u32,
    end: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspPosition {
    line: u32,
    character: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspRange {
    start: StyxLspPosition,
    end: StyxLspPosition,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspCursor {
    line: u32,
    character: u32,
    offset: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum StyxLspCapability {
    Completions,
    Hover,
    Diagnostics,
    CodeActions,
    Definition,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspInitializeParams {
    styx_version: String,
    document_uri: String,
    schema_id: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspInitializeResult {
    name: String,
    version: String,
    capabilities: Vec<StyxLspCapability>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspCompletionParams {
    document_uri: String,
    cursor: StyxLspCursor,
    path: Vec<String>,
    prefix: String,
    context: Option<StyxValue>,
    tagged_context: Option<StyxValue>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum StyxLspCompletionKind {
    Field,
    Type,
    Function,
    Keyword,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspCompletionItem {
    label: String,
    detail: Option<String>,
    documentation: Option<String>,
    kind: Option<StyxLspCompletionKind>,
    sort_text: Option<String>,
    insert_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspHoverParams {
    document_uri: String,
    cursor: StyxLspCursor,
    path: Vec<String>,
    context: Option<StyxValue>,
    tagged_context: Option<StyxValue>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspHoverResult {
    contents: String,
    range: Option<StyxLspRange>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspInlayHintParams {
    document_uri: String,
    range: StyxLspRange,
    context: Option<StyxValue>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum StyxLspInlayHintKind {
    Type,
    Parameter,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspInlayHint {
    position: StyxLspPosition,
    label: String,
    kind: Option<StyxLspInlayHintKind>,
    padding_left: bool,
    padding_right: bool,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum StyxLspDiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspDiagnostic {
    span: StyxSpan,
    severity: StyxLspDiagnosticSeverity,
    message: String,
    source: Option<String>,
    code: Option<String>,
    data: Option<StyxValue>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspDiagnosticParams {
    document_uri: String,
    tree: StyxValue,
    content: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspCodeActionParams {
    document_uri: String,
    span: StyxSpan,
    diagnostics: Vec<StyxLspDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum StyxLspCodeActionKind {
    QuickFix,
    Refactor,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspWorkspaceEdit {
    changes: Vec<StyxLspDocumentEdit>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspDocumentEdit {
    uri: String,
    edits: Vec<StyxLspTextEdit>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspTextEdit {
    span: StyxSpan,
    new_text: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspCodeAction {
    title: String,
    kind: Option<StyxLspCodeActionKind>,
    edit: Option<StyxLspWorkspaceEdit>,
    is_preferred: bool,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspDefinitionParams {
    document_uri: String,
    cursor: StyxLspCursor,
    path: Vec<String>,
    context: Option<StyxValue>,
    tagged_context: Option<StyxValue>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspLocation {
    uri: String,
    span: StyxSpan,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspSchemaInfo {
    source: String,
    uri: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspGetSubtreeParams {
    document_uri: String,
    path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspGetDocumentParams {
    document_uri: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspGetSourceParams {
    document_uri: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspGetSchemaParams {
    document_uri: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspOffsetToPositionParams {
    document_uri: String,
    offset: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspPositionToOffsetParams {
    document_uri: String,
    position: StyxLspPosition,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StyxLspSurfaceFixture {
    initialize_params: StyxLspInitializeParams,
    initialize_result: StyxLspInitializeResult,
    completion_params: StyxLspCompletionParams,
    completions: Vec<StyxLspCompletionItem>,
    hover_params: StyxLspHoverParams,
    hover_result: Option<StyxLspHoverResult>,
    inlay_hint_params: StyxLspInlayHintParams,
    inlay_hints: Vec<StyxLspInlayHint>,
    diagnostic_params: StyxLspDiagnosticParams,
    diagnostics: Vec<StyxLspDiagnostic>,
    code_action_params: StyxLspCodeActionParams,
    code_actions: Vec<StyxLspCodeAction>,
    definition_params: StyxLspDefinitionParams,
    locations: Vec<StyxLspLocation>,
    get_subtree_params: StyxLspGetSubtreeParams,
    subtree: Option<StyxValue>,
    get_document_params: StyxLspGetDocumentParams,
    document: Option<StyxValue>,
    get_source_params: StyxLspGetSourceParams,
    source: Option<String>,
    get_schema_params: StyxLspGetSchemaParams,
    schema: Option<StyxLspSchemaInfo>,
    offset_to_position_params: StyxLspOffsetToPositionParams,
    position: Option<StyxLspPosition>,
    position_to_offset_params: StyxLspPositionToOffsetParams,
    offset: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct OffCpuBreakdown {
    sleep_ns: u64,
    io_ns: u64,
    mutex_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct FlameNode {
    address: u64,
    function_name: Option<u32>,
    binary: Option<u32>,
    on_cpu_ns: u64,
    off_cpu: OffCpuBreakdown,
    children: Vec<FlameNode>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct StaxFlamegraphUpdate {
    total_on_cpu_ns: u64,
    strings: Vec<String>,
    root: FlameNode,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct StaxLinuxPerfSessionConfig {
    target_pid: u32,
    frequency_hz: u32,
    kernel_stacks: bool,
    request_waking: bool,
    request_pmu: bool,
    request_dwarf_unwind: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
struct StaxLinuxWakingFieldOffsets {
    wakee_pid_offset: u32,
    wakee_pid_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
enum StaxLinuxPerfSessionError {
    NotPrivileged {
        detail: String,
    },
    PerfEventOpen {
        cpu: u32,
        errno: i32,
        detail: String,
    },
    NoSuchTarget(u32),
    NotAuthorized {
        caller_uid: u32,
        target_uid: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct StaxLinuxDaemonStatus {
    version: String,
    host_arch: String,
    privileged: bool,
    perf_event_paranoid: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct StaxLinuxBrokerControlFixture {
    config: StaxLinuxPerfSessionConfig,
    status: StaxLinuxDaemonStatus,
    errors: Vec<StaxLinuxPerfSessionError>,
    waking_field_offsets: Option<StaxLinuxWakingFieldOffsets>,
}

// Mirrors the macOS staxd surface from:
// /Users/amos/stax/staxd-proto/src/macos.rs
// /Users/amos/stax/stax-mac-kperf-sys/src/kdebug.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(C)]
struct StaxMacKdBuf {
    timestamp: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    debugid: u32,
    cpuid: u32,
    unused: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct StaxMacSessionConfig {
    target_pid: u32,
    frequency_hz: u32,
    buf_records: u32,
    samplers: u32,
    pmu_event_configs: Vec<u64>,
    class_mask: u32,
    filter_range_value1: u32,
    filter_range_value2: u32,
    typefilter_cscs: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct StaxMacKdBufBatch {
    records: Vec<StaxMacKdBuf>,
    read_started_mach_ticks: u64,
    drained_mach_ticks: u64,
    queued_for_send_mach_ticks: u64,
    send_started_mach_ticks: u64,
    drained_at_unix_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct StaxMacRecordSummary {
    records_drained: u64,
    session_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
enum StaxMacRecordError {
    NotRoot,
    NotAuthorized {
        caller_uid: u32,
        target_uid: u32,
    },
    Busy {
        holder_uid: u32,
        holder_pid: u32,
        since_unix_ns: u64,
    },
    NoSuchTarget(u32),
    Kperf {
        op: String,
        code: i32,
    },
    Sysctl {
        op: String,
        message: String,
    },
    Evicted,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
enum StaxMacRecordResult {
    Ok(StaxMacRecordSummary),
    Err(StaxMacRecordError),
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
enum StaxMacSessionState {
    Idle,
    Recording {
        target_pid: u32,
        holder_uid: u32,
        holder_pid: u32,
        since_unix_ns: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct StaxMacDaemonStatus {
    version: String,
    state: StaxMacSessionState,
    host_arch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct StaxMacRecordFixture {
    config: StaxMacSessionConfig,
    batch: StaxMacKdBufBatch,
    result: StaxMacRecordResult,
    status: StaxMacDaemonStatus,
}

// Mirrors representative payloads from:
// /Users/amos/helix/crates/helix-trace-server/src/service.rs
// /Users/amos/helix/crates/helix-trace-server/src/derived.rs
// /Users/amos/helix/crates/helix-trace-server/src/store.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
struct HelixSchedulerPulseId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
struct HelixAudioTokenId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
struct HelixTextTokenId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
struct HelixAudioRepresentationVersion(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
struct HelixLogicalPosition(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
struct HelixNativeEncoderWindowId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
struct HelixConvStemChunkId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
struct HelixAdmissionSegmentId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
struct HelixAudioTokenRange {
    start: HelixAudioTokenId,
    end: HelixAudioTokenId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
struct HelixMelFrameRange {
    start: u32,
    end: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
struct HelixAudioRepresentationSpan {
    audio: HelixAudioTokenRange,
    audio_representation_version: HelixAudioRepresentationVersion,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum HelixAudioTokenMergeProvenance {
    NoMerge {
        pre_merge_audio_token_id: HelixAudioTokenId,
    },
    Merged {
        pre_merge: HelixAudioTokenRange,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
enum HelixAudioTokenAdmissionProvenance {
    AdmitAll {
        admission_segment: HelixAdmissionSegmentId,
    },
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixAudioTokenProvenance {
    audio_token_id: HelixAudioTokenId,
    audio_representation_version: HelixAudioRepresentationVersion,
    mel_frames: Vec<HelixMelFrameRange>,
    native_window: HelixNativeEncoderWindowId,
    conv_stem_chunk: HelixConvStemChunkId,
    post_merge_audio_token_id: HelixAudioTokenId,
    merge: HelixAudioTokenMergeProvenance,
    admission: HelixAudioTokenAdmissionProvenance,
    cosine_to_previous: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixStreamMeta {
    schema_version: u32,
    pulse_ids: Vec<HelixSchedulerPulseId>,
    timeline_event_count: u64,
    attention_batch_count: u64,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixVerifyOutcome {
    rewind_k: u64,
    accepted_prefix_len: Option<u64>,
    divergence_row: Option<u64>,
    discarded_speculative_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixPulseRollup {
    pulse_id: HelixSchedulerPulseId,
    pulse_start_us: Option<u64>,
    pulse_duration_us: Option<u64>,
    encoder_duration_us: Option<u64>,
    refresh_duration_us: Option<u64>,
    verify_duration_us: Option<u64>,
    decode_duration_us: Option<u64>,
    commit_duration_us: Option<u64>,
    pulse_mel_frames: u64,
    committed_tokens: u64,
    retained_speculative_tokens: u64,
    resident_committed_tokens: u64,
    evicted_audio_tokens: u64,
    evicted_committed_tokens: u64,
    decoded_tokens: u64,
    hit_eos: bool,
    verify: Option<HelixVerifyOutcome>,
    has_attention_batch: bool,
    ar_token_count: u64,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixTextTokenSnapshot {
    text_token_id: HelixTextTokenId,
    text: Option<String>,
    text_before: Option<String>,
    in_verify_batch: bool,
    decoded_this_pulse: bool,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixPromptLayout {
    pulse_id: HelixSchedulerPulseId,
    first_audio_token_id: HelixAudioTokenId,
    resident_audio_frames: u64,
    changed_audio_spans: Vec<HelixAudioRepresentationSpan>,
    text_token_start: HelixTextTokenId,
    text_token_end: HelixTextTokenId,
    text_tokens: Vec<HelixTextTokenSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixTextAttendanceRow {
    text_token_id: HelixTextTokenId,
    decoder_layer_index: u32,
    head_index: u32,
    dominant_audio_mass: f32,
    total_audio_mass: f32,
    observed_audio: HelixAudioTokenRange,
    dominant_audio: HelixAudioTokenRange,
    audio_weights: Vec<f32>,
    queried_audio_weight: f32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixAudioAttendanceRow {
    decoder_layer_index: u32,
    head_index: u32,
    dominant_audio_mass: f32,
    total_audio_mass: f32,
    center_audio_token: Option<f32>,
    width_audio_tokens: Option<f32>,
    observed_audio: HelixAudioTokenRange,
    dominant_audio: HelixAudioTokenRange,
    audio_weights: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixRefreshAttendanceRow {
    query_position: HelixLogicalPosition,
    decoder_layer_index: u32,
    head_index: u32,
    dominant_audio_mass: f32,
    total_audio_mass: f32,
    center_audio_token: Option<f32>,
    width_audio_tokens: Option<f32>,
    observed_audio: HelixAudioTokenRange,
    dominant_audio: HelixAudioTokenRange,
    audio_weights: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixAudioSelfAttentionRow {
    encoder_layer_index: u32,
    head_index: u32,
    audio_representation_version: HelixAudioRepresentationVersion,
    dominant_audio_mass: f32,
    total_audio_mass: f32,
    center_audio_token: Option<f32>,
    width_audio_tokens: Option<f32>,
    observed_audio: HelixAudioTokenRange,
    dominant_audio: HelixAudioTokenRange,
    frontier_debt: f32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixTranscriptToken {
    text_token_id: HelixTextTokenId,
    decoded_in_pulse: HelixSchedulerPulseId,
    text: String,
    committed: bool,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixAudioClip {
    sample_rate: u32,
    first_sample: u64,
    samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixMelClip {
    num_mel_bins: u32,
    first_mel_frame: u32,
    num_mel_frames: u32,
    values: Vec<f32>,
    min_value: f32,
    max_value: f32,
    corpus_min_value: f32,
    corpus_max_value: f32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixPulseAttentionHeatmap {
    pulse_id: HelixSchedulerPulseId,
    first_audio_token_id: HelixAudioTokenId,
    audio_token_count: u32,
    text_token_start: HelixTextTokenId,
    text_token_count: u32,
    record_count: u32,
    max_value: f32,
    mean_audio_mass: Vec<f32>,
    text_token_glyphs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixStreamMetrics {
    pulse_ids: Vec<HelixSchedulerPulseId>,
    pulse_duration_us: Vec<u64>,
    decoded_tokens: Vec<u64>,
    committed_tokens: Vec<u64>,
    retained_speculative_tokens: Vec<u64>,
    evicted_audio_tokens: Vec<u64>,
    evicted_committed_tokens: Vec<u64>,
    rewind_k: Vec<u64>,
    ar_token_count: Vec<u64>,
    rolling_wer: Vec<f64>,
    s2d_p50_ms: Vec<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
struct HelixPulseAvailable {
    pulse_id: HelixSchedulerPulseId,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixAttentionSupportSummary {
    total_audio_mass: f32,
    observed_audio: HelixAudioTokenRange,
    dominant_audio: HelixAudioTokenRange,
    dominant_audio_mass: f32,
    center_audio_token: Option<f32>,
    width_audio_tokens: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixTextAttentionSupportRecord {
    text_token_id: HelixTextTokenId,
    query_position: HelixLogicalPosition,
    decoder_layer_index: u32,
    head_index: u32,
    support: HelixAttentionSupportSummary,
    audio_weights: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixAudioEncoderSupportRecord {
    audio_token_id: HelixAudioTokenId,
    audio_representation_version: HelixAudioRepresentationVersion,
    encoder_layer_index: u32,
    head_index: u32,
    support: HelixAttentionSupportSummary,
    frontier_debt: f32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum HelixDecoderEvidenceKind {
    Decode {
        input_token_id: u32,
    },
    VerifyPrediction {
        verified_draft_index: u32,
        draft_token_id: u32,
        query_row: u32,
        max_logit: f32,
        draft_logit: f32,
    },
    VerifySeed {
        query_row: u32,
        next_token_seed: u32,
        max_logit: f32,
    },
    PromptPrefill,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixDecoderEvidenceRecord {
    text_token_id: Option<HelixTextTokenId>,
    query_position: HelixLogicalPosition,
    expected_observed_audio: HelixAudioTokenRange,
    records: Vec<HelixTextAttentionSupportRecord>,
    kind: HelixDecoderEvidenceKind,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixQueryRowAttentionRecord {
    query_position: HelixLogicalPosition,
    decoder_layer_index: u32,
    head_index: u32,
    support: HelixAttentionSupportSummary,
    audio_weights: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixAttentionSummaryBatch {
    schema_version: u32,
    pulse_id: HelixSchedulerPulseId,
    audio_context_id: u64,
    text_context_id: u64,
    audio_representation_spans: Vec<HelixAudioRepresentationSpan>,
    changed_audio_representation_spans: Vec<HelixAudioRepresentationSpan>,
    text_support: Vec<HelixTextAttentionSupportRecord>,
    header_text_support: Vec<HelixQueryRowAttentionRecord>,
    audio_encoder_support: Vec<HelixAudioEncoderSupportRecord>,
    decoder_evidence: Vec<HelixDecoderEvidenceRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
enum HelixVerifyDraftStatus {
    Accepted,
    Divergent,
    DiscardedAfterDivergence,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixVerifyDraftRow {
    draft_index: u32,
    draft_token_id: u32,
    verified_text_token_id: HelixTextTokenId,
    text: String,
    status: HelixVerifyDraftStatus,
    expected_observed_audio: HelixAudioTokenRange,
    max_dominant_audio_mass: f32,
    record_count: u32,
    max_logit: f32,
    draft_logit: f32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixVerifySeedRow {
    query_row: u32,
    next_token_seed: u32,
    expected_observed_audio: HelixAudioTokenRange,
    max_dominant_audio_mass: f32,
    record_count: u32,
    max_logit: f32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixVerifyEvidenceDigest {
    pulse_id: HelixSchedulerPulseId,
    rewind_k: u64,
    accepted_prefix_len: Option<u64>,
    divergence_row: Option<u64>,
    drafts: Vec<HelixVerifyDraftRow>,
    seed: Option<HelixVerifySeedRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
struct HelixDecodeFact {
    text_token_id: HelixTextTokenId,
    query_position: HelixLogicalPosition,
    input_token_id: u32,
    observed_audio: HelixAudioTokenRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
struct HelixVerifyPredictionFact {
    verified_text_token_id: HelixTextTokenId,
    verified_draft_index: u32,
    draft_token_id: u32,
    query_row: u32,
    query_position: HelixLogicalPosition,
    observed_audio: HelixAudioTokenRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
struct HelixVerifySeedFact {
    query_row: u32,
    query_position: HelixLogicalPosition,
    next_token_seed: u32,
    observed_audio: HelixAudioTokenRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
struct HelixPromptPrefillFact {
    query_position: HelixLogicalPosition,
    observed_audio: HelixAudioTokenRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
struct HelixDecoderEvidenceFactCounts {
    decode: u32,
    verify_prediction: u32,
    verify_seed: u32,
    prompt_prefill: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixEncoderFactsSnapshot {
    refreshed_audio: HelixAudioTokenRange,
    audio_representation_version: HelixAudioRepresentationVersion,
    provenance: Vec<HelixAudioTokenProvenance>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixPulseEvidenceSnapshot {
    pulse_id: HelixSchedulerPulseId,
    encoder: Option<HelixEncoderFactsSnapshot>,
    counts: HelixDecoderEvidenceFactCounts,
    decode: Vec<HelixDecodeFact>,
    verify_prediction: Vec<HelixVerifyPredictionFact>,
    verify_seed: Vec<HelixVerifySeedFact>,
    prompt_prefill: Vec<HelixPromptPrefillFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
enum HelixEncoderProvenanceViolationKind {
    MissingProvenance,
    VersionMismatch,
    EmptyMelFrames,
    NonFiniteFrontierDebt,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixEncoderProvenanceViolation {
    audio_token_id: HelixAudioTokenId,
    encoder_layer_index: u32,
    head_index: u32,
    observed_audio_token_id: Option<HelixAudioTokenId>,
    kind: HelixEncoderProvenanceViolationKind,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixEncoderProvenanceReport {
    pulse_id: HelixSchedulerPulseId,
    records_checked: u64,
    violations: Vec<HelixEncoderProvenanceViolation>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixDecoderEvidenceVariantCounts {
    decode: u64,
    verify_prediction: u64,
    verify_seed: u64,
    prompt_prefill: u64,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixDecoderEvidenceReport {
    total_batches: u64,
    batches_without_decoder_evidence: u64,
    pulses_without_decoder_evidence: Vec<HelixSchedulerPulseId>,
    variant_evidence_counts: HelixDecoderEvidenceVariantCounts,
    variant_record_counts: HelixDecoderEvidenceVariantCounts,
    observed_decoder_layer_indices: Vec<u32>,
    observed_decoder_head_indices: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixEncoderFrontierPoint {
    audio_token_id: HelixAudioTokenId,
    mean_frontier_debt: f32,
    head_count: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixEncoderFrontierLayer {
    encoder_layer_index: u32,
    points: Vec<HelixEncoderFrontierPoint>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixEncoderFrontierSeries {
    pulse_id: HelixSchedulerPulseId,
    layers: Vec<HelixEncoderFrontierLayer>,
    min_audio_token_id: HelixAudioTokenId,
    max_audio_token_id: HelixAudioTokenId,
    min_frontier_debt: f32,
    max_frontier_debt: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
struct HelixTracePositionSpan {
    logical_start: u64,
    rows: u64,
    physical_start: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
enum HelixArDecodeEarlyExitReason {
    BudgetExhausted,
    NoBudget,
    SeedWasEos,
    ProducedEos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
enum HelixVerifySkippedReason {
    RewindGuardFailed,
    PreCommitFullRewind,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum HelixStreamingTraceEvent {
    Pulse {
        start_us: u64,
        duration_us: u64,
        pulse_id: u64,
        previous_consumed_mel_frames: u64,
        consumed_mel_frames: u64,
        pulse_mel_frames: u64,
        committed_text_len_start: u64,
        speculative_len_start: u64,
        committed_tokens: u64,
        retained_speculative_tokens: u64,
        resident_committed_tokens: u64,
        evicted_audio_tokens: u64,
        evicted_committed_tokens: u64,
    },
    RefreshPrompt {
        start_us: u64,
        duration_us: u64,
        pulse_id: u64,
        first_audio_token_id: u64,
        resident_audio_frames: u64,
        committed_text_len: u64,
        resident_committed_len: u64,
        resident_text_len: u64,
        logical_start: u64,
        logical_end: u64,
        text_token_start: u64,
        text_token_end: u64,
        spans: Vec<HelixTracePositionSpan>,
    },
    Verify {
        start_us: u64,
        duration_us: u64,
        pulse_id: u64,
        rewind_k: u64,
        post_rewind_text_len: u64,
        text_token_start: u64,
        text_token_end: u64,
        logical_start: u64,
        logical_end: u64,
        spans: Vec<HelixTracePositionSpan>,
        accepted_prefix_len: Option<u64>,
        divergence_row: Option<u64>,
        next_token_seed: Option<u64>,
        discarded_speculative_tokens: Option<u64>,
        invalidated_speculative_slots: Option<u64>,
    },
    ArDecode {
        start_us: u64,
        duration_us: u64,
        pulse_id: u64,
        decode_steps: u64,
        decoded_tokens: u64,
        speculative_len_entering: u64,
        live_speculative_tokens: u64,
        hit_eos: bool,
        seed_token_id: u64,
        seed_token_text: String,
        early_exit_reason: HelixArDecodeEarlyExitReason,
        next_after_tail: u64,
    },
    ArToken {
        start_us: u64,
        duration_us: u64,
        pulse_id: u64,
        step_index: u64,
        input_token_id: u64,
        input_text: String,
        text_token_id: u64,
        query_position: u64,
        physical_start: u64,
        summary_records: u64,
        next_token_id: u64,
        next_text: String,
    },
    Commit {
        start_us: u64,
        duration_us: u64,
        pulse_id: u64,
        speculative_len_pre: u64,
        revisable_tail_target: u64,
        committed_tokens: u64,
        retained_speculative_tokens: u64,
        committed_text_len: u64,
        next_after_committed: u64,
    },
    VerifySkipped {
        timestamp_us: u64,
        pulse_id: u64,
        reason: HelixVerifySkippedReason,
        rewind_k: u64,
        resident_committed_len: u64,
        speculative_len: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixChromeTraceEvent {
    name: String,
    cat: String,
    ph: String,
    ts: f64,
    dur: Option<f64>,
    pid: u32,
    tid: u32,
    s: Option<String>,
    args: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixPulseBundleFields {
    prompt_layout: bool,
    audio_provenance: bool,
    attention_heatmap: bool,
    encoder_frontier: bool,
    encoder_provenance: bool,
    audio_clip: bool,
    mel_clip: bool,
    pulse_rollup: bool,
    timeline: bool,
    gpu_chrome_events: bool,
    verify_evidence: bool,
    scheduler_snapshot: bool,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixPulseBundle {
    pulse_id: HelixSchedulerPulseId,
    schema_version: u32,
    prompt_layout: Option<HelixPromptLayout>,
    audio_provenance: Option<Vec<HelixAudioTokenProvenance>>,
    attention_heatmap: Option<HelixPulseAttentionHeatmap>,
    encoder_frontier: Option<HelixEncoderFrontierSeries>,
    encoder_provenance: Option<HelixEncoderProvenanceReport>,
    audio_clip: Option<HelixAudioClip>,
    mel_clip: Option<HelixMelClip>,
    pulse_rollup: Option<HelixPulseRollup>,
    timeline: Option<Vec<HelixStreamingTraceEvent>>,
    gpu_chrome_events: Option<Vec<HelixChromeTraceEvent>>,
    verify_evidence: Option<HelixVerifyEvidenceDigest>,
    scheduler_snapshot: Option<HelixPulseEvidenceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixPieceEvalSnapshot {
    audio_now_ms: f64,
    reference_words_available: u32,
    hypothesis_words: u32,
    substitutions: u32,
    deletions: u32,
    insertions: u32,
    rolling_wer: f64,
    s2d_matched_words: u32,
    s2d_new_words: u32,
    s2d_p50_ms: Option<f64>,
    s2d_p90_ms: Option<f64>,
    s2d_p100_ms: Option<f64>,
    s2d_avg_ms: Option<f64>,
    audio_frontier: u32,
    displayed_frontier: u32,
    committed_frontier: u32,
    lag_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixPieceEvalReference {
    piece: String,
    language: String,
    words: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixTraceServiceSurface {
    meta: HelixStreamMeta,
    pulse_rollup: Option<HelixPulseRollup>,
    timeline: Vec<HelixStreamingTraceEvent>,
    attention_batch: Option<HelixAttentionSummaryBatch>,
    prompt_layout: Option<HelixPromptLayout>,
    audio_attended_by: Vec<HelixTextAttendanceRow>,
    text_attends_to: Vec<HelixAudioAttendanceRow>,
    refresh_attends_to: Vec<HelixRefreshAttendanceRow>,
    audio_token_provenance: Option<HelixAudioTokenProvenance>,
    audio_provenance_for_pulse: Vec<HelixAudioTokenProvenance>,
    audio_tokens_for_mel_frame: Vec<HelixAudioTokenId>,
    audio_clip_for_audio_token: Option<HelixAudioClip>,
    audio_clip_for_prompt: Option<HelixAudioClip>,
    audio_clip_for_audio_range: Option<HelixAudioClip>,
    mel_clip_for_prompt: Option<HelixMelClip>,
    audio_self_attention: Vec<HelixAudioSelfAttentionRow>,
    transcript: Vec<HelixTranscriptToken>,
    pulse_attention_heatmap: Option<HelixPulseAttentionHeatmap>,
    encoder_frontier: Option<HelixEncoderFrontierSeries>,
    stream_metrics: HelixStreamMetrics,
    verify_evidence: Option<HelixVerifyEvidenceDigest>,
    decoder_evidence_report: HelixDecoderEvidenceReport,
    pulse_evidence_snapshot: Option<HelixPulseEvidenceSnapshot>,
    gpu_chrome_events_for_pulse: Vec<HelixChromeTraceEvent>,
    run_info: Option<HelixRunInfo>,
    piece_eval_reference: Option<HelixPieceEvalReference>,
    piece_eval_for_pulse: Option<HelixPieceEvalSnapshot>,
    encoder_provenance_report: Option<HelixEncoderProvenanceReport>,
    pulse_bundle_fields: HelixPulseBundleFields,
    pulse_bundle: HelixPulseBundle,
    pulse_available: HelixPulseAvailable,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixTraceSnapshot {
    meta: HelixStreamMeta,
    run_info: HelixRunInfo,
    rollup: Option<HelixPulseRollup>,
    prompt_layout: Option<HelixPromptLayout>,
    attention_heatmap: Option<HelixPulseAttentionHeatmap>,
    stream_metrics: HelixStreamMetrics,
    pulse_available: HelixPulseAvailable,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HelixRunInfo {
    backend: String,
    model_dir: String,
    input: String,
    piece: Option<String>,
    pulse_ms: u32,
    audio_ring_capacity: u32,
    text_ring_capacity: u32,
    commit_revisable_tail_text_tokens: u32,
    revise_logit_margin: f32,
    sample_rate: u32,
    mel_hop_samples: u32,
    num_mel_bins: u32,
    num_mel_frames: u32,
    audio_tokens_per_chunk: u32,
    native_window_tokens: u32,
    realtime_pacing: bool,
    profile_phases: bool,
    attention_trace_schema_version: u32,
    trace_server_schema_version: u32,
}

fn helix_audio_range(start: u32, end: u32) -> HelixAudioTokenRange {
    HelixAudioTokenRange {
        start: HelixAudioTokenId(start),
        end: HelixAudioTokenId(end),
    }
}

fn sample_helix_support() -> HelixAttentionSupportSummary {
    HelixAttentionSupportSummary {
        total_audio_mass: 0.42,
        observed_audio: helix_audio_range(32, 40),
        dominant_audio: helix_audio_range(34, 36),
        dominant_audio_mass: 0.21,
        center_audio_token: Some(35.25),
        width_audio_tokens: Some(3.5),
    }
}

fn sample_helix_text_support() -> Vec<HelixTextAttentionSupportRecord> {
    vec![HelixTextAttentionSupportRecord {
        text_token_id: HelixTextTokenId(91),
        query_position: HelixLogicalPosition(118),
        decoder_layer_index: 7,
        head_index: 3,
        support: sample_helix_support(),
        audio_weights: vec![0.05, 0.07, 0.10, 0.20],
    }]
}

fn sample_helix_audio_provenance() -> Vec<HelixAudioTokenProvenance> {
    vec![
        HelixAudioTokenProvenance {
            audio_token_id: HelixAudioTokenId(34),
            audio_representation_version: HelixAudioRepresentationVersion(7),
            mel_frames: vec![HelixMelFrameRange {
                start: 128,
                end: 136,
            }],
            native_window: HelixNativeEncoderWindowId(2),
            conv_stem_chunk: HelixConvStemChunkId(4),
            post_merge_audio_token_id: HelixAudioTokenId(34),
            merge: HelixAudioTokenMergeProvenance::NoMerge {
                pre_merge_audio_token_id: HelixAudioTokenId(34),
            },
            admission: HelixAudioTokenAdmissionProvenance::AdmitAll {
                admission_segment: HelixAdmissionSegmentId(12),
            },
            cosine_to_previous: Some(0.98),
        },
        HelixAudioTokenProvenance {
            audio_token_id: HelixAudioTokenId(35),
            audio_representation_version: HelixAudioRepresentationVersion(7),
            mel_frames: vec![
                HelixMelFrameRange {
                    start: 136,
                    end: 144,
                },
                HelixMelFrameRange {
                    start: 144,
                    end: 152,
                },
            ],
            native_window: HelixNativeEncoderWindowId(2),
            conv_stem_chunk: HelixConvStemChunkId(4),
            post_merge_audio_token_id: HelixAudioTokenId(35),
            merge: HelixAudioTokenMergeProvenance::Merged {
                pre_merge: helix_audio_range(35, 37),
            },
            admission: HelixAudioTokenAdmissionProvenance::AdmitAll {
                admission_segment: HelixAdmissionSegmentId(13),
            },
            cosine_to_previous: None,
        },
    ]
}

fn sample_helix_verify_evidence() -> HelixVerifyEvidenceDigest {
    HelixVerifyEvidenceDigest {
        pulse_id: HelixSchedulerPulseId(17),
        rewind_k: 2,
        accepted_prefix_len: Some(1),
        divergence_row: Some(1),
        drafts: vec![
            HelixVerifyDraftRow {
                draft_index: 0,
                draft_token_id: 1201,
                verified_text_token_id: HelixTextTokenId(91),
                text: "pho".to_string(),
                status: HelixVerifyDraftStatus::Accepted,
                expected_observed_audio: helix_audio_range(32, 36),
                max_dominant_audio_mass: 0.45,
                record_count: 16,
                max_logit: 13.5,
                draft_logit: 13.1,
            },
            HelixVerifyDraftRow {
                draft_index: 1,
                draft_token_id: 1202,
                verified_text_token_id: HelixTextTokenId(92),
                text: "n".to_string(),
                status: HelixVerifyDraftStatus::Divergent,
                expected_observed_audio: helix_audio_range(36, 40),
                max_dominant_audio_mass: 0.35,
                record_count: 16,
                max_logit: 11.5,
                draft_logit: 9.25,
            },
        ],
        seed: Some(HelixVerifySeedRow {
            query_row: 2,
            next_token_seed: 1401,
            expected_observed_audio: helix_audio_range(40, 48),
            max_dominant_audio_mass: 0.27,
            record_count: 8,
            max_logit: 10.75,
        }),
    }
}

fn sample_helix_audio_clip() -> HelixAudioClip {
    HelixAudioClip {
        sample_rate: 16_000,
        first_sample: 262_144,
        samples: vec![-0.25, -0.10, 0.0, 0.10, 0.25, 0.50, 0.25, 0.0],
    }
}

fn sample_helix_mel_clip() -> HelixMelClip {
    HelixMelClip {
        num_mel_bins: 4,
        first_mel_frame: 128,
        num_mel_frames: 3,
        values: vec![
            0.10, 0.20, 0.30, 0.40, 0.15, 0.25, 0.35, 0.45, 0.05, 0.12, 0.18, 0.22,
        ],
        min_value: 0.05,
        max_value: 0.45,
        corpus_min_value: -1.25,
        corpus_max_value: 2.75,
    }
}

fn sample_helix_encoder_frontier() -> HelixEncoderFrontierSeries {
    HelixEncoderFrontierSeries {
        pulse_id: HelixSchedulerPulseId(17),
        layers: vec![HelixEncoderFrontierLayer {
            encoder_layer_index: 3,
            points: vec![
                HelixEncoderFrontierPoint {
                    audio_token_id: HelixAudioTokenId(34),
                    mean_frontier_debt: 0.125,
                    head_count: 4,
                },
                HelixEncoderFrontierPoint {
                    audio_token_id: HelixAudioTokenId(35),
                    mean_frontier_debt: 0.25,
                    head_count: 4,
                },
            ],
        }],
        min_audio_token_id: HelixAudioTokenId(34),
        max_audio_token_id: HelixAudioTokenId(35),
        min_frontier_debt: 0.125,
        max_frontier_debt: 0.25,
    }
}

fn sample_helix_encoder_provenance_report() -> HelixEncoderProvenanceReport {
    HelixEncoderProvenanceReport {
        pulse_id: HelixSchedulerPulseId(17),
        records_checked: 32,
        violations: vec![HelixEncoderProvenanceViolation {
            audio_token_id: HelixAudioTokenId(36),
            encoder_layer_index: 2,
            head_index: 3,
            observed_audio_token_id: Some(HelixAudioTokenId(37)),
            kind: HelixEncoderProvenanceViolationKind::VersionMismatch,
            message: "observed audio provenance version lagged refresh".to_string(),
        }],
    }
}

fn sample_helix_timeline() -> Vec<HelixStreamingTraceEvent> {
    vec![
        HelixStreamingTraceEvent::Pulse {
            start_us: 1_000_000,
            duration_us: 44_000,
            pulse_id: 17,
            previous_consumed_mel_frames: 1_600,
            consumed_mel_frames: 1_624,
            pulse_mel_frames: 24,
            committed_text_len_start: 78,
            speculative_len_start: 4,
            committed_tokens: 3,
            retained_speculative_tokens: 5,
            resident_committed_tokens: 80,
            evicted_audio_tokens: 2,
            evicted_committed_tokens: 1,
        },
        HelixStreamingTraceEvent::RefreshPrompt {
            start_us: 1_002_500,
            duration_us: 8_000,
            pulse_id: 17,
            first_audio_token_id: 32,
            resident_audio_frames: 8,
            committed_text_len: 80,
            resident_committed_len: 80,
            resident_text_len: 85,
            logical_start: 90,
            logical_end: 118,
            text_token_start: 90,
            text_token_end: 92,
            spans: vec![HelixTracePositionSpan {
                logical_start: 90,
                rows: 8,
                physical_start: 12,
            }],
        },
        HelixStreamingTraceEvent::Verify {
            start_us: 1_004_000,
            duration_us: 4_000,
            pulse_id: 17,
            rewind_k: 2,
            post_rewind_text_len: 81,
            text_token_start: 90,
            text_token_end: 92,
            logical_start: 114,
            logical_end: 117,
            spans: vec![HelixTracePositionSpan {
                logical_start: 114,
                rows: 3,
                physical_start: 46,
            }],
            accepted_prefix_len: Some(1),
            divergence_row: Some(1),
            next_token_seed: Some(1401),
            discarded_speculative_tokens: Some(1),
            invalidated_speculative_slots: Some(2),
        },
        HelixStreamingTraceEvent::ArDecode {
            start_us: 1_005_000,
            duration_us: 16_000,
            pulse_id: 17,
            decode_steps: 5,
            decoded_tokens: 6,
            speculative_len_entering: 1,
            live_speculative_tokens: 6,
            hit_eos: false,
            seed_token_id: 1401,
            seed_token_text: "pho".to_string(),
            early_exit_reason: HelixArDecodeEarlyExitReason::BudgetExhausted,
            next_after_tail: 1502,
        },
        HelixStreamingTraceEvent::ArToken {
            start_us: 1_005_100,
            duration_us: 300,
            pulse_id: 17,
            step_index: 0,
            input_token_id: 1401,
            input_text: "pho".to_string(),
            text_token_id: 91,
            query_position: 118,
            physical_start: 49,
            summary_records: 64,
            next_token_id: 1502,
            next_text: "n".to_string(),
        },
        HelixStreamingTraceEvent::Commit {
            start_us: 1_007_500,
            duration_us: 1_000,
            pulse_id: 17,
            speculative_len_pre: 6,
            revisable_tail_target: 2,
            committed_tokens: 3,
            retained_speculative_tokens: 5,
            committed_text_len: 83,
            next_after_committed: 1502,
        },
        HelixStreamingTraceEvent::VerifySkipped {
            timestamp_us: 1_007_800,
            pulse_id: 17,
            reason: HelixVerifySkippedReason::PreCommitFullRewind,
            rewind_k: 0,
            resident_committed_len: 0,
            speculative_len: 2,
        },
    ]
}

fn sample_helix_chrome_events() -> Vec<HelixChromeTraceEvent> {
    let mut args = BTreeMap::new();
    args.insert(VString::new("pulse_id").into(), Value::from(17_u64));
    vec![HelixChromeTraceEvent {
        name: "metal.dispatch".to_string(),
        cat: "gpu".to_string(),
        ph: "X".to_string(),
        ts: 1_006_000.0,
        dur: Some(420.0),
        pid: 2,
        tid: 7,
        s: None,
        args,
    }]
}

fn sample_helix_pulse_evidence() -> HelixPulseEvidenceSnapshot {
    HelixPulseEvidenceSnapshot {
        pulse_id: HelixSchedulerPulseId(17),
        encoder: Some(HelixEncoderFactsSnapshot {
            refreshed_audio: helix_audio_range(32, 40),
            audio_representation_version: HelixAudioRepresentationVersion(7),
            provenance: sample_helix_audio_provenance(),
        }),
        counts: HelixDecoderEvidenceFactCounts {
            decode: 1,
            verify_prediction: 1,
            verify_seed: 1,
            prompt_prefill: 1,
        },
        decode: vec![HelixDecodeFact {
            text_token_id: HelixTextTokenId(91),
            query_position: HelixLogicalPosition(118),
            input_token_id: 1401,
            observed_audio: helix_audio_range(32, 40),
        }],
        verify_prediction: vec![HelixVerifyPredictionFact {
            verified_text_token_id: HelixTextTokenId(92),
            verified_draft_index: 1,
            draft_token_id: 1202,
            query_row: 2,
            query_position: HelixLogicalPosition(116),
            observed_audio: helix_audio_range(36, 40),
        }],
        verify_seed: vec![HelixVerifySeedFact {
            query_row: 3,
            query_position: HelixLogicalPosition(117),
            next_token_seed: 1401,
            observed_audio: helix_audio_range(40, 48),
        }],
        prompt_prefill: vec![HelixPromptPrefillFact {
            query_position: HelixLogicalPosition(90),
            observed_audio: helix_audio_range(32, 40),
        }],
    }
}

fn sample_helix_attention_batch() -> HelixAttentionSummaryBatch {
    HelixAttentionSummaryBatch {
        schema_version: 5,
        pulse_id: HelixSchedulerPulseId(17),
        audio_context_id: 7001,
        text_context_id: 8001,
        audio_representation_spans: vec![HelixAudioRepresentationSpan {
            audio: helix_audio_range(32, 40),
            audio_representation_version: HelixAudioRepresentationVersion(7),
        }],
        changed_audio_representation_spans: vec![HelixAudioRepresentationSpan {
            audio: helix_audio_range(34, 36),
            audio_representation_version: HelixAudioRepresentationVersion(8),
        }],
        text_support: sample_helix_text_support(),
        header_text_support: vec![HelixQueryRowAttentionRecord {
            query_position: HelixLogicalPosition(90),
            decoder_layer_index: 1,
            head_index: 2,
            support: sample_helix_support(),
            audio_weights: vec![0.04, 0.06, 0.12, 0.20],
        }],
        audio_encoder_support: vec![HelixAudioEncoderSupportRecord {
            audio_token_id: HelixAudioTokenId(34),
            audio_representation_version: HelixAudioRepresentationVersion(7),
            encoder_layer_index: 3,
            head_index: 4,
            support: sample_helix_support(),
            frontier_debt: 0.125,
        }],
        decoder_evidence: vec![
            HelixDecoderEvidenceRecord {
                text_token_id: Some(HelixTextTokenId(91)),
                query_position: HelixLogicalPosition(118),
                expected_observed_audio: helix_audio_range(32, 40),
                records: sample_helix_text_support(),
                kind: HelixDecoderEvidenceKind::Decode {
                    input_token_id: 1401,
                },
            },
            HelixDecoderEvidenceRecord {
                text_token_id: Some(HelixTextTokenId(92)),
                query_position: HelixLogicalPosition(116),
                expected_observed_audio: helix_audio_range(36, 40),
                records: sample_helix_text_support(),
                kind: HelixDecoderEvidenceKind::VerifyPrediction {
                    verified_draft_index: 1,
                    draft_token_id: 1202,
                    query_row: 2,
                    max_logit: 11.5,
                    draft_logit: 9.25,
                },
            },
            HelixDecoderEvidenceRecord {
                text_token_id: None,
                query_position: HelixLogicalPosition(117),
                expected_observed_audio: helix_audio_range(40, 48),
                records: sample_helix_text_support(),
                kind: HelixDecoderEvidenceKind::VerifySeed {
                    query_row: 3,
                    next_token_seed: 1401,
                    max_logit: 10.75,
                },
            },
            HelixDecoderEvidenceRecord {
                text_token_id: None,
                query_position: HelixLogicalPosition(90),
                expected_observed_audio: helix_audio_range(32, 40),
                records: sample_helix_text_support(),
                kind: HelixDecoderEvidenceKind::PromptPrefill,
            },
        ],
    }
}

fn sample_helix_piece_eval() -> HelixPieceEvalSnapshot {
    HelixPieceEvalSnapshot {
        audio_now_ms: 12_500.0,
        reference_words_available: 42,
        hypothesis_words: 40,
        substitutions: 2,
        deletions: 1,
        insertions: 0,
        rolling_wer: 0.0714,
        s2d_matched_words: 38,
        s2d_new_words: 4,
        s2d_p50_ms: Some(210.0),
        s2d_p90_ms: Some(330.0),
        s2d_p100_ms: Some(410.0),
        s2d_avg_ms: Some(240.5),
        audio_frontier: 43,
        displayed_frontier: 40,
        committed_frontier: 38,
        lag_ms: -120.0,
    }
}

fn sample_helix_pulse_bundle(
    pulse_id: HelixSchedulerPulseId,
    prompt_layout: HelixPromptLayout,
    heatmap: HelixPulseAttentionHeatmap,
    rollup: HelixPulseRollup,
) -> HelixPulseBundle {
    HelixPulseBundle {
        pulse_id,
        schema_version: 1,
        prompt_layout: Some(prompt_layout),
        audio_provenance: Some(sample_helix_audio_provenance()),
        attention_heatmap: Some(heatmap),
        encoder_frontier: Some(sample_helix_encoder_frontier()),
        encoder_provenance: Some(sample_helix_encoder_provenance_report()),
        audio_clip: Some(sample_helix_audio_clip()),
        mel_clip: Some(sample_helix_mel_clip()),
        pulse_rollup: Some(rollup),
        timeline: Some(sample_helix_timeline()),
        gpu_chrome_events: Some(sample_helix_chrome_events()),
        verify_evidence: Some(sample_helix_verify_evidence()),
        scheduler_snapshot: Some(sample_helix_pulse_evidence()),
    }
}

fn sample_helix_trace_service_surface() -> HelixTraceServiceSurface {
    let pulse_id = HelixSchedulerPulseId(17);
    let audio_range = helix_audio_range(32, 40);
    let rollup = HelixPulseRollup {
        pulse_id,
        pulse_start_us: Some(1_000_000),
        pulse_duration_us: Some(44_000),
        encoder_duration_us: Some(12_000),
        refresh_duration_us: Some(8_000),
        verify_duration_us: Some(4_000),
        decode_duration_us: Some(16_000),
        commit_duration_us: Some(1_000),
        pulse_mel_frames: 24,
        committed_tokens: 3,
        retained_speculative_tokens: 5,
        resident_committed_tokens: 80,
        evicted_audio_tokens: 2,
        evicted_committed_tokens: 1,
        decoded_tokens: 6,
        hit_eos: false,
        verify: Some(HelixVerifyOutcome {
            rewind_k: 2,
            accepted_prefix_len: Some(3),
            divergence_row: Some(4),
            discarded_speculative_tokens: None,
        }),
        has_attention_batch: true,
        ar_token_count: 6,
    };
    let prompt_layout = HelixPromptLayout {
        pulse_id,
        first_audio_token_id: audio_range.start,
        resident_audio_frames: 8,
        changed_audio_spans: vec![HelixAudioRepresentationSpan {
            audio: audio_range,
            audio_representation_version: HelixAudioRepresentationVersion(3),
        }],
        text_token_start: HelixTextTokenId(90),
        text_token_end: HelixTextTokenId(92),
        text_tokens: vec![
            HelixTextTokenSnapshot {
                text_token_id: HelixTextTokenId(90),
                text: Some("pho".to_string()),
                text_before: Some("fo".to_string()),
                in_verify_batch: true,
                decoded_this_pulse: true,
            },
            HelixTextTokenSnapshot {
                text_token_id: HelixTextTokenId(91),
                text: Some("n".to_string()),
                text_before: None,
                in_verify_batch: false,
                decoded_this_pulse: true,
            },
        ],
    };
    let heatmap = HelixPulseAttentionHeatmap {
        pulse_id,
        first_audio_token_id: audio_range.start,
        audio_token_count: 4,
        text_token_start: HelixTextTokenId(90),
        text_token_count: 2,
        record_count: 8,
        max_value: 0.75,
        mean_audio_mass: vec![0.1, 0.2, 0.3, 0.4, 0.05, 0.15, 0.25, 0.35],
        text_token_glyphs: vec!["pho".to_string(), "n".to_string()],
    };
    let metrics = HelixStreamMetrics {
        pulse_ids: vec![HelixSchedulerPulseId(16), pulse_id],
        pulse_duration_us: vec![42_000, 44_000],
        decoded_tokens: vec![5, 6],
        committed_tokens: vec![2, 3],
        retained_speculative_tokens: vec![4, 5],
        evicted_audio_tokens: vec![0, 2],
        evicted_committed_tokens: vec![0, 1],
        rewind_k: vec![0, 2],
        ar_token_count: vec![5, 6],
        rolling_wer: vec![0.18, 0.16],
        s2d_p50_ms: vec![220.0, 210.0],
    };
    let run_info = HelixRunInfo {
        backend: "metal".to_string(),
        model_dir: "/weights/qwen3-asr".to_string(),
        input: "/audio/sample.wav".to_string(),
        piece: Some("ceramic".to_string()),
        pulse_ms: 120,
        audio_ring_capacity: 512,
        text_ring_capacity: 256,
        commit_revisable_tail_text_tokens: 8,
        revise_logit_margin: 1.5,
        sample_rate: 16_000,
        mel_hop_samples: 160,
        num_mel_bins: 128,
        num_mel_frames: 2_048,
        audio_tokens_per_chunk: 8,
        native_window_tokens: 64,
        realtime_pacing: true,
        profile_phases: false,
        attention_trace_schema_version: 2,
        trace_server_schema_version: 1,
    };

    HelixTraceServiceSurface {
        meta: HelixStreamMeta {
            schema_version: 1,
            pulse_ids: vec![HelixSchedulerPulseId(16), pulse_id],
            timeline_event_count: 420,
            attention_batch_count: 17,
        },
        pulse_rollup: Some(rollup.clone()),
        timeline: sample_helix_timeline(),
        attention_batch: Some(sample_helix_attention_batch()),
        prompt_layout: Some(prompt_layout.clone()),
        audio_attended_by: vec![HelixTextAttendanceRow {
            text_token_id: HelixTextTokenId(91),
            decoder_layer_index: 7,
            head_index: 3,
            dominant_audio_mass: 0.21,
            total_audio_mass: 0.42,
            observed_audio: audio_range,
            dominant_audio: helix_audio_range(34, 36),
            audio_weights: vec![0.05, 0.07, 0.10, 0.20],
            queried_audio_weight: 0.10,
        }],
        text_attends_to: vec![HelixAudioAttendanceRow {
            decoder_layer_index: 7,
            head_index: 3,
            dominant_audio_mass: 0.21,
            total_audio_mass: 0.42,
            center_audio_token: Some(35.25),
            width_audio_tokens: Some(3.5),
            observed_audio: audio_range,
            dominant_audio: helix_audio_range(34, 36),
            audio_weights: vec![0.05, 0.07, 0.10, 0.20],
        }],
        refresh_attends_to: vec![HelixRefreshAttendanceRow {
            query_position: HelixLogicalPosition(90),
            decoder_layer_index: 1,
            head_index: 2,
            dominant_audio_mass: 0.18,
            total_audio_mass: 0.38,
            center_audio_token: Some(34.75),
            width_audio_tokens: Some(2.5),
            observed_audio: audio_range,
            dominant_audio: helix_audio_range(34, 36),
            audio_weights: vec![0.04, 0.06, 0.12, 0.16],
        }],
        audio_token_provenance: sample_helix_audio_provenance().into_iter().next(),
        audio_provenance_for_pulse: sample_helix_audio_provenance(),
        audio_tokens_for_mel_frame: vec![HelixAudioTokenId(34), HelixAudioTokenId(35)],
        audio_clip_for_audio_token: Some(sample_helix_audio_clip()),
        audio_clip_for_prompt: Some(sample_helix_audio_clip()),
        audio_clip_for_audio_range: Some(sample_helix_audio_clip()),
        mel_clip_for_prompt: Some(sample_helix_mel_clip()),
        audio_self_attention: vec![HelixAudioSelfAttentionRow {
            encoder_layer_index: 3,
            head_index: 4,
            audio_representation_version: HelixAudioRepresentationVersion(7),
            dominant_audio_mass: 0.33,
            total_audio_mass: 0.77,
            center_audio_token: Some(35.5),
            width_audio_tokens: Some(4.0),
            observed_audio: audio_range,
            dominant_audio: helix_audio_range(34, 36),
            frontier_debt: 0.125,
        }],
        transcript: vec![
            HelixTranscriptToken {
                text_token_id: HelixTextTokenId(90),
                decoded_in_pulse: pulse_id,
                text: "pho".to_string(),
                committed: true,
            },
            HelixTranscriptToken {
                text_token_id: HelixTextTokenId(91),
                decoded_in_pulse: pulse_id,
                text: "n".to_string(),
                committed: false,
            },
        ],
        pulse_attention_heatmap: Some(heatmap.clone()),
        encoder_frontier: Some(sample_helix_encoder_frontier()),
        stream_metrics: metrics.clone(),
        verify_evidence: Some(sample_helix_verify_evidence()),
        decoder_evidence_report: HelixDecoderEvidenceReport {
            total_batches: 17,
            batches_without_decoder_evidence: 1,
            pulses_without_decoder_evidence: vec![HelixSchedulerPulseId(16)],
            variant_evidence_counts: HelixDecoderEvidenceVariantCounts {
                decode: 12,
                verify_prediction: 6,
                verify_seed: 3,
                prompt_prefill: 4,
            },
            variant_record_counts: HelixDecoderEvidenceVariantCounts {
                decode: 96,
                verify_prediction: 48,
                verify_seed: 24,
                prompt_prefill: 32,
            },
            observed_decoder_layer_indices: vec![0, 1, 7],
            observed_decoder_head_indices: vec![0, 2, 3],
        },
        pulse_evidence_snapshot: Some(sample_helix_pulse_evidence()),
        gpu_chrome_events_for_pulse: sample_helix_chrome_events(),
        run_info: Some(run_info.clone()),
        piece_eval_reference: Some(HelixPieceEvalReference {
            piece: "ceramic".to_string(),
            language: "en".to_string(),
            words: vec!["phon".to_string(), "surface".to_string()],
        }),
        piece_eval_for_pulse: Some(sample_helix_piece_eval()),
        encoder_provenance_report: Some(sample_helix_encoder_provenance_report()),
        pulse_bundle_fields: HelixPulseBundleFields {
            prompt_layout: true,
            audio_provenance: true,
            attention_heatmap: true,
            encoder_frontier: true,
            encoder_provenance: true,
            audio_clip: true,
            mel_clip: true,
            pulse_rollup: true,
            timeline: true,
            gpu_chrome_events: true,
            verify_evidence: true,
            scheduler_snapshot: true,
        },
        pulse_bundle: sample_helix_pulse_bundle(pulse_id, prompt_layout, heatmap, rollup),
        pulse_available: HelixPulseAvailable { pulse_id },
    }
}

// Mirrors the small Vox live-reload surface in:
// /Users/amos/hotmeal/hotmeal-server/src/lib.rs
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
enum HotmealLiveReloadEvent {
    Reload,
    Patches {
        route: String,
        patches_blob: Vec<u8>,
    },
    HeadChanged {
        route: String,
    },
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HotmealSubscribeRequest {
    route: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct HotmealLiveReloadFixture {
    subscribe: HotmealSubscribeRequest,
    events: Vec<HotmealLiveReloadEvent>,
}

// Migration target for /Users/amos/tracey/crates/tracey-proto/src/lib.rs.
// The current roam DTOs use usize in several fields; this target Vox fixture
// uses fixed-width integers because phon schemas are cross-language.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct TraceyRuleId {
    base: String,
    version: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct TraceyRuleRef {
    id: TraceyRuleId,
    text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct TraceySectionRules {
    section: String,
    rules: Vec<TraceyRuleRef>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct TraceyUncoveredRequest {
    spec: Option<String>,
    impl_name: Option<String>,
    prefix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct TraceyUncoveredResponse {
    spec: String,
    impl_name: String,
    total_rules: usize,
    uncovered_count: usize,
    by_section: Vec<TraceySectionRules>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct TraceyImplStatus {
    spec: String,
    impl_name: String,
    total_rules: usize,
    covered_rules: usize,
    stale_rules: usize,
    verified_rules: usize,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct TraceyStatusResponse {
    impls: Vec<TraceyImplStatus>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct TraceyCoverageChange {
    rule_id: TraceyRuleId,
    file: String,
    line: usize,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct TraceyDeltaSummary {
    newly_covered: Vec<TraceyCoverageChange>,
    newly_uncovered: Vec<TraceyRuleId>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct TraceyDataUpdate {
    version: u64,
    delta: Option<TraceyDeltaSummary>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct TraceyLspDiagnostic {
    severity: String,
    code: String,
    message: String,
    start_line: u32,
    start_char: u32,
    end_line: u32,
    end_char: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct TraceyLspFileDiagnostics {
    path: String,
    diagnostics: Vec<TraceyLspDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct TraceyLspSymbol {
    name: String,
    kind: String,
    path: Option<String>,
    start_line: u32,
    start_char: u32,
    end_line: u32,
    end_char: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct TraceyMigrationFixture {
    status: TraceyStatusResponse,
    uncovered_request: TraceyUncoveredRequest,
    uncovered_response: TraceyUncoveredResponse,
    data_update_item: TraceyDataUpdate,
    workspace_diagnostics: Vec<TraceyLspFileDiagnostics>,
    workspace_symbols: Vec<TraceyLspSymbol>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct NativeSizedPayload {
    count: usize,
    delta: isize,
    counts: Vec<usize>,
    maybe_delta: Option<isize>,
}

#[track_caller]
// r[verify typed.no-dynamic-bounce]
fn roundtrip<T>(value: T) -> phon::api::MethodJitFallbackReport
where
    T: for<'facet> Facet<'facet> + PartialEq + std::fmt::Debug,
{
    let codec = Codec::<T>::new().expect("ecosystem fixture should lower");
    let bytes = codec
        .encode(&value)
        .expect("ecosystem fixture should encode");
    let back = codec
        .decode(&bytes)
        .expect("ecosystem fixture should decode");
    assert_eq!(back, value);
    codec.jit_fallback_report().scoped("ecosystem", "fixture")
}

#[track_caller]
fn expect_native_clean_when_jit_available(
    report: phon::api::MethodJitFallbackReport,
    context: &str,
) {
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    assert!(report.is_empty(), "{context}: {report:#?}");

    #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
    let _ = (report, context);
}

fn dynamic_object() -> Value {
    let mut object = VObject::new();
    object.insert(VString::new("sidebar"), Value::from(true));
    object.insert(VString::new("title"), Value::from("Phon migration"));
    object.insert(VString::new("count"), Value::from(42i64));
    object.into()
}

fn sample_dodeca_dynamic_data_value() -> Value {
    let mut object = VObject::new();
    object.insert(VString::new("title"), Value::from("Phon"));
    object.insert(VString::new("sidebar"), Value::from(true));
    object.insert(VString::new("count"), Value::from(42i64));
    object.into()
}

fn sample_dodeca_load_data_result() -> DodecaLoadDataResult {
    DodecaLoadDataResult::Success {
        value: sample_dodeca_dynamic_data_value(),
    }
}

fn dodeca_frontmatter_extra() -> Value {
    let mut object = VObject::new();
    object.insert(VString::new("sidebar"), Value::from(true));
    object.insert(VString::new("icon"), Value::from("book"));
    object.insert(VString::new("custom_value"), Value::from(42i64));
    object.into()
}

fn sample_dodeca_parse_result() -> DodecaParseResult {
    DodecaParseResult::Success {
        frontmatter: DodecaFrontmatter {
            title: "Phon migration".to_string(),
            weight: 10,
            description: Some("Generated fixture for Dodeca markdown".to_string()),
            template: Some("page.html".to_string()),
            extra: dodeca_frontmatter_extra(),
        },
        html: "<h1 data-sid=\"h1\">Intro</h1><p data-sid=\"p1\">Generated fixture</p>".to_string(),
        headings: vec![DodecaMarkdownHeading {
            title: "Intro".to_string(),
            id: "intro".to_string(),
            level: 1,
        }],
        reqs: vec![DodecaReqDefinition {
            id: "vox.dodeca.markdown".to_string(),
            anchor_id: "r-vox-dodeca-markdown".to_string(),
        }],
        head_injections: vec![
            "<link rel=\"stylesheet\" href=\"/assets/arborium.css\">".to_string(),
        ],
        source_map: Box::new(DodecaSourceMap {
            source_path: Some("content/guide.md".to_string()),
            entries: vec![
                DodecaSourceMapEntry {
                    id: "h1".to_string(),
                    kind: DodecaSourceKind::Heading,
                    line_start: 5,
                    line_end: 5,
                    byte_start: 38,
                    byte_end: 45,
                },
                DodecaSourceMapEntry {
                    id: "p1".to_string(),
                    kind: DodecaSourceKind::Paragraph,
                    line_start: 7,
                    line_end: 7,
                    byte_start: 47,
                    byte_end: 71,
                },
            ],
        }),
    }
}

fn sample_dodeca_decoded_image(seed: u8, width: u32, height: u32) -> DodecaDecodedImage {
    let pixels = (0..width * height * 4)
        .map(|i| seed.wrapping_add((i & 0xff) as u8))
        .collect();
    DodecaDecodedImage {
        pixels,
        width,
        height,
        channels: 4,
    }
}

fn sample_dodeca_image_processor_fixture() -> DodecaImageProcessorFixture {
    let decoded = sample_dodeca_decoded_image(0x20, 4, 3);
    let resized = sample_dodeca_decoded_image(0x80, 2, 2);
    DodecaImageProcessorFixture {
        png_data: vec![
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d,
        ],
        decoded_result: DodecaImageResult::Success {
            image: decoded.clone(),
        },
        resize_input: DodecaResizeInput {
            pixels: decoded.pixels.clone(),
            width: decoded.width,
            height: decoded.height,
            channels: decoded.channels,
            target_width: 2,
        },
        resize_result: DodecaImageResult::Success { image: resized },
        thumbhash_input: DodecaThumbhashInput {
            pixels: decoded.pixels,
            width: decoded.width,
            height: decoded.height,
        },
        thumbhash_result: DodecaImageResult::ThumbhashSuccess {
            data_url: "data:image/png;base64,thumbhash-fixture".to_string(),
        },
        error_result: DodecaImageResult::Error {
            message: "unsupported image format".to_string(),
        },
    }
}

fn sample_dodeca_search_indexer_fixture() -> DodecaSearchIndexerFixture {
    DodecaSearchIndexerFixture {
        pages: vec![
            DodecaSearchPage {
                url: "/guide/intro/".to_string(),
                source: "docs".to_string(),
                html: "<main><h1>Intro</h1><p>Phon and Vox migration.</p></main>".to_string(),
            },
            DodecaSearchPage {
                url: "/reference/jit/".to_string(),
                source: "docs".to_string(),
                html: "<main><h1>JIT</h1><p>Typed plans lower to native code.</p></main>"
                    .to_string(),
            },
        ],
        result: DodecaSearchIndexResult::Success {
            files: vec![
                DodecaSearchFile {
                    path: "/search/meta".to_string(),
                    contents: vec![0x91, 0x02, 0x01, 0x00],
                },
                DodecaSearchFile {
                    path: "/search/chunk-0".to_string(),
                    contents: vec![0xde, 0xad, 0xbe, 0xef, 0x42],
                },
            ],
        },
        error_result: DodecaSearchIndexResult::Error {
            message: "search index extraction failed".to_string(),
        },
    }
}

fn sample_dodeca_asset_processing_fixture() -> DodecaAssetProcessingFixture {
    let mut css_path_map = HashMap::new();
    css_path_map.insert("/old/bg.png".to_string(), "/assets/bg.abcd.png".to_string());
    css_path_map.insert(
        "/old/font.woff2".to_string(),
        "/assets/font.woff2".to_string(),
    );

    let mut sass_files = HashMap::new();
    sass_files.insert(
        "styles/app.scss".to_string(),
        "$brand: #c0ffee; @import 'partials/buttons'; body { color: $brand; }".to_string(),
    );
    sass_files.insert(
        "styles/partials/_buttons.scss".to_string(),
        ".button { padding: 4px; }".to_string(),
    );

    DodecaAssetProcessingFixture {
        css_source: "body { background: url('/old/bg.png'); color: red; }".to_string(),
        css_path_map,
        css_result: DodecaCssResult::Success {
            css: "body{background:url('/assets/bg.abcd.png');color:red}".to_string(),
        },
        sass_entrypoint: "styles/app.scss".to_string(),
        sass_files,
        sass_load_paths: vec!["styles".to_string(), "vendor".to_string()],
        sass_result: DodecaSassResult::Success {
            css: "body{color:#c0ffee}.button{padding:4px}".to_string(),
        },
        svg_source:
            "<svg viewBox=\"0 0 10 10\"><rect width=\"10\" height=\"10\" fill=\"red\"/></svg>"
                .to_string(),
        svgo_result: DodecaSvgoResult::Success {
            svg: "<svg viewBox=\"0 0 10 10\"><path fill=\"red\" d=\"M0 0h10v10H0z\"/></svg>"
                .to_string(),
        },
    }
}

fn sample_dodeca_task_progress(
    name: &str,
    total: u32,
    completed: u32,
    status: DodecaTaskStatus,
) -> DodecaTaskProgress {
    DodecaTaskProgress {
        name: name.to_string(),
        total,
        completed,
        status,
        message: (status == DodecaTaskStatus::Error).then(|| format!("{name} failed")),
    }
}

fn sample_dodeca_small_cell_services_fixture() -> DodecaSmallCellServicesFixture {
    let mut js_path_map = HashMap::new();
    js_path_map.insert(
        "/assets/app.js".to_string(),
        "/assets/app.1234.js".to_string(),
    );
    js_path_map.insert(
        "/assets/theme.css".to_string(),
        "/assets/theme.abcd.css".to_string(),
    );

    let mut link_results = HashMap::new();
    link_results.insert("https://example.com/ok".to_string(), DodecaLinkStatus::Ok);
    link_results.insert(
        "https://example.com/missing".to_string(),
        DodecaLinkStatus::HttpError {
            code: 404,
            diagnostics: DodecaLinkDiagnostics {
                request_headers: vec![("accept".to_string(), "text/html".to_string())],
                response_headers: vec![("content-type".to_string(), "text/html".to_string())],
                response_body: "<h1>not found</h1>".to_string(),
            },
        },
    );
    link_results.insert(
        "https://slow.example.com".to_string(),
        DodecaLinkStatus::Skipped,
    );

    DodecaSmallCellServicesFixture {
        ready_msg: DodecaReadyMsg {
            peer_id: 42,
            cell_name: "ddc-cell-fonts".to_string(),
            pid: Some(12_345),
            version: Some("1.0.0-dev".to_string()),
            features: vec!["woff2".to_string(), "subset".to_string()],
        },
        ready_ack: DodecaReadyAck {
            ok: true,
            host_time_unix_ms: Some(1_778_000_000_000),
        },
        minify_result: DodecaMinifyResult::Success {
            content: "<main><h1>Hi</h1></main>".to_string(),
        },
        js_input: DodecaJsRewriteInput {
            js: "import '/assets/theme.css'; console.log('/assets/app.js')".to_string(),
            path_map: js_path_map,
        },
        js_result: Ok(
            "import '/assets/theme.abcd.css'; console.log('/assets/app.1234.js')".to_string(),
        ),
        html_diff_input: DodecaHtmlDiffInput {
            old_html: "<main><h1>Old</h1></main>".to_string(),
            new_html: "<main><h1>New</h1><p>body</p></main>".to_string(),
        },
        html_diff_result: Ok(DodecaHtmlDiffOutcome {
            patches_blob: vec![0x91, 0xa4, b'p', b'a', b't', b'h'],
        }),
        subset_font_input: DodecaSubsetFontInput {
            data: vec![0x77, 0x4f, 0x46, 0x32],
            chars: vec!['A', '\u{00e9}', '\u{1f41d}'],
        },
        font_results: vec![
            DodecaFontResult::DecompressSuccess {
                data: vec![0x00, 0x01, 0x00, 0x00],
            },
            DodecaFontResult::SubsetSuccess {
                data: vec![0xde, 0xad, 0xbe, 0xef],
            },
            DodecaFontResult::CompressSuccess {
                data: vec![0x77, 0x4f, 0x46, 0x32, 0x01],
            },
        ],
        webp_encode_input: DodecaWebpEncodeInput {
            pixels: vec![0, 32, 64, 255, 255, 128, 0, 255],
            width: 2,
            height: 1,
            quality: 82,
        },
        webp_results: vec![
            DodecaWebpResult::DecodeSuccess {
                pixels: vec![0, 32, 64, 255],
                width: 1,
                height: 1,
                channels: 4,
            },
            DodecaWebpResult::EncodeSuccess {
                data: vec![b'R', b'I', b'F', b'F'],
            },
        ],
        jxl_encode_input: DodecaJxlEncodeInput {
            pixels: vec![0, 0, 0, 255, 255, 255, 255, 255],
            width: 2,
            height: 1,
            quality: 90,
        },
        jxl_results: vec![
            DodecaJxlResult::DecodeSuccess {
                pixels: vec![255, 0, 255, 255],
                width: 1,
                height: 1,
                channels: 4,
            },
            DodecaJxlResult::Error {
                message: "unsupported color profile".to_string(),
            },
        ],
        select_result: DodecaSelectResult::Selected { index: 2 },
        confirm_result: DodecaConfirmResult::Yes,
        record_config: DodecaRecordConfig {
            shell: Some("/bin/zsh".to_string()),
        },
        term_result: DodecaTermResult::Success {
            html: "<t-b>cargo nextest</t-b>".to_string(),
        },
        start_dev_server_result: DodecaStartDevServerResult::Success { port: 5173 },
        run_build_result: DodecaRunBuildResult::Error {
            message: "vite config missing".to_string(),
        },
        link_check_input: DodecaLinkCheckInput {
            urls: vec![
                "https://example.com/ok".to_string(),
                "https://example.com/missing".to_string(),
            ],
            delay_ms: 250,
            timeout_secs: 15,
        },
        link_check_result: DodecaLinkCheckResult::Success {
            output: DodecaLinkCheckOutput {
                results: link_results,
            },
        },
        build_progress: DodecaBuildProgress {
            parse: sample_dodeca_task_progress("parse", 12, 12, DodecaTaskStatus::Done),
            render: sample_dodeca_task_progress("render", 48, 40, DodecaTaskStatus::Running),
            sass: sample_dodeca_task_progress("sass", 3, 3, DodecaTaskStatus::Done),
            links: sample_dodeca_task_progress("links", 10, 7, DodecaTaskStatus::Running),
            search: sample_dodeca_task_progress("search", 1, 0, DodecaTaskStatus::Pending),
        },
        log_event: DodecaLogEvent {
            level: DodecaLogLevel::Warn,
            kind: DodecaEventKind::Http { status: 404 },
            message: "dead link".to_string(),
            fields: vec![
                ("route".to_string(), "/guide/".to_string()),
                ("href".to_string(), "/missing/".to_string()),
            ],
        },
        server_status: DodecaServerStatus {
            urls: vec![
                "http://127.0.0.1:5173".to_string(),
                "http://192.168.1.42:5173".to_string(),
            ],
            is_running: true,
            bind_mode: DodecaBindMode::Lan,
            picante_cache_size: 4_096,
            cas_cache_size: 8_192,
            code_exec_cache_size: 1_024,
        },
        server_command: DodecaServerCommand::SetLogFilter {
            filter: "dodeca=debug,cell=trace".to_string(),
        },
        command_result: DodecaCommandResult::Ok,
    }
}

fn value_object(entries: &[(&str, Value)]) -> Value {
    let mut object = VObject::new();
    for (key, value) in entries {
        object.insert(VString::new(key), value.clone());
    }
    object.into()
}

fn styx_span(start: u32, end: u32) -> Option<StyxSpan> {
    Some(StyxSpan { start, end })
}

fn styx_scalar(text: &str, kind: StyxScalarKind, start: u32, end: u32) -> StyxValue {
    StyxValue {
        tag: None,
        payload: Some(StyxPayload::Scalar(StyxScalar {
            text: text.to_string(),
            kind,
            span: styx_span(start, end),
        })),
        span: styx_span(start, end),
    }
}

fn sample_styx_value() -> StyxValue {
    StyxValue {
        tag: Some(StyxTag {
            name: "schema".to_string(),
            span: styx_span(0, 7),
        }),
        payload: Some(StyxPayload::Object(StyxObject {
            entries: vec![
                StyxEntry {
                    key: styx_scalar("title", StyxScalarKind::Bare, 9, 14),
                    value: styx_scalar("Phon migration", StyxScalarKind::Quoted, 15, 31),
                    doc_comment: Some("page title".to_string()),
                },
                StyxEntry {
                    key: styx_scalar("features", StyxScalarKind::Bare, 33, 41),
                    value: StyxValue {
                        tag: Some(StyxTag {
                            name: "seq".to_string(),
                            span: styx_span(42, 46),
                        }),
                        payload: Some(StyxPayload::Sequence(StyxSequence {
                            items: vec![
                                styx_scalar("jit", StyxScalarKind::Bare, 47, 50),
                                StyxValue {
                                    tag: Some(StyxTag {
                                        name: "object".to_string(),
                                        span: styx_span(51, 58),
                                    }),
                                    payload: Some(StyxPayload::Object(StyxObject {
                                        entries: vec![StyxEntry {
                                            key: styx_scalar("lang", StyxScalarKind::Bare, 59, 63),
                                            value: styx_scalar("rust", StyxScalarKind::Raw, 64, 70),
                                            doc_comment: None,
                                        }],
                                        span: styx_span(58, 71),
                                    })),
                                    span: styx_span(51, 71),
                                },
                            ],
                            span: styx_span(46, 72),
                        })),
                        span: styx_span(42, 72),
                    },
                    doc_comment: None,
                },
            ],
            span: styx_span(8, 73),
        })),
        span: styx_span(0, 73),
    }
}

fn sample_styx_lsp_uri() -> String {
    "file:///workspace/queries.styx".to_string()
}

fn sample_styx_lsp_source() -> String {
    "@query { from products select (id name) }".to_string()
}

fn sample_styx_lsp_position() -> StyxLspPosition {
    StyxLspPosition {
        line: 0,
        character: 16,
    }
}

fn sample_styx_lsp_cursor() -> StyxLspCursor {
    StyxLspCursor {
        line: 0,
        character: 16,
        offset: 16,
    }
}

fn sample_styx_lsp_range() -> StyxLspRange {
    StyxLspRange {
        start: StyxLspPosition {
            line: 0,
            character: 0,
        },
        end: StyxLspPosition {
            line: 0,
            character: 38,
        },
    }
}

fn sample_styx_lsp_initialize_params() -> StyxLspInitializeParams {
    StyxLspInitializeParams {
        styx_version: "4.0".to_string(),
        document_uri: sample_styx_lsp_uri(),
        schema_id: "crate:dibs-queries@1".to_string(),
    }
}

fn sample_styx_lsp_initialize_result() -> StyxLspInitializeResult {
    StyxLspInitializeResult {
        name: "dibs-styx-extension".to_string(),
        version: "0.1.0".to_string(),
        capabilities: vec![
            StyxLspCapability::Completions,
            StyxLspCapability::Hover,
            StyxLspCapability::Diagnostics,
            StyxLspCapability::CodeActions,
            StyxLspCapability::Definition,
        ],
    }
}

fn sample_styx_lsp_completion_params() -> StyxLspCompletionParams {
    StyxLspCompletionParams {
        document_uri: sample_styx_lsp_uri(),
        cursor: sample_styx_lsp_cursor(),
        path: vec![
            "AllProducts".to_string(),
            "@query".to_string(),
            "select".to_string(),
        ],
        prefix: "na".to_string(),
        context: Some(sample_styx_value()),
        tagged_context: Some(sample_styx_value()),
    }
}

fn sample_styx_lsp_completions() -> Vec<StyxLspCompletionItem> {
    vec![
        StyxLspCompletionItem {
            label: "name".to_string(),
            detail: Some("TEXT".to_string()),
            documentation: Some("Product display name".to_string()),
            kind: Some(StyxLspCompletionKind::Field),
            sort_text: Some("0001".to_string()),
            insert_text: None,
        },
        StyxLspCompletionItem {
            label: "metadata".to_string(),
            detail: Some("JSONB".to_string()),
            documentation: None,
            kind: Some(StyxLspCompletionKind::Field),
            sort_text: Some("0002".to_string()),
            insert_text: Some("metadata".to_string()),
        },
    ]
}

fn sample_styx_lsp_hover_params() -> StyxLspHoverParams {
    StyxLspHoverParams {
        document_uri: sample_styx_lsp_uri(),
        cursor: sample_styx_lsp_cursor(),
        path: vec![
            "AllProducts".to_string(),
            "@query".to_string(),
            "from".to_string(),
        ],
        context: Some(sample_styx_value()),
        tagged_context: Some(sample_styx_value()),
    }
}

fn sample_styx_lsp_hover_result() -> StyxLspHoverResult {
    StyxLspHoverResult {
        contents: "**products** table\n\nBacked by `Product`.".to_string(),
        range: Some(StyxLspRange {
            start: StyxLspPosition {
                line: 0,
                character: 14,
            },
            end: StyxLspPosition {
                line: 0,
                character: 22,
            },
        }),
    }
}

fn sample_styx_lsp_inlay_hint_params() -> StyxLspInlayHintParams {
    StyxLspInlayHintParams {
        document_uri: sample_styx_lsp_uri(),
        range: sample_styx_lsp_range(),
        context: Some(sample_styx_value()),
    }
}

fn sample_styx_lsp_inlay_hints() -> Vec<StyxLspInlayHint> {
    vec![StyxLspInlayHint {
        position: StyxLspPosition {
            line: 0,
            character: 9,
        },
        label: "Product".to_string(),
        kind: Some(StyxLspInlayHintKind::Type),
        padding_left: true,
        padding_right: false,
    }]
}

fn sample_styx_lsp_diagnostic() -> StyxLspDiagnostic {
    StyxLspDiagnostic {
        span: StyxSpan { start: 23, end: 29 },
        severity: StyxLspDiagnosticSeverity::Warning,
        message: "column `legacy` is deprecated".to_string(),
        source: Some("dibs".to_string()),
        code: Some("deprecated-column".to_string()),
        data: Some(sample_styx_value()),
    }
}

fn sample_styx_lsp_diagnostic_params() -> StyxLspDiagnosticParams {
    StyxLspDiagnosticParams {
        document_uri: sample_styx_lsp_uri(),
        tree: sample_styx_value(),
        content: sample_styx_lsp_source(),
    }
}

fn sample_styx_lsp_diagnostics() -> Vec<StyxLspDiagnostic> {
    vec![sample_styx_lsp_diagnostic()]
}

fn sample_styx_lsp_code_action_params() -> StyxLspCodeActionParams {
    StyxLspCodeActionParams {
        document_uri: sample_styx_lsp_uri(),
        span: StyxSpan { start: 23, end: 29 },
        diagnostics: sample_styx_lsp_diagnostics(),
    }
}

fn sample_styx_lsp_code_actions() -> Vec<StyxLspCodeAction> {
    vec![StyxLspCodeAction {
        title: "Replace legacy column".to_string(),
        kind: Some(StyxLspCodeActionKind::QuickFix),
        edit: Some(StyxLspWorkspaceEdit {
            changes: vec![StyxLspDocumentEdit {
                uri: sample_styx_lsp_uri(),
                edits: vec![StyxLspTextEdit {
                    span: StyxSpan { start: 23, end: 29 },
                    new_text: "name".to_string(),
                }],
            }],
        }),
        is_preferred: true,
    }]
}

fn sample_styx_lsp_definition_params() -> StyxLspDefinitionParams {
    StyxLspDefinitionParams {
        document_uri: sample_styx_lsp_uri(),
        cursor: sample_styx_lsp_cursor(),
        path: vec![
            "AllProducts".to_string(),
            "@query".to_string(),
            "from".to_string(),
        ],
        context: Some(sample_styx_value()),
        tagged_context: Some(sample_styx_value()),
    }
}

fn sample_styx_lsp_locations() -> Vec<StyxLspLocation> {
    vec![StyxLspLocation {
        uri: "file:///workspace/schema.styx".to_string(),
        span: StyxSpan {
            start: 120,
            end: 128,
        },
    }]
}

fn sample_styx_lsp_schema_info() -> StyxLspSchemaInfo {
    StyxLspSchemaInfo {
        source: "@schema { @ @object{ name @string } }".to_string(),
        uri: "styx-embedded://crate:dibs-queries@1".to_string(),
    }
}

fn sample_styx_lsp_surface_fixture() -> StyxLspSurfaceFixture {
    StyxLspSurfaceFixture {
        initialize_params: sample_styx_lsp_initialize_params(),
        initialize_result: sample_styx_lsp_initialize_result(),
        completion_params: sample_styx_lsp_completion_params(),
        completions: sample_styx_lsp_completions(),
        hover_params: sample_styx_lsp_hover_params(),
        hover_result: Some(sample_styx_lsp_hover_result()),
        inlay_hint_params: sample_styx_lsp_inlay_hint_params(),
        inlay_hints: sample_styx_lsp_inlay_hints(),
        diagnostic_params: sample_styx_lsp_diagnostic_params(),
        diagnostics: sample_styx_lsp_diagnostics(),
        code_action_params: sample_styx_lsp_code_action_params(),
        code_actions: sample_styx_lsp_code_actions(),
        definition_params: sample_styx_lsp_definition_params(),
        locations: sample_styx_lsp_locations(),
        get_subtree_params: StyxLspGetSubtreeParams {
            document_uri: sample_styx_lsp_uri(),
            path: vec!["AllProducts".to_string(), "@query".to_string()],
        },
        subtree: Some(sample_styx_value()),
        get_document_params: StyxLspGetDocumentParams {
            document_uri: sample_styx_lsp_uri(),
        },
        document: Some(sample_styx_value()),
        get_source_params: StyxLspGetSourceParams {
            document_uri: sample_styx_lsp_uri(),
        },
        source: Some(sample_styx_lsp_source()),
        get_schema_params: StyxLspGetSchemaParams {
            document_uri: sample_styx_lsp_uri(),
        },
        schema: Some(sample_styx_lsp_schema_info()),
        offset_to_position_params: StyxLspOffsetToPositionParams {
            document_uri: sample_styx_lsp_uri(),
            offset: 16,
        },
        position: Some(sample_styx_lsp_position()),
        position_to_offset_params: StyxLspPositionToOffsetParams {
            document_uri: sample_styx_lsp_uri(),
            position: sample_styx_lsp_position(),
        },
        offset: Some(16),
    }
}

fn sample_stax_linux_broker_control_fixture() -> StaxLinuxBrokerControlFixture {
    StaxLinuxBrokerControlFixture {
        config: StaxLinuxPerfSessionConfig {
            target_pid: 42_424,
            frequency_hz: 997,
            kernel_stacks: true,
            request_waking: true,
            request_pmu: true,
            request_dwarf_unwind: false,
        },
        status: StaxLinuxDaemonStatus {
            version: "1.0.0-dev".to_string(),
            host_arch: "x86_64".to_string(),
            privileged: true,
            perf_event_paranoid: 1,
        },
        errors: vec![
            StaxLinuxPerfSessionError::NotPrivileged {
                detail: "perf_event_paranoid=3 without CAP_PERFMON".to_string(),
            },
            StaxLinuxPerfSessionError::PerfEventOpen {
                cpu: 3,
                errno: 24,
                detail: "EMFILE while opening PMU sibling".to_string(),
            },
            StaxLinuxPerfSessionError::NoSuchTarget(99_999),
            StaxLinuxPerfSessionError::NotAuthorized {
                caller_uid: 501,
                target_uid: 0,
            },
        ],
        waking_field_offsets: Some(StaxLinuxWakingFieldOffsets {
            wakee_pid_offset: 16,
            wakee_pid_size: 4,
        }),
    }
}

fn sample_stax_mac_record_fixture() -> StaxMacRecordFixture {
    StaxMacRecordFixture {
        config: StaxMacSessionConfig {
            target_pid: 42_424,
            frequency_hz: 997,
            buf_records: 1_048_576,
            samplers: 0x1 | 0x2 | 0x10,
            pmu_event_configs: vec![0xfeed_beef, 0x1_0000_0001],
            class_mask: 0b1011,
            filter_range_value1: 0x3100_0000,
            filter_range_value2: 0x31ff_ffff,
            typefilter_cscs: vec![0x3101, 0x3102, 0x3108],
        },
        batch: StaxMacKdBufBatch {
            records: vec![
                StaxMacKdBuf {
                    timestamp: 900_000,
                    arg1: 0x1000,
                    arg2: 0x2000,
                    arg3: 0x3000,
                    arg4: 0x4000,
                    arg5: 0xfeed_face,
                    debugid: 0x3101_0004,
                    cpuid: 3,
                    unused: 0,
                },
                StaxMacKdBuf {
                    timestamp: 900_128,
                    arg1: 0x1008,
                    arg2: 0x2008,
                    arg3: 0x3008,
                    arg4: 0x4008,
                    arg5: 0xfeed_face,
                    debugid: 0x3101_0008,
                    cpuid: 4,
                    unused: 0,
                },
            ],
            read_started_mach_ticks: 899_900,
            drained_mach_ticks: 900_140,
            queued_for_send_mach_ticks: 900_150,
            send_started_mach_ticks: 900_180,
            drained_at_unix_ns: 1_801_000_000_123_456_789,
        },
        result: StaxMacRecordResult::Err(StaxMacRecordError::Busy {
            holder_uid: 501,
            holder_pid: 12_345,
            since_unix_ns: 1_801_000_000_000_000_000,
        }),
        status: StaxMacDaemonStatus {
            version: "1.0.0-dev".to_string(),
            state: StaxMacSessionState::Recording {
                target_pid: 42_424,
                holder_uid: 501,
                holder_pid: 12_345,
                since_unix_ns: 1_801_000_000_000_000_000,
            },
            host_arch: "aarch64".to_string(),
        },
    }
}

fn sample_dibs_row(i: u32) -> DibsRow {
    DibsRow {
        fields: vec![
            RowField {
                name: "id".to_string(),
                value: SqlValue::I64(1_000 + i as i64),
            },
            RowField {
                name: "name".to_string(),
                value: SqlValue::String(format!("product-{i}")),
            },
            RowField {
                name: "metadata".to_string(),
                value: SqlValue::Bytes(vec![1, 3, 3, 7, (i & 0xff) as u8]),
            },
        ],
    }
}

fn sample_dibs_schema_info() -> DibsSchemaInfo {
    DibsSchemaInfo {
        tables: vec![DibsTableInfo {
            name: "products".to_string(),
            columns: vec![
                DibsColumnInfo {
                    name: "id".to_string(),
                    sql_type: "BIGINT".to_string(),
                    rust_type: Some("i64".to_string()),
                    nullable: false,
                    default: Some("generated always as identity".to_string()),
                    primary_key: true,
                    unique: true,
                    auto_generated: true,
                    long: false,
                    label: false,
                    enum_variants: vec![],
                    doc: Some("Primary key".to_string()),
                    lang: None,
                    icon: Some("key".to_string()),
                    subtype: None,
                },
                DibsColumnInfo {
                    name: "name".to_string(),
                    sql_type: "TEXT".to_string(),
                    rust_type: Some("String".to_string()),
                    nullable: false,
                    default: None,
                    primary_key: false,
                    unique: false,
                    auto_generated: false,
                    long: false,
                    label: true,
                    enum_variants: vec![],
                    doc: Some("Display name".to_string()),
                    lang: Some("en".to_string()),
                    icon: None,
                    subtype: None,
                },
            ],
            foreign_keys: vec![DibsForeignKeyInfo {
                columns: vec!["owner_id".to_string()],
                references_table: "users".to_string(),
                references_columns: vec!["id".to_string()],
            }],
            indices: vec![DibsIndexInfo {
                name: "products_name_idx".to_string(),
                columns: vec![DibsIndexColumnInfo {
                    name: "name".to_string(),
                    order: "ASC".to_string(),
                    nulls: "LAST".to_string(),
                }],
                unique: false,
                where_clause: Some("deleted_at IS NULL".to_string()),
            }],
            source_file: Some("crates/app/src/schema.rs".to_string()),
            source_line: Some(42),
            doc: Some("Products available in the catalog".to_string()),
            icon: Some("box".to_string()),
        }],
    }
}

fn sample_dibs_squel_service_fixture() -> DibsSquelServiceFixture {
    let row = sample_dibs_row(7);
    DibsSquelServiceFixture {
        schema: sample_dibs_schema_info(),
        list_request: DibsListRequest {
            table: "products".to_string(),
            filters: vec![DibsFilter {
                field: "name".to_string(),
                op: DibsFilterOp::ILike,
                value: SqlValue::String("phon%".to_string()),
                values: vec![
                    SqlValue::String("phon".to_string()),
                    SqlValue::String("vox".to_string()),
                ],
            }],
            sort: vec![DibsSort {
                field: "created_at".to_string(),
                dir: DibsSortDir::Desc,
            }],
            limit: Some(50),
            offset: Some(100),
            select: vec!["id".to_string(), "name".to_string(), "metadata".to_string()],
        },
        list_response: DibsListResult::Ok(DibsServiceListResponse {
            rows: vec![row.clone(), sample_dibs_row(8)],
            total: Some(2),
        }),
        get_request: DibsGetRequest {
            table: "products".to_string(),
            pk: SqlValue::I64(1_007),
        },
        get_response: DibsGetResult::Ok(Some(row.clone())),
        create_request: DibsCreateRequest {
            table: "products".to_string(),
            data: row.clone(),
        },
        create_response: DibsRowResult::Ok(row.clone()),
        update_request: DibsUpdateRequest {
            table: "products".to_string(),
            pk: SqlValue::I64(1_007),
            data: row.clone(),
        },
        update_response: DibsRowResult::Err(DibsError::UnknownColumn("legacy_name".to_string())),
        delete_request: DibsDeleteRequest {
            table: "products".to_string(),
            pk: SqlValue::I64(1_007),
        },
        delete_response: DibsDeleteResult::Ok(1),
    }
}

fn sample_dibs_migration_service_fixture() -> DibsMigrationServiceFixture {
    DibsMigrationServiceFixture {
        status_request: DibsMigrationStatusRequest {
            database_url: "postgres://localhost/app".to_string(),
        },
        status_response: DibsMigrationStatusResult::Ok(vec![
            DibsMigrationInfo {
                version: "202606050701_add_trace_indexes".to_string(),
                name: "add trace indexes".to_string(),
                applied: true,
                applied_at: Some("2026-06-05T07:03:22Z".to_string()),
                source_file: Some(
                    "crates/app-db/src/migrations/m202606050701_add_trace_indexes.rs".to_string(),
                ),
                source: Some("CREATE INDEX trace_idx ON trace_event (trace_id);".to_string()),
            },
            DibsMigrationInfo {
                version: "202606051142_add_dibs_admin_views".to_string(),
                name: "add dibs admin views".to_string(),
                applied: false,
                applied_at: None,
                source_file: Some(
                    "crates/app-db/src/migrations/m202606051142_add_dibs_admin_views.rs"
                        .to_string(),
                ),
                source: None,
            },
        ]),
        migrate_request: DibsMigrateRequest {
            database_url: "postgres://localhost/app".to_string(),
            migration: None,
        },
        migrate_response: DibsMigrateCallResult::Ok(DibsMigrateResult {
            total_defined: 2,
            already_applied: vec![DibsAppliedMigration {
                version: "202606050701_add_trace_indexes".to_string(),
                applied_at: "2026-06-05T07:03:22Z".to_string(),
            }],
            applied: vec![DibsRanMigration {
                version: "202606051142_add_dibs_admin_views".to_string(),
                duration_ms: 38,
            }],
            setup_ms: 7,
            total_time_ms: 45,
        }),
        log_item: DibsMigrationLog {
            level: DibsLogLevel::Info,
            message: "Applied 202606051142_add_dibs_admin_views (38ms)".to_string(),
            migration: Some("202606051142_add_dibs_admin_views".to_string()),
        },
    }
}

fn required_field(name: &str, schema: SchemaRef) -> Field {
    Field {
        name: name.to_string(),
        schema,
        required: true,
    }
}

fn temporary_ref(id: u64) -> SchemaRef {
    SchemaRef::concrete(SchemaId(id))
}

fn primitive_ref(primitive: Primitive) -> SchemaRef {
    SchemaRef::concrete(phon_schema::primitive_id(primitive))
}

fn stax_external_fd_registry() -> (Registry, SchemaId, SchemaId) {
    const FD: u64 = 1;
    const LIST_FD: u64 = 2;
    const LIST_U64: u64 = 3;
    const WAKING_OFFSETS: u64 = 4;
    const OPTION_WAKING_OFFSETS: u64 = 5;
    const PERF_SESSION_FDS: u64 = 6;

    let schemas = phon_schema::resolve_ids(vec![
        Schema {
            id: SchemaId(FD),
            type_params: Vec::new(),
            kind: SchemaKind::External {
                kind: "fd".to_string(),
                metadata: None,
            },
        },
        Schema {
            id: SchemaId(LIST_FD),
            type_params: Vec::new(),
            kind: SchemaKind::List {
                element: temporary_ref(FD),
            },
        },
        Schema {
            id: SchemaId(LIST_U64),
            type_params: Vec::new(),
            kind: SchemaKind::List {
                element: primitive_ref(Primitive::U64),
            },
        },
        Schema {
            id: SchemaId(WAKING_OFFSETS),
            type_params: Vec::new(),
            kind: SchemaKind::Struct {
                name: "StaxLinuxWakingFieldOffsets".to_string(),
                fields: vec![
                    required_field("wakee_pid_offset", primitive_ref(Primitive::U32)),
                    required_field("wakee_pid_size", primitive_ref(Primitive::U32)),
                ],
            },
        },
        Schema {
            id: SchemaId(OPTION_WAKING_OFFSETS),
            type_params: Vec::new(),
            kind: SchemaKind::Option {
                element: temporary_ref(WAKING_OFFSETS),
            },
        },
        Schema {
            id: SchemaId(PERF_SESSION_FDS),
            type_params: Vec::new(),
            kind: SchemaKind::Struct {
                name: "StaxLinuxPerfSessionFds".to_string(),
                fields: vec![
                    required_field("sampling", temporary_ref(LIST_FD)),
                    required_field("switch", temporary_ref(LIST_FD)),
                    required_field("waking", temporary_ref(LIST_FD)),
                    required_field("waking_field_offsets", temporary_ref(OPTION_WAKING_OFFSETS)),
                    required_field("pmu", temporary_ref(LIST_FD)),
                    required_field("pmu_ids", temporary_ref(LIST_U64)),
                    required_field("pmu_per_cpu", primitive_ref(Primitive::U32)),
                    required_field("cpu_count", primitive_ref(Primitive::U32)),
                    required_field("page_size", primitive_ref(Primitive::U32)),
                    required_field("data_pages", primitive_ref(Primitive::U32)),
                    required_field("target_pid", primitive_ref(Primitive::U32)),
                    required_field("frequency_hz", primitive_ref(Primitive::U32)),
                    required_field("kernel_stacks", primitive_ref(Primitive::Bool)),
                ],
            },
        },
    ]);

    let fd_root = schemas[0].id;
    let fds_root = schemas[5].id;
    (Registry::new(schemas), fd_root, fds_root)
}

fn transport_capability_boundary_registry() -> (
    Registry,
    SchemaId,
    SchemaId,
    SchemaId,
    SchemaId,
    SchemaId,
    SchemaId,
) {
    const WRITER_ITEM: u64 = 1;
    const READER_ITEM: u64 = 2;
    const CHANNEL: u64 = 3;
    const WRITER_METADATA: u64 = 4;
    const READER_METADATA: u64 = 5;
    const EXTERNAL: u64 = 6;

    let schemas = phon_schema::resolve_ids(vec![
        Schema {
            id: SchemaId(WRITER_ITEM),
            type_params: Vec::new(),
            kind: SchemaKind::Struct {
                name: "DodecaTunnelItem".to_string(),
                fields: vec![
                    required_field("seq", primitive_ref(Primitive::U64)),
                    required_field("chunk_len", primitive_ref(Primitive::U32)),
                    required_field("transient_id", primitive_ref(Primitive::U64)),
                ],
            },
        },
        Schema {
            id: SchemaId(READER_ITEM),
            type_params: Vec::new(),
            kind: SchemaKind::Struct {
                name: "DodecaTunnelItem".to_string(),
                fields: vec![
                    required_field("seq", primitive_ref(Primitive::U64)),
                    required_field("chunk_len", primitive_ref(Primitive::U32)),
                ],
            },
        },
        Schema {
            id: SchemaId(CHANNEL),
            type_params: Vec::new(),
            kind: SchemaKind::Channel {
                direction: ChannelDirection::Tx,
                element: temporary_ref(WRITER_ITEM),
            },
        },
        Schema {
            id: SchemaId(WRITER_METADATA),
            type_params: Vec::new(),
            kind: SchemaKind::Struct {
                name: "StaxFdMetadata".to_string(),
                fields: vec![
                    required_field("path", primitive_ref(Primitive::String)),
                    required_field("flags", primitive_ref(Primitive::U32)),
                    required_field("probe_id", primitive_ref(Primitive::U64)),
                ],
            },
        },
        Schema {
            id: SchemaId(READER_METADATA),
            type_params: Vec::new(),
            kind: SchemaKind::Struct {
                name: "StaxFdMetadata".to_string(),
                fields: vec![
                    required_field("path", primitive_ref(Primitive::String)),
                    required_field("flags", primitive_ref(Primitive::U32)),
                ],
            },
        },
        Schema {
            id: SchemaId(EXTERNAL),
            type_params: Vec::new(),
            kind: SchemaKind::External {
                kind: "fd".to_string(),
                metadata: Some(temporary_ref(WRITER_METADATA)),
            },
        },
    ]);

    let writer_item = schemas[0].id;
    let reader_item = schemas[1].id;
    let channel = schemas[2].id;
    let writer_metadata = schemas[3].id;
    let reader_metadata = schemas[4].id;
    let external = schemas[5].id;

    (
        Registry::new(schemas),
        writer_item,
        reader_item,
        channel,
        writer_metadata,
        reader_metadata,
        external,
    )
}

#[test]
// r[verify type-system.dynamic]
// r[verify type-system.variant-payloads]
fn dodeca_dynamic_template_call_roundtrips() {
    let report = roundtrip(DodecaTemplateCall {
        context_id: "ctx-1".to_string(),
        name: "get_section".to_string(),
        args: vec![dynamic_object(), Value::from("docs")],
        kwargs: vec![("path".to_string(), Value::from("/guide/"))],
    });

    expect_native_clean_when_jit_available(
        report,
        "Dodeca dynamic values should be native-clean when dynamic Value stencils are supported",
    );
}

#[test]
// r[verify type-system.dynamic]
// r[verify type-system.variant-payloads]
// r[verify exec.jit-optional]
fn dodeca_load_data_result_with_dynamic_value_roundtrips() {
    let report = roundtrip(sample_dodeca_load_data_result());

    expect_native_clean_when_jit_available(
        report,
        "Dodeca data-loader result should stay native-clean with dynamic Value payloads",
    );
}

#[test]
// r[verify descriptors.thunk-binding]
// r[verify type-system.dynamic]
// r[verify type-system.variant-payloads]
// r[verify exec.jit-optional]
fn dodeca_markdown_parse_result_with_boxed_source_map_roundtrips() {
    let report = roundtrip(sample_dodeca_parse_result());

    expect_native_clean_when_jit_available(
        report,
        "Dodeca parse/render result with boxed source map should stay native-clean",
    );
}

#[test]
// r[verify descriptors.fact-driven]
// r[verify type-system.variant-payloads]
// r[verify typed.no-dynamic-bounce]
// r[verify exec.jit-optional]
fn dodeca_image_processor_payloads_roundtrip() {
    let report = roundtrip(sample_dodeca_image_processor_fixture());

    expect_native_clean_when_jit_available(
        report,
        "Dodeca image processor roots should stay native-clean",
    );
}

#[test]
// r[verify descriptors.fact-driven]
// r[verify type-system.variant-payloads]
// r[verify typed.no-dynamic-bounce]
// r[verify exec.jit-optional]
fn dodeca_search_indexer_payloads_roundtrip() {
    let report = roundtrip(sample_dodeca_search_indexer_fixture());

    expect_native_clean_when_jit_available(
        report,
        "Dodeca search indexer roots should stay native-clean",
    );
}

#[test]
// r[verify descriptors.fact-driven]
// r[verify type-system.variant-payloads]
// r[verify typed.no-dynamic-bounce]
// r[verify exec.jit-optional]
fn dodeca_asset_processing_payloads_roundtrip() {
    let report = roundtrip(sample_dodeca_asset_processing_fixture());

    expect_native_clean_when_jit_available(
        report,
        "Dodeca CSS/SASS/SVGO asset-processing roots should stay native-clean",
    );
}

#[test]
// r[verify descriptors.fact-driven]
// r[verify type-system.variant-payloads]
// r[verify typed.no-dynamic-bounce]
// r[verify exec.jit-optional]
fn dodeca_small_cell_service_payloads_roundtrip() {
    let report = roundtrip(sample_dodeca_small_cell_services_fixture());

    expect_native_clean_when_jit_available(
        report,
        "Dodeca small-cell lifecycle/minify/js/html-diff/font/image/dialog/tui roots should stay native-clean",
    );
}

#[test]
// r[verify compat.type-match]
// r[verify descriptors.thunk-binding]
// r[verify validate.uniqueness]
fn dodeca_html_maps_sets_and_tuple_vectors_roundtrip() {
    let mut path_map = HashMap::new();
    path_map.insert("/old.css".to_string(), "/assets/new.css".to_string());

    let mut known_routes = HashSet::new();
    known_routes.insert("/".to_string());
    known_routes.insert("/guide/".to_string());

    let mut code_metadata = HashMap::new();
    code_metadata.insert(
        "sample.rs".to_string(),
        CodeExecutionMetadata {
            language: "rust".to_string(),
            dependencies: vec![ResolvedDependency {
                name: "facet".to_string(),
                version: Some("0.29".to_string()),
            }],
            duration_ms: 12,
        },
    );

    let mut image_variants = HashMap::new();
    image_variants.insert(
        "/hero.png".to_string(),
        ResponsiveImageInfo {
            jxl_srcset: vec![("/hero-640.jxl".to_string(), 640)],
            webp_srcset: vec![("/hero-640.webp".to_string(), 640)],
        },
    );

    let mut vite_css_map = HashMap::new();
    vite_css_map.insert(
        "/entry.ts".to_string(),
        vec![
            "/assets/entry.css".to_string(),
            "/assets/chunk.css".to_string(),
        ],
    );

    let mut mounted_routes = HashSet::new();
    mounted_routes.insert("/wiki/".to_string());
    mounted_routes.insert("/wiki/exec/".to_string());

    let report = roundtrip(DodecaHtmlProcessInput {
        html: "<main><img src=\"/hero.png\"></main>".to_string(),
        path_map: Some(path_map),
        known_routes: Some(known_routes),
        code_metadata: Some(code_metadata),
        injections: vec![Injection {
            location: InjectionLocation::Head,
            content: "<meta charset=\"utf-8\">".to_string(),
        }],
        image_variants: Some(image_variants),
        vite_css_map: Some(vite_css_map),
        mount: Some(MountLocalization {
            segment: "wiki".to_string(),
            routes: mounted_routes,
        }),
    });

    expect_native_clean_when_jit_available(
        report,
        "Dodeca maps/sets/tuple-vector roots should be native-clean once set stencils are supported",
    );
}

#[test]
// r[verify type-system.variant-payloads]
fn dibs_sql_value_rows_roundtrip() {
    let report = roundtrip(DibsListResponse {
        rows: vec![vec![
            RowField {
                name: "id".to_string(),
                value: SqlValue::I64(123),
            },
            RowField {
                name: "payload".to_string(),
                value: SqlValue::Bytes(vec![1, 2, 3, 5, 8]),
            },
        ]],
        total: Some(1),
    });

    expect_native_clean_when_jit_available(
        report,
        "Dibs SQL values are ordinary Facet enum/struct shapes",
    );
}

#[test]
// r[verify type-system.variant-payloads]
// r[verify codegen.schema-is-source-of-truth]
fn dibs_squel_service_payloads_roundtrip() {
    let report = roundtrip(sample_dibs_squel_service_fixture());

    expect_native_clean_when_jit_available(
        report,
        "Dibs generated service roots should stay native-clean for schema/list/crud/result payloads",
    );
}

#[test]
// r[verify descriptors.fact-driven]
// r[verify type-system.variant-payloads]
// r[verify type-system.channel]
// r[verify exec.jit-optional]
fn dibs_migration_service_payloads_roundtrip() {
    let report = roundtrip(sample_dibs_migration_service_fixture());

    expect_native_clean_when_jit_available(
        report,
        "Dibs migration status/migrate roots and migration-log channel items should stay native-clean",
    );
}

#[test]
// r[verify descriptors.fact-driven]
// r[verify ir.memory]
fn styx_recursive_value_roundtrips() {
    let report = roundtrip(sample_styx_value());

    expect_native_clean_when_jit_available(
        report,
        "recursive Styx-shaped values should be native-clean when recursive blocks are supported",
    );
}

#[test]
// r[verify descriptors.fact-driven]
// r[verify ir.memory]
fn styx_lsp_surface_roundtrips() {
    let report = roundtrip(sample_styx_lsp_surface_fixture());

    expect_native_clean_when_jit_available(
        report,
        "Styx LSP extension and host callback DTOs should stay on the same typed engine/JIT path as recursive values",
    );
}

#[test]
// r[verify descriptors.fact-driven]
// r[verify ir.memory]
fn stax_flamegraph_update_roundtrips() {
    let update = StaxFlamegraphUpdate {
        total_on_cpu_ns: 9_000,
        strings: vec![
            "main".to_string(),
            "poll".to_string(),
            "libbee.dylib".to_string(),
        ],
        root: FlameNode {
            address: 0x1000,
            function_name: Some(0),
            binary: Some(2),
            on_cpu_ns: 9_000,
            off_cpu: OffCpuBreakdown {
                sleep_ns: 100,
                io_ns: 200,
                mutex_ns: 300,
            },
            children: vec![FlameNode {
                address: 0x1040,
                function_name: Some(1),
                binary: Some(2),
                on_cpu_ns: 4_500,
                off_cpu: OffCpuBreakdown {
                    sleep_ns: 10,
                    io_ns: 20,
                    mutex_ns: 30,
                },
                children: Vec::new(),
            }],
        },
    };

    let report = roundtrip(update);

    expect_native_clean_when_jit_available(
        report,
        "recursive Stax-shaped flamegraphs should be native-clean when recursive blocks are supported",
    );
}

#[test]
// r[verify type-system.variant-payloads]
fn stax_linux_fd_broker_control_surface_roundtrips() {
    let report = roundtrip(sample_stax_linux_broker_control_fixture());

    expect_native_clean_when_jit_available(
        report,
        "Stax Linux fd-broker config/status/error DTOs are ordinary payload data",
    );
}

#[test]
// r[verify descriptors.fact-driven]
// r[verify type-system.channel]
// r[verify type-system.variant-payloads]
// r[verify exec.jit-optional]
fn stax_macos_kdbuf_batch_stream_item_roundtrips() {
    let report = roundtrip(sample_stax_mac_record_fixture());

    expect_native_clean_when_jit_available(
        report,
        "Stax macOS KdBufBatch stream item and record/status DTOs should stay native-clean",
    );
}

#[test]
// r[verify type-system.external]
fn stax_fd_capabilities_are_external_and_explicitly_unsupported_in_payload_codec() {
    let (registry, fd_root, fds_root) = stax_external_fd_registry();

    let encode_err = compact::to_bytes(&Value::from(0u64), fd_root, &registry)
        .expect_err("external fd handles must not encode as ordinary payload data");
    assert_eq!(encode_err, CompactError::Unsupported("external"));

    let decode_err = compact::from_bytes(&0u64.to_le_bytes(), fd_root, &registry)
        .expect_err("external fd handles must not decode as ordinary payload data");
    assert_eq!(decode_err, CompactError::Unsupported("external"));

    let plan_err = match plan::build_plan(fds_root, fds_root, &registry) {
        Ok(_) => panic!("PerfSessionFds planning must surface the external capability hole"),
        Err(err) => err,
    };
    assert_eq!(plan_err, CompactError::Unsupported("external"));
}

#[test]
// r[verify compat.type-match]
// r[verify type-system.channel]
// r[verify type-system.external]
fn transport_capability_roots_are_not_payloads_but_items_and_metadata_use_compat() {
    let (
        registry,
        writer_item,
        reader_item,
        channel_root,
        writer_metadata,
        reader_metadata,
        external_root,
    ) = transport_capability_boundary_registry();

    let channel_err = match plan::build_plan(channel_root, channel_root, &registry) {
        Ok(_) => panic!("channel roots must stay transport capabilities, not payload plans"),
        Err(err) => err,
    };
    assert_eq!(channel_err, CompactError::Unsupported("channel"));

    let external_err = match plan::build_plan(external_root, external_root, &registry) {
        Ok(_) => panic!("external roots must stay transport capabilities, not payload plans"),
        Err(err) => err,
    };
    assert_eq!(external_err, CompactError::Unsupported("external"));

    let item_wire = compact::to_bytes(
        &value_object(&[
            ("seq", Value::from(7u64)),
            ("chunk_len", Value::from(128u32)),
            ("transient_id", Value::from(99u64)),
        ]),
        writer_item,
        &registry,
    )
    .unwrap();
    let item = plan::decode(&item_wire, writer_item, reader_item, &registry).unwrap();
    assert_eq!(
        item,
        value_object(&[
            ("seq", Value::from(7u64)),
            ("chunk_len", Value::from(128u32)),
        ])
    );

    let metadata_wire = compact::to_bytes(
        &value_object(&[
            ("path", Value::from("/proc/self/fd/7")),
            ("flags", Value::from(0x800u32)),
            ("probe_id", Value::from(44u64)),
        ]),
        writer_metadata,
        &registry,
    )
    .unwrap();
    let metadata =
        plan::decode(&metadata_wire, writer_metadata, reader_metadata, &registry).unwrap();
    assert_eq!(
        metadata,
        value_object(&[
            ("path", Value::from("/proc/self/fd/7")),
            ("flags", Value::from(0x800u32)),
        ])
    );
}

#[test]
// r[verify type-system.variant-payloads]
fn hotmeal_live_reload_surface_roundtrips() {
    let report = roundtrip(HotmealLiveReloadFixture {
        subscribe: HotmealSubscribeRequest {
            route: "/guide/".to_string(),
        },
        events: vec![
            HotmealLiveReloadEvent::Reload,
            HotmealLiveReloadEvent::Patches {
                route: "/guide/".to_string(),
                patches_blob: vec![0xde, 0xad, 0xbe, 0xef],
            },
            HotmealLiveReloadEvent::HeadChanged {
                route: "/guide/".to_string(),
            },
        ],
    });

    expect_native_clean_when_jit_available(
        report,
        "Hotmeal live-reload payloads should be native-clean small Vox messages",
    );
}

#[test]
// r[verify descriptors.fact-driven]
// r[verify ir.memory]
fn helix_trace_server_payloads_roundtrip() {
    let pulse_id = HelixSchedulerPulseId(17);
    let audio_range = HelixAudioTokenRange {
        start: HelixAudioTokenId(32),
        end: HelixAudioTokenId(40),
    };

    let report = roundtrip(HelixTraceSnapshot {
        meta: HelixStreamMeta {
            schema_version: 1,
            pulse_ids: vec![HelixSchedulerPulseId(16), pulse_id],
            timeline_event_count: 420,
            attention_batch_count: 17,
        },
        run_info: HelixRunInfo {
            backend: "metal".to_string(),
            model_dir: "/weights/qwen3-asr".to_string(),
            input: "/audio/sample.wav".to_string(),
            piece: Some("ceramic".to_string()),
            pulse_ms: 120,
            audio_ring_capacity: 512,
            text_ring_capacity: 256,
            commit_revisable_tail_text_tokens: 8,
            revise_logit_margin: 1.5,
            sample_rate: 16_000,
            mel_hop_samples: 160,
            num_mel_bins: 128,
            num_mel_frames: 2_048,
            audio_tokens_per_chunk: 8,
            native_window_tokens: 64,
            realtime_pacing: true,
            profile_phases: false,
            attention_trace_schema_version: 2,
            trace_server_schema_version: 1,
        },
        rollup: Some(HelixPulseRollup {
            pulse_id,
            pulse_start_us: Some(1_000_000),
            pulse_duration_us: Some(44_000),
            encoder_duration_us: Some(12_000),
            refresh_duration_us: Some(8_000),
            verify_duration_us: Some(4_000),
            decode_duration_us: Some(16_000),
            commit_duration_us: Some(1_000),
            pulse_mel_frames: 24,
            committed_tokens: 3,
            retained_speculative_tokens: 5,
            resident_committed_tokens: 80,
            evicted_audio_tokens: 2,
            evicted_committed_tokens: 1,
            decoded_tokens: 6,
            hit_eos: false,
            verify: Some(HelixVerifyOutcome {
                rewind_k: 2,
                accepted_prefix_len: Some(3),
                divergence_row: Some(4),
                discarded_speculative_tokens: None,
            }),
            has_attention_batch: true,
            ar_token_count: 6,
        }),
        prompt_layout: Some(HelixPromptLayout {
            pulse_id,
            first_audio_token_id: audio_range.start,
            resident_audio_frames: 8,
            changed_audio_spans: vec![HelixAudioRepresentationSpan {
                audio: audio_range,
                audio_representation_version: HelixAudioRepresentationVersion(3),
            }],
            text_token_start: HelixTextTokenId(90),
            text_token_end: HelixTextTokenId(92),
            text_tokens: vec![
                HelixTextTokenSnapshot {
                    text_token_id: HelixTextTokenId(90),
                    text: Some("pho".to_string()),
                    text_before: Some("fo".to_string()),
                    in_verify_batch: true,
                    decoded_this_pulse: true,
                },
                HelixTextTokenSnapshot {
                    text_token_id: HelixTextTokenId(91),
                    text: Some("n".to_string()),
                    text_before: None,
                    in_verify_batch: false,
                    decoded_this_pulse: true,
                },
            ],
        }),
        attention_heatmap: Some(HelixPulseAttentionHeatmap {
            pulse_id,
            first_audio_token_id: audio_range.start,
            audio_token_count: 4,
            text_token_start: HelixTextTokenId(90),
            text_token_count: 2,
            record_count: 8,
            max_value: 0.75,
            mean_audio_mass: vec![0.1, 0.2, 0.3, 0.4, 0.05, 0.15, 0.25, 0.35],
            text_token_glyphs: vec!["pho".to_string(), "n".to_string()],
        }),
        stream_metrics: HelixStreamMetrics {
            pulse_ids: vec![HelixSchedulerPulseId(16), pulse_id],
            pulse_duration_us: vec![42_000, 44_000],
            decoded_tokens: vec![5, 6],
            committed_tokens: vec![2, 3],
            retained_speculative_tokens: vec![4, 5],
            evicted_audio_tokens: vec![0, 2],
            evicted_committed_tokens: vec![0, 1],
            rewind_k: vec![0, 2],
            ar_token_count: vec![5, 6],
            rolling_wer: vec![0.18, 0.16],
            s2d_p50_ms: vec![220.0, 210.0],
        },
        pulse_available: HelixPulseAvailable { pulse_id },
    });

    expect_native_clean_when_jit_available(
        report,
        "Helix trace-server payloads should stay native-clean for nested vectors/options/newtypes",
    );
}

#[test]
// r[verify descriptors.fact-driven]
// r[verify ir.memory]
// r[verify type-system.variant-payloads]
// r[verify type-system.dynamic]
fn helix_trace_service_surface_payloads_roundtrip() {
    let report = roundtrip(sample_helix_trace_service_surface());

    expect_native_clean_when_jit_available(
        report,
        "Helix trace-service endpoint payloads should stay native-clean across the live query surface",
    );
}

#[test]
// r[verify type-system.variant-payloads]
fn tracey_migration_payloads_roundtrip() {
    let rule_id = TraceyRuleId {
        base: "compat.plan-first".to_string(),
        version: 1,
    };
    let report = roundtrip(TraceyMigrationFixture {
        status: TraceyStatusResponse {
            impls: vec![TraceyImplStatus {
                spec: "phon".to_string(),
                impl_name: "rust".to_string(),
                total_rules: 69,
                covered_rules: 55,
                stale_rules: 0,
                verified_rules: 46,
            }],
        },
        uncovered_request: TraceyUncoveredRequest {
            spec: Some("phon".to_string()),
            impl_name: Some("rust".to_string()),
            prefix: Some("compat".to_string()),
        },
        uncovered_response: TraceyUncoveredResponse {
            spec: "phon".to_string(),
            impl_name: "rust".to_string(),
            total_rules: 69,
            uncovered_count: 1,
            by_section: vec![TraceySectionRules {
                section: "Compatibility".to_string(),
                rules: vec![TraceyRuleRef {
                    id: rule_id.clone(),
                    text: Some("Compatibility plans are built before decode.".to_string()),
                }],
            }],
        },
        data_update_item: TraceyDataUpdate {
            version: 42,
            delta: Some(TraceyDeltaSummary {
                newly_covered: vec![TraceyCoverageChange {
                    rule_id: rule_id.clone(),
                    file: "rust/phon/src/lib.rs".to_string(),
                    line: 10,
                }],
                newly_uncovered: vec![TraceyRuleId {
                    base: "type-system.channel".to_string(),
                    version: 1,
                }],
            }),
        },
        workspace_diagnostics: vec![TraceyLspFileDiagnostics {
            path: "docs/content/spec.md".to_string(),
            diagnostics: vec![TraceyLspDiagnostic {
                severity: "warning".to_string(),
                code: "stale".to_string(),
                message: "reference points to an older rule version".to_string(),
                start_line: 120,
                start_char: 4,
                end_line: 120,
                end_char: 22,
            }],
        }],
        workspace_symbols: vec![TraceyLspSymbol {
            name: rule_id.base,
            kind: "definition".to_string(),
            path: Some("docs/content/spec.md".to_string()),
            start_line: 1021,
            start_char: 2,
            end_line: 1021,
            end_char: 28,
        }],
    });

    expect_native_clean_when_jit_available(
        report,
        "Tracey migration DTOs should be native-clean fixed-width struct/list/option payloads",
    );
}

#[test]
// r[verify type-system.rust-subset]
fn native_sized_integers_are_fixed_width_on_the_wire() {
    let report = roundtrip(NativeSizedPayload {
        count: 0xCAFE_F00D,
        delta: -42,
        counts: vec![1, 2, 3, 5, 8, 13],
        maybe_delta: Some(-2048),
    });

    expect_native_clean_when_jit_available(
        report,
        "native-sized integer payloads should be native-clean once native-sized casts are supported",
    );

    let derived = phon::derive::of::<NativeSizedPayload>().unwrap();
    let root = derived
        .schemas
        .iter()
        .find(|schema| schema.id == derived.root)
        .expect("root schema should be present");
    let phon_schema::SchemaKind::Struct { fields, .. } = &root.kind else {
        panic!("native-sized payload should derive as a struct");
    };
    assert_eq!(
        fields[0].schema,
        phon_schema::SchemaRef::concrete(phon_schema::primitive_id(phon_schema::Primitive::U64)),
        "usize fields must use u64 wire schema"
    );
    assert_eq!(
        fields[1].schema,
        phon_schema::SchemaRef::concrete(phon_schema::primitive_id(phon_schema::Primitive::I64)),
        "isize fields must use i64 wire schema"
    );
}

#[test]
// r[verify validate.uniqueness]
fn set_decode_rejects_duplicate_elements() {
    let set_codec = Codec::<HashSet<String>>::new().expect("HashSet<String> should lower");
    let string_codec = Codec::<String>::new().expect("String should lower");
    let encoded_element = string_codec
        .encode(&"repeat".to_string())
        .expect("String should encode");

    let mut wire = 2u32.to_le_bytes().to_vec();
    wire.extend_from_slice(&encoded_element);
    wire.extend_from_slice(&encoded_element);

    let err = set_codec
        .decode(&wire)
        .expect_err("duplicate set elements must be rejected");
    assert!(
        matches!(
            err,
            phon::api::Error::Compact(phon_engine::CompactError::Decode(
                phon_schema::DecodeError::DuplicateElement
            ))
        ),
        "expected DuplicateElement, got {err:?}"
    );
}

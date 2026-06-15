#![deny(unsafe_code)]

pub mod evolved;

use std::collections::{BTreeMap, BTreeSet};

use facet::Facet;
use facet_value::Value;
use vox::service;
use vox::{Rx, Tx};

/// Testbed service for conformance testing.
///
/// Combines simple RPC, channeling, and complex type methods for comprehensive testing.
#[service]
pub trait Testbed {
    // ========================================================================
    // Simple RPC methods
    // ========================================================================

    /// Echoes the message back.
    async fn echo(&self, message: String) -> String;

    /// Returns the message reversed.
    async fn reverse(&self, message: String) -> String;

    // ========================================================================
    // Fallible methods (for testing User(E) error path)
    // ========================================================================

    /// Divides two numbers, returning an error if divisor is zero or would overflow.
    async fn divide(&self, dividend: i64, divisor: i64) -> Result<i64, MathError>;

    /// Looks up a user by ID.
    ///
    /// - IDs 1..=3: return Ok(Person)
    /// - IDs 100..=199: return Err(AccessDenied)
    /// - Anything else: return Err(NotFound)
    async fn lookup(&self, id: u32) -> Result<Person, LookupError>;

    // ========================================================================
    // Streaming methods
    // ========================================================================

    /// Client sends numbers, server returns their sum.
    ///
    /// Tests: client→server streaming. Server receives via `Rx<T>`, returns scalar.
    async fn sum(&self, numbers: Rx<i32>) -> i64;

    /// Server streams numbers back to client.
    ///
    /// Tests: server→client streaming. Server sends via `Tx<T>`.
    async fn generate(&self, count: u32, output: Tx<i32>);

    /// Bidirectional: client sends strings, server echoes each back.
    ///
    /// Tests: bidirectional streaming. Server receives via `Rx<T>`, sends via `Tx<T>`.
    async fn transform(&self, input: Rx<String>, output: Tx<String>);

    /// Dodeca-shaped byte tunnel: browser→cell bytes in, cell→browser bytes out.
    ///
    /// Mirrors `cell-http-proto::TcpTunnel::open` with direct non-nested channels.
    async fn dodeca_byte_tunnel(&self, inbound: Rx<Vec<u8>>, outbound: Tx<Vec<u8>>);

    /// Dodeca devtools LSP tunnel: browser JSON-RPC chunks in, LSP chunks out.
    ///
    /// Mirrors `dodeca_protocol::DevtoolsService::lsp` with string channels.
    async fn dodeca_devtools_lsp(
        &self,
        token: String,
        client_to_server: Rx<String>,
        server_to_client: Tx<String>,
    );

    /// Echo Dodeca browser devtools push events from `BrowserService::on_event`.
    async fn echo_dodeca_devtools_event(&self, event: DodecaDevtoolsEvent) -> DodecaDevtoolsEvent;

    /// Dibs schema metadata as returned by `DibsService::schema` / `SquelService::schema`.
    async fn dibs_schema(&self) -> DibsSchemaInfo;

    /// Dibs/Squel-shaped backoffice list query.
    async fn dibs_list(&self, request: DibsListRequest) -> Result<DibsListResponse, DibsError>;

    /// Dibs/Squel-shaped single-row lookup.
    async fn dibs_get(&self, request: DibsGetRequest) -> Result<Option<DibsRow>, DibsError>;

    /// Dibs/Squel-shaped row creation.
    async fn dibs_create(&self, request: DibsCreateRequest) -> Result<DibsRow, DibsError>;

    /// Dibs/Squel-shaped row update.
    async fn dibs_update(&self, request: DibsUpdateRequest) -> Result<DibsRow, DibsError>;

    /// Dibs/Squel-shaped row deletion.
    async fn dibs_delete(&self, request: DibsDeleteRequest) -> Result<u64, DibsError>;

    /// Dibs migration status query.
    async fn dibs_migration_status(
        &self,
        request: DibsMigrationStatusRequest,
    ) -> Result<Vec<DibsMigrationInfo>, DibsError>;

    /// Dibs migration runner with streamed migration logs.
    async fn dibs_migrate(
        &self,
        request: DibsMigrateRequest,
        logs: Tx<DibsMigrationLog>,
    ) -> Result<DibsMigrateResult, DibsError>;

    /// Server returns before streaming numbers back to the client.
    ///
    /// Tests: callee-held `Tx<T>` outlives the unary method response.
    async fn post_reply_generate(&self, output: Tx<i32>);

    /// Server returns before receiving numbers from the client, then reports their sum.
    ///
    /// Tests: callee-held `Rx<T>` outlives the unary method response.
    async fn post_reply_sum(&self, input: Rx<i32>, result: Tx<i64>);

    // ========================================================================
    // Complex type methods
    // ========================================================================

    /// Echo a point back.
    async fn echo_point(&self, point: Point) -> Point;

    /// Create a person and return it.
    async fn create_person(&self, name: String, age: u8, email: Option<String>) -> Person;

    /// Calculate the area of a rectangle.
    async fn rectangle_area(&self, rect: Rectangle) -> f64;

    /// Get a color by name.
    async fn parse_color(&self, name: String) -> Option<Color>;

    /// Calculate the area of a shape.
    async fn shape_area(&self, shape: Shape) -> f64;

    /// Create a canvas with given shapes.
    async fn create_canvas(&self, name: String, shapes: Vec<Shape>, background: Color) -> Canvas;

    /// Echo a deeply nested payload back unchanged.
    async fn echo_gnarly(&self, payload: GnarlyPayload) -> GnarlyPayload;

    /// Process a message and return a response.
    async fn process_message(&self, msg: Message) -> Message;

    /// Return multiple points.
    async fn get_points(&self, count: u32) -> Vec<Point>;

    /// Test tuple types.
    async fn swap_pair(&self, pair: (i32, String)) -> (String, i32);

    /// Echo raw bytes back. Tests Vec<u8> as a first-class arg/return type.
    async fn echo_bytes(&self, data: Vec<u8>) -> Vec<u8>;

    /// Echo a bool. Tests the bool primitive type.
    async fn echo_bool(&self, b: bool) -> bool;

    /// Echo a u64. Tests the u64 primitive type.
    async fn echo_u64(&self, n: u64) -> u64;

    /// Echo an optional string. Tests Option<String> directly.
    async fn echo_option_string(&self, s: Option<String>) -> Option<String>;

    /// Sum a large stream (tests channel credit/backpressure for > initial credit).
    ///
    /// Tests: channel flow control when sender must wait for credit grants.
    async fn sum_large(&self, numbers: Rx<i32>) -> i64;

    /// Generate a large stream (tests Tx backpressure with > initial credit items).
    ///
    /// Tests: server must wait for client to grant credit mid-stream.
    async fn generate_large(&self, count: u32, output: Tx<i32>);

    /// Return all three Color variants in a Vec, testing enum + vec round-trip.
    async fn all_colors(&self) -> Vec<Color>;

    /// Accept multiple args of different types; return a summary struct.
    /// Tests multi-arg encoding and struct return.
    async fn describe_point(&self, label: String, x: i32, y: i32, active: bool) -> TaggedPoint;

    /// Echo a nested enum back unchanged. Tests deep enum encoding.
    async fn echo_shape(&self, shape: Shape) -> Shape;

    /// Echo a status back. Tests simple enum with unit variants.
    async fn echo_status_v1(&self, status: Status) -> Status;

    /// Echo a tag back. Tests struct with String + u32 + String fields.
    async fn echo_tag_v1(&self, tag: Tag) -> Tag;

    // ========================================================================
    // Schema evolution methods
    // ========================================================================

    /// Echo a profile back. Tests added optional field.
    async fn echo_profile(&self, profile: Profile) -> Profile;

    /// Echo a record back. Tests field reordering.
    async fn echo_record(&self, record: Record) -> Record;

    /// Echo a status back. Tests added enum variant.
    async fn echo_status(&self, status: Status) -> Status;

    /// Echo a tag back. Tests removed field (v2 drops a field v1 has).
    async fn echo_tag(&self, tag: Tag) -> Tag;

    /// Echo a measurement back. Tests incompatible type change.
    async fn echo_measurement(&self, m: Measurement) -> Measurement;

    /// Echo a config back. Tests missing required field.
    async fn echo_config(&self, c: Config) -> Config;

    /// Echo a recursive tree back unchanged. Tests typed-VM recursion
    /// (`Access::Recurse` / `CallBlock`) end to end across the wire.
    async fn echo_tree(&self, tree: Tree) -> Tree;

    /// Echo a representative ecosystem bridge payload with maps, sets, tuple
    /// vectors, and byte blobs.
    async fn echo_ecosystem_bridge(
        &self,
        payload: EcosystemBridgePayload,
    ) -> EcosystemBridgePayload;

    /// Echo a Dodeca-style template call carrying dynamic values and tuple
    /// vector kwargs.
    async fn echo_dodeca_template_call(&self, call: DodecaTemplateCall) -> DodecaTemplateCall;

    /// Dodeca HTML processor root with maps, sets, tuple vectors, nested
    /// structs, and optional processing panels.
    async fn dodeca_html_process(&self, input: DodecaHtmlProcessInput) -> DodecaHtmlProcessResult;

    /// Dodeca code-execution root with executable samples, language config,
    /// dependency metadata, and tuple-vector sample results.
    async fn dodeca_execute_code_samples(
        &self,
        input: DodecaExecuteSamplesInput,
    ) -> DodecaCodeExecutionResult;

    /// Dodeca data-loader root carrying parsed dynamic values.
    async fn dodeca_load_data(
        &self,
        content: String,
        format: DodecaDataFormat,
    ) -> DodecaLoadDataResult;

    /// Dodeca markdown parse/render root with frontmatter, headings, reqs,
    /// injections, and source maps.
    async fn dodeca_parse_and_render(
        &self,
        source_path: String,
        content: String,
        source_map: bool,
    ) -> DodecaParseResult;

    /// Echo Dodeca image processor byte/scalar/result roots from
    /// `cell-image-proto`.
    async fn echo_dodeca_image_processor_fixture(
        &self,
        fixture: DodecaImageProcessorFixture,
    ) -> DodecaImageProcessorFixture;

    /// Echo Dodeca search indexer page/file/result roots from
    /// `cell-search-proto`.
    async fn echo_dodeca_search_indexer_fixture(
        &self,
        fixture: DodecaSearchIndexerFixture,
    ) -> DodecaSearchIndexerFixture;

    /// Echo Dodeca CSS/SASS/SVGO asset-processing roots from the asset proto crates.
    async fn echo_dodeca_asset_processing_fixture(
        &self,
        fixture: DodecaAssetProcessingFixture,
    ) -> DodecaAssetProcessingFixture;

    /// Echo Dodeca small-cell lifecycle/minify/image/dialog/tui service roots.
    async fn echo_dodeca_small_cell_services_fixture(
        &self,
        fixture: DodecaSmallCellServicesFixture,
    ) -> DodecaSmallCellServicesFixture;

    /// Dodeca devtools scope query from the browser overlay.
    async fn dodeca_devtools_get_scope(&self, path: Option<Vec<String>>) -> Vec<DodecaScopeEntry>;

    /// Dodeca devtools expression evaluation root.
    async fn dodeca_devtools_eval(
        &self,
        snapshot_id: String,
        expression: String,
    ) -> DodecaEvalResult;

    /// Dodeca devtools dead-link source-opening root.
    async fn dodeca_devtools_open_dead_link(
        &self,
        route: String,
        target: DodecaDeadLinkTarget,
    ) -> DodecaOpenSourceResult;

    /// Dodeca browser editor load root.
    async fn dodeca_devtools_edit_load(&self, token: String, route: String) -> DodecaEditLoad;

    /// Dodeca browser editor preview root.
    async fn dodeca_devtools_edit_preview(
        &self,
        token: String,
        source_key: String,
        buffer: String,
    ) -> DodecaEditPreview;

    /// Dodeca browser editor save root.
    async fn dodeca_devtools_edit_save(
        &self,
        token: String,
        req: DodecaEditSaveReq,
    ) -> DodecaEditSave;

    /// Dodeca browser editor image-upload root.
    async fn dodeca_devtools_edit_upload(
        &self,
        token: String,
        req: DodecaEditUploadReq,
    ) -> DodecaEditUpload;

    /// Dodeca browser editor file-provider read root.
    async fn dodeca_devtools_edit_read(&self, token: String, uri: String) -> DodecaEditRead;

    /// Dodeca browser editor file tree root.
    async fn dodeca_devtools_edit_list(&self, token: String) -> DodecaEditList;

    /// Echo a Styx tree value. This mirrors `styx_tree::Value`: recursive
    /// structs/enums with tags, spans, sequences, objects, and entry key/value
    /// recursion.
    async fn echo_styx_value(&self, value: StyxValue) -> StyxValue;

    /// Styx LSP extension initialization root.
    async fn styx_lsp_initialize(&self, params: StyxLspInitializeParams)
    -> StyxLspInitializeResult;

    /// Styx LSP extension completions root.
    async fn styx_lsp_completions(
        &self,
        params: StyxLspCompletionParams,
    ) -> Vec<StyxLspCompletionItem>;

    /// Styx LSP extension hover root.
    async fn styx_lsp_hover(&self, params: StyxLspHoverParams) -> Option<StyxLspHoverResult>;

    /// Styx LSP extension inlay hints root.
    async fn styx_lsp_inlay_hints(&self, params: StyxLspInlayHintParams) -> Vec<StyxLspInlayHint>;

    /// Styx LSP extension diagnostics root.
    async fn styx_lsp_diagnostics(&self, params: StyxLspDiagnosticParams)
    -> Vec<StyxLspDiagnostic>;

    /// Styx LSP extension code actions root.
    async fn styx_lsp_code_actions(
        &self,
        params: StyxLspCodeActionParams,
    ) -> Vec<StyxLspCodeAction>;

    /// Styx LSP extension definition root.
    async fn styx_lsp_definition(&self, params: StyxLspDefinitionParams) -> Vec<StyxLspLocation>;

    /// Styx LSP extension shutdown root.
    async fn styx_lsp_shutdown(&self);

    /// Styx LSP host callback for subtree lookup.
    async fn styx_host_get_subtree(&self, params: StyxLspGetSubtreeParams) -> Option<StyxValue>;

    /// Styx LSP host callback for full document lookup.
    async fn styx_host_get_document(&self, params: StyxLspGetDocumentParams) -> Option<StyxValue>;

    /// Styx LSP host callback for source text lookup.
    async fn styx_host_get_source(&self, params: StyxLspGetSourceParams) -> Option<String>;

    /// Styx LSP host callback for schema source lookup.
    async fn styx_host_get_schema(
        &self,
        params: StyxLspGetSchemaParams,
    ) -> Option<StyxLspSchemaInfo>;

    /// Styx LSP host callback for offset to position conversion.
    async fn styx_host_offset_to_position(
        &self,
        params: StyxLspOffsetToPositionParams,
    ) -> Option<StyxLspPosition>;

    /// Styx LSP host callback for position to offset conversion.
    async fn styx_host_position_to_offset(
        &self,
        params: StyxLspPositionToOffsetParams,
    ) -> Option<u32>;

    /// Stax-style flamegraph query: request filters in, recursive flamegraph
    /// update out.
    async fn stax_flamegraph(&self, params: StaxViewParams) -> StaxFlamegraphUpdate;

    /// Echo a Stax flamegraph update. This mirrors the recursive
    /// `stax_live_proto::FlamegraphUpdate` payload used by live profiling.
    async fn echo_stax_flamegraph_update(
        &self,
        update: StaxFlamegraphUpdate,
    ) -> StaxFlamegraphUpdate;

    /// Stax live flamegraph subscription shape: a non-nested channel of
    /// recursive flamegraph updates.
    async fn stax_subscribe_flamegraph_updates(&self, output: Tx<StaxFlamegraphUpdate>);

    /// Echo Stax Linux broker-control DTOs. This mirrors the ordinary typed
    /// config/status/error surface around fd brokering without carrying
    /// transport-owned file descriptors as payload data.
    async fn echo_stax_linux_broker_control(
        &self,
        fixture: StaxLinuxBrokerControlFixture,
    ) -> StaxLinuxBrokerControlFixture;

    /// Stax macOS daemon record shape: session config in, raw kdebug record
    /// batches streamed back through a non-nested channel, and a terminal
    /// record result returned as a normal user-error result.
    async fn stax_macos_record(
        &self,
        config: StaxMacSessionConfig,
        records: Tx<StaxMacKdBufBatch>,
    ) -> Result<StaxMacRecordSummary, StaxMacRecordError>;

    /// Echo a Hotmeal live-reload event. This mirrors
    /// `hotmeal_server::LiveReloadEvent`, including reload, patch bytes, and
    /// head-change notifications.
    async fn echo_hotmeal_live_reload_event(
        &self,
        event: HotmealLiveReloadEvent,
    ) -> HotmealLiveReloadEvent;

    /// Echo a Hotmeal browser-fuzzer patch result. This mirrors the
    /// browser-proto result shape with a recursive DOM tree and patch trace.
    async fn echo_hotmeal_apply_patches_result(
        &self,
        result: HotmealApplyPatchesResult,
    ) -> HotmealApplyPatchesResult;

    /// Hotmeal live-reload browser subscription method. This mirrors
    /// `hotmeal_server::LiveReloadService::subscribe(route)`.
    async fn hotmeal_live_reload_subscribe(&self, route: String);

    /// Hotmeal live-reload browser callback method. This mirrors
    /// `hotmeal_server::LiveReloadBrowser::on_event(event)`.
    async fn hotmeal_live_reload_on_event(&self, event: HotmealLiveReloadEvent);

    /// Echo Helix trace metrics. This mirrors the large vector-heavy
    /// `helix_trace_server::StreamMetrics` payload.
    async fn echo_helix_stream_metrics(&self, metrics: HelixStreamMetrics) -> HelixStreamMetrics;

    /// Echo Helix verify evidence. This mirrors the nested option/vector/enum
    /// shape returned by `TraceService::verify_evidence`.
    async fn echo_helix_verify_evidence(
        &self,
        digest: HelixVerifyEvidenceDigest,
    ) -> HelixVerifyEvidenceDigest;

    /// Helix pulse subscription shape: a non-nested channel of pulse
    /// notifications, as used by `TraceService::subscribe_pulses`.
    async fn helix_subscribe_pulses(&self, output: Tx<HelixPulseAvailable>);

    /// Helix coherent per-pulse snapshot. Mirrors the `TraceService::pulse_bundle`
    /// request mask and large response shape without depending on Helix crates.
    async fn helix_pulse_bundle(
        &self,
        pulse_id: HelixSchedulerPulseId,
        fields: HelixPulseBundleFields,
    ) -> HelixPulseBundle;

    /// Helix broad trace-service query surface. Mirrors the live standalone
    /// query return families as one generated bridge root.
    async fn helix_trace_service_surface(&self) -> HelixTraceServiceSurface;

    /// Tracey daemon status query, mirrored from the current roam
    /// `TraceyDaemon::status` migration surface.
    async fn tracey_status(&self) -> TraceyStatusResponse;

    /// Tracey uncovered-rules query.
    async fn tracey_uncovered(&self, req: TraceyUncoveredRequest) -> TraceyUncoveredResponse;

    /// Tracey untested-rules query.
    async fn tracey_untested(&self, req: TraceyUntestedRequest) -> TraceyUntestedResponse;

    /// Tracey stale-references query.
    async fn tracey_stale(&self, req: TraceyStaleRequest) -> TraceyStaleResponse;

    /// Tracey unmapped-code query.
    async fn tracey_unmapped(&self, req: TraceyUnmappedRequest) -> TraceyUnmappedResponse;

    /// Tracey rule detail query, mirrored from the current roam
    /// `TraceyDaemon::rule` migration surface.
    async fn tracey_rule(&self, rule_id: TraceyRuleId) -> Option<TraceyRuleInfo>;

    /// Tracey forward traceability dashboard query.
    async fn tracey_forward(&self, spec: String, impl_name: String)
    -> Option<TraceyApiSpecForward>;

    /// Tracey reverse traceability dashboard query.
    async fn tracey_reverse(&self, spec: String, impl_name: String)
    -> Option<TraceyApiReverseData>;

    /// Tracey dashboard file-content query.
    async fn tracey_file(&self, req: TraceyFileRequest) -> Option<TraceyApiFileData>;

    /// Tracey rendered spec-content query.
    async fn tracey_spec_content(
        &self,
        spec: String,
        impl_name: String,
    ) -> Option<TraceyApiSpecData>;

    /// Tracey dashboard search query.
    async fn tracey_search(&self, query: String, limit: u32) -> Vec<TraceySearchResult>;

    /// Tracey dashboard inline file range update.
    async fn tracey_update_file_range(
        &self,
        req: TraceyUpdateFileRangeRequest,
    ) -> Result<(), TraceyUpdateError>;

    /// Tracey MCP config exclude mutation.
    async fn tracey_config_add_exclude(
        &self,
        req: TraceyConfigPatternRequest,
    ) -> Result<(), String>;

    /// Tracey MCP config include mutation.
    async fn tracey_config_add_include(
        &self,
        req: TraceyConfigPatternRequest,
    ) -> Result<(), String>;

    /// Tracey daemon configuration query.
    async fn tracey_config(&self) -> TraceyApiConfig;

    /// Tracey VFS overlay open notification.
    async fn tracey_vfs_open(&self, path: String, content: String);

    /// Tracey VFS overlay change notification.
    async fn tracey_vfs_change(&self, path: String, content: String);

    /// Tracey VFS overlay close notification.
    async fn tracey_vfs_close(&self, path: String);

    /// Tracey daemon reload control query.
    async fn tracey_reload(&self) -> TraceyReloadResponse;

    /// Tracey daemon data version query.
    async fn tracey_version(&self) -> u64;

    /// Tracey daemon health query.
    async fn tracey_health(&self) -> TraceyHealthResponse;

    /// Tracey daemon shutdown notification.
    async fn tracey_shutdown(&self);

    /// Tracey validation query, mirrored from the current roam
    /// `TraceyDaemon::validate` migration surface.
    async fn tracey_validate(&self, req: TraceyValidateRequest) -> TraceyValidationResult;

    /// Tracey LSP test-file classifier.
    async fn tracey_is_test_file(&self, path: String) -> bool;

    /// Tracey LSP hover query.
    async fn tracey_lsp_hover(&self, req: TraceyLspPositionRequest) -> Option<TraceyHoverInfo>;

    /// Tracey LSP definition query.
    async fn tracey_lsp_definition(&self, req: TraceyLspPositionRequest) -> Vec<TraceyLspLocation>;

    /// Tracey LSP implementation query.
    async fn tracey_lsp_implementation(
        &self,
        req: TraceyLspPositionRequest,
    ) -> Vec<TraceyLspLocation>;

    /// Tracey LSP references query.
    async fn tracey_lsp_references(
        &self,
        req: TraceyLspReferencesRequest,
    ) -> Vec<TraceyLspLocation>;

    /// Tracey LSP completions query.
    async fn tracey_lsp_completions(
        &self,
        req: TraceyLspPositionRequest,
    ) -> Vec<TraceyLspCompletionItem>;

    /// Tracey workspace diagnostics query, mirroring the LSP-facing daemon
    /// surface.
    async fn tracey_lsp_workspace_diagnostics(&self) -> Vec<TraceyLspFileDiagnostics>;

    /// Tracey LSP document symbols query.
    async fn tracey_lsp_document_symbols(
        &self,
        req: TraceyLspDocumentRequest,
    ) -> Vec<TraceyLspSymbol>;

    /// Tracey LSP workspace symbols query.
    async fn tracey_lsp_workspace_symbols(&self, query: String) -> Vec<TraceyLspSymbol>;

    /// Tracey LSP semantic tokens query.
    async fn tracey_lsp_semantic_tokens(
        &self,
        req: TraceyLspDocumentRequest,
    ) -> Vec<TraceyLspSemanticToken>;

    /// Tracey LSP code lens query.
    async fn tracey_lsp_code_lens(&self, req: TraceyLspDocumentRequest) -> Vec<TraceyLspCodeLens>;

    /// Tracey LSP inlay hints query.
    async fn tracey_lsp_inlay_hints(
        &self,
        req: TraceyLspInlayHintsRequest,
    ) -> Vec<TraceyLspInlayHint>;

    /// Tracey LSP prepare rename query.
    async fn tracey_lsp_prepare_rename(
        &self,
        req: TraceyLspPositionRequest,
    ) -> Option<TraceyPrepareRenameResult>;

    /// Tracey LSP rename query.
    async fn tracey_lsp_rename(&self, req: TraceyLspRenameRequest) -> Vec<TraceyLspTextEdit>;

    /// Tracey LSP code actions query.
    async fn tracey_lsp_code_actions(
        &self,
        req: TraceyLspPositionRequest,
    ) -> Vec<TraceyLspCodeAction>;

    /// Tracey LSP document highlight query.
    async fn tracey_lsp_document_highlight(
        &self,
        req: TraceyLspPositionRequest,
    ) -> Vec<TraceyLspLocation>;

    /// Tracey daemon update subscription shape: a non-nested channel of
    /// `DataUpdate` messages.
    async fn tracey_subscribe_updates(&self, updates: Tx<TraceyDataUpdate>);
}

// ============================================================================
// Complex types for testing encoding/decoding
// ============================================================================

/// A point with a string label and an active flag.
/// Used to test multi-arg methods and varied field types.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct TaggedPoint {
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub active: bool,
}

/// A simple struct with primitive fields.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// A self-recursive tree: a value plus child trees of the same type. Exercises
/// typed-VM recursion (`Access::Recurse` lowered to `MemOp::CallBlock`) end to end
/// — encode, the reconciling decode, and the cross-language matrix.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Tree {
    pub value: u32,
    pub children: Vec<Tree>,
}

/// Responsive image variants as sent by Dodeca-style HTML processing payloads.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct BridgeResponsiveImageInfo {
    pub jxl_srcset: Vec<(String, u32)>,
    pub webp_srcset: Vec<(String, u32)>,
}

/// A compact representative of the ecosystem bridge surface:
/// string-keyed maps, string sets, tuple vectors, and byte blobs.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct EcosystemBridgePayload {
    pub html: String,
    pub path_map: BTreeMap<String, String>,
    pub known_routes: BTreeSet<String>,
    pub image_variants: BTreeMap<String, BridgeResponsiveImageInfo>,
    pub blobs: Vec<Vec<u8>>,
}

/// Dynamic Dodeca template/host call payloads. `Value` is intentionally dynamic:
/// it is only compatible with another dynamic schema, not with arbitrary concrete
/// reader schemas.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaTemplateCall {
    pub context_id: String,
    pub name: String,
    pub args: Vec<Value>,
    pub kwargs: Vec<(String, Value)>,
}

/// Data format selector from `cell-data-proto::DataFormat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaDataFormat {
    Json,
    Toml,
    Yaml,
}

/// Data-loader response from `cell-data-proto::LoadDataResult`.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaLoadDataResult {
    Success { value: Value },
    Error { message: String },
}

/// Markdown heading from `cell-markdown-proto::Heading`.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaMarkdownHeading {
    pub title: String,
    pub id: String,
    pub level: u8,
}

/// Requirement definition found by Dodeca markdown processing.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaReqDefinition {
    pub id: String,
    pub anchor_id: String,
}

/// Source-map node kind from `cell-markdown-proto::SourceKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaSourceKind {
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

/// One Dodeca markdown source-map entry.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaSourceMapEntry {
    pub id: String,
    pub kind: DodecaSourceKind,
    pub line_start: u32,
    pub line_end: u32,
    pub byte_start: u64,
    pub byte_end: u64,
}

/// Dodeca markdown source map.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaSourceMap {
    pub source_path: Option<String>,
    pub entries: Vec<DodecaSourceMapEntry>,
}

/// Dodeca markdown frontmatter with dynamic extra fields.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaFrontmatter {
    pub title: String,
    pub weight: i32,
    pub description: Option<String>,
    pub template: Option<String>,
    pub extra: Value,
}

/// Combined Dodeca markdown parse/render response.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaParseResult {
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

/// Decoded image bytes from `cell-image-proto`.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaDecodedImage {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub channels: u8,
}

/// Image-processing result from `cell-image-proto`.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaImageResult {
    Success { image: DodecaDecodedImage },
    ThumbhashSuccess { data_url: String },
    Error { message: String },
}

/// Resize request from `cell-image-proto`.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaResizeInput {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub channels: u8,
    pub target_width: u32,
}

/// Thumbhash request from `cell-image-proto`.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaThumbhashInput {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Aggregate Dodeca image processor fixture root.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaImageProcessorFixture {
    pub png_data: Vec<u8>,
    pub decoded_result: DodecaImageResult,
    pub resize_input: DodecaResizeInput,
    pub resize_result: DodecaImageResult,
    pub thumbhash_input: DodecaThumbhashInput,
    pub thumbhash_result: DodecaImageResult,
    pub error_result: DodecaImageResult,
}

/// Page input to the Dodeca search indexer.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaSearchPage {
    pub url: String,
    pub source: String,
    pub html: String,
}

/// Search index output file.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaSearchFile {
    pub path: String,
    pub contents: Vec<u8>,
}

/// Search-indexing result from `cell-search-proto`.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaSearchIndexResult {
    Success { files: Vec<DodecaSearchFile> },
    Error { message: String },
}

/// Aggregate Dodeca search indexer fixture root.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaSearchIndexerFixture {
    pub pages: Vec<DodecaSearchPage>,
    pub result: DodecaSearchIndexResult,
    pub error_result: DodecaSearchIndexResult,
}

/// CSS rewrite/minification result from `cell-css-proto`.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaCssResult {
    Success { css: String },
    Error { message: String },
}

/// SASS compilation result from `cell-sass-proto`.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaSassResult {
    Success { css: String },
    Error { message: String },
}

/// SVGO optimization result from `cell-svgo-proto`.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaSvgoResult {
    Success { svg: String },
    Error { message: String },
}

/// Aggregate Dodeca CSS/SASS/SVGO asset-processing fixture root.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaAssetProcessingFixture {
    pub css_source: String,
    pub css_path_map: BTreeMap<String, String>,
    pub css_result: DodecaCssResult,
    pub sass_entrypoint: String,
    pub sass_files: BTreeMap<String, String>,
    pub sass_load_paths: Vec<String>,
    pub sass_result: DodecaSassResult,
    pub svg_source: String,
    pub svgo_result: DodecaSvgoResult,
}

/// Lifecycle ready message from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaReadyMsg {
    pub peer_id: u16,
    pub cell_name: String,
    pub pid: Option<u32>,
    pub version: Option<String>,
    pub features: Vec<String>,
}

/// Lifecycle ready acknowledgement from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaReadyAck {
    pub ok: bool,
    pub host_time_unix_ms: Option<u64>,
}

/// HTML/CSS/JS minification result from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaMinifyResult {
    Success { content: String },
    Error { message: String },
}

/// JavaScript path-rewrite input from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaJsRewriteInput {
    pub js: String,
    pub path_map: BTreeMap<String, String>,
}

/// HTML diff input from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaHtmlDiffInput {
    pub old_html: String,
    pub new_html: String,
}

/// HTML diff success payload from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaHtmlDiffOutcome {
    pub patches_blob: Vec<u8>,
}

/// HTML diff error payload from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaHtmlDiffError {
    Generic(String),
}

/// Font subsetting input from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaSubsetFontInput {
    pub data: Vec<u8>,
    pub chars: Vec<char>,
}

/// Font processing result from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaFontResult {
    DecompressSuccess { data: Vec<u8> },
    SubsetSuccess { data: Vec<u8> },
    CompressSuccess { data: Vec<u8> },
    Error { message: String },
}

/// WebP encoding input from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaWebpEncodeInput {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub quality: u8,
}

/// WebP result from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaWebpResult {
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

/// JPEG XL encoding input from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaJxlEncodeInput {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub quality: u8,
}

/// JPEG XL result from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaJxlResult {
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

/// Terminal menu selection result from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaSelectResult {
    Selected { index: usize },
    Cancelled,
}

/// Terminal confirmation result from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaConfirmResult {
    Yes,
    No,
    Cancelled,
}

/// Terminal recording configuration from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaRecordConfig {
    pub shell: Option<String>,
}

/// Terminal render result from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaTermResult {
    Success { html: String },
    Error { message: String },
}

/// Dev server startup result from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaStartDevServerResult {
    Success { port: u16 },
    Error { message: String },
}

/// Build run result from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaRunBuildResult {
    Success,
    Error { message: String },
}

/// Link check diagnostics from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaLinkDiagnostics {
    pub request_headers: Vec<(String, String)>,
    pub response_headers: Vec<(String, String)>,
    pub response_body: String,
}

/// Link status from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaLinkStatus {
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

/// Link check request from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaLinkCheckInput {
    pub urls: Vec<String>,
    pub delay_ms: u64,
    pub timeout_secs: u64,
}

/// Link check output from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaLinkCheckOutput {
    pub results: BTreeMap<String, DodecaLinkStatus>,
}

/// Link check result from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaLinkCheckResult {
    Success { output: DodecaLinkCheckOutput },
    Error { message: String },
}

/// Build task status from Dodeca small-cell services.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaTaskStatus {
    Pending,
    Running,
    Done,
    Error,
}

/// Build task progress from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaTaskProgress {
    pub name: String,
    pub total: u32,
    pub completed: u32,
    pub status: DodecaTaskStatus,
    pub message: Option<String>,
}

/// Full build progress from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaBuildProgress {
    pub parse: DodecaTaskProgress,
    pub render: DodecaTaskProgress,
    pub sass: DodecaTaskProgress,
    pub links: DodecaTaskProgress,
    pub search: DodecaTaskProgress,
}

/// Log level from Dodeca small-cell services.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Log event kind from Dodeca small-cell services.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaEventKind {
    Http { status: u16 },
    FileChange,
    Reload,
    Patch,
    Search,
    Server,
    Build,
    Generic,
}

/// Log event from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaLogEvent {
    pub level: DodecaLogLevel,
    pub kind: DodecaEventKind,
    pub message: String,
    pub fields: Vec<(String, String)>,
}

/// Bind mode from Dodeca small-cell services.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaBindMode {
    Local,
    Lan,
}

/// Server status from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaServerStatus {
    pub urls: Vec<String>,
    pub is_running: bool,
    pub bind_mode: DodecaBindMode,
    pub picante_cache_size: u64,
    pub cas_cache_size: u64,
    pub code_exec_cache_size: u64,
}

/// Server command from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaServerCommand {
    GoPublic,
    GoLocal,
    TogglePicanteDebug,
    CycleLogLevel,
    SetLogFilter { filter: String },
}

/// Command result from Dodeca small-cell services.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaCommandResult {
    Ok,
    Error { message: String },
}

/// Aggregate Dodeca small-cell services fixture root.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaSmallCellServicesFixture {
    pub ready_msg: DodecaReadyMsg,
    pub ready_ack: DodecaReadyAck,
    pub minify_result: DodecaMinifyResult,
    pub js_input: DodecaJsRewriteInput,
    pub js_result: Result<String, String>,
    pub html_diff_input: DodecaHtmlDiffInput,
    pub html_diff_result: Result<DodecaHtmlDiffOutcome, DodecaHtmlDiffError>,
    pub subset_font_input: DodecaSubsetFontInput,
    pub font_results: Vec<DodecaFontResult>,
    pub webp_encode_input: DodecaWebpEncodeInput,
    pub webp_results: Vec<DodecaWebpResult>,
    pub jxl_encode_input: DodecaJxlEncodeInput,
    pub jxl_results: Vec<DodecaJxlResult>,
    pub select_result: DodecaSelectResult,
    pub confirm_result: DodecaConfirmResult,
    pub record_config: DodecaRecordConfig,
    pub term_result: DodecaTermResult,
    pub start_dev_server_result: DodecaStartDevServerResult,
    pub run_build_result: DodecaRunBuildResult,
    pub link_check_input: DodecaLinkCheckInput,
    pub link_check_result: DodecaLinkCheckResult,
    pub build_progress: DodecaBuildProgress,
    pub log_event: DodecaLogEvent,
    pub server_status: DodecaServerStatus,
    pub server_command: DodecaServerCommand,
    pub command_result: DodecaCommandResult,
}

/// Browser devtools event from `dodeca_protocol::BrowserService::on_event`.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaDevtoolsEvent {
    Reload,
    CssChanged { path: String },
    Patches { route: String, patches: Vec<u8> },
    Error(DodecaErrorInfo),
    ErrorResolved { route: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaOpenSourceResult {
    Ok,
    Err(String),
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaDeadLinkTarget {
    Wiki { key: String, title: String },
    Internal { href: String, title: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaEditLoad {
    Ok {
        source_key: String,
        route: String,
        uri: String,
        content: String,
        base: String,
    },
    Denied,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaEditPreview {
    Ok {
        html: String,
        source_map: Vec<DodecaSidLine>,
    },
    Denied,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaSidLine {
    pub sid: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaEditRead {
    Ok { content: String, base: String },
    Denied,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaEditUploadReq {
    pub source_key: String,
    pub filename: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaEditUpload {
    Ok { markdown: String, path: String },
    Denied,
    NotFound,
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaEditSaveReq {
    pub source_key: String,
    pub buffer: String,
    pub base: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaEditEntry {
    pub source_key: String,
    pub route: String,
    pub uri: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaEditList {
    Ok { entries: Vec<DodecaEditEntry> },
    Denied,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaEditSave {
    Ok { commit: String, base: String },
    Denied,
    NotFound,
    Conflict { current: String },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaEvalResult {
    Ok(DodecaScopeValue),
    Err(String),
}

#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaErrorInfo {
    pub route: String,
    pub message: String,
    pub template: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub source_snippet: Option<DodecaSourceSnippet>,
    pub snapshot_id: String,
    pub available_variables: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaSourceSnippet {
    pub lines: Vec<DodecaSourceLine>,
    pub error_line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct DodecaSourceLine {
    pub number: u32,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaScopeEntry {
    pub name: String,
    pub value: DodecaScopeValue,
    pub expandable: bool,
}

#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaScopeValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array { length: usize, preview: String },
    Object { fields: usize, preview: String },
}

/// Dodeca HTML minification options from `cell-html-proto`.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaMinifyOptions {
    pub minify_inline_css: bool,
    pub minify_inline_js: bool,
    pub minify_html: bool,
}

/// Dodeca HTML document injection request from `cell-html-proto`.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaInjection {
    HeadStyle { css: String },
    HeadScript { js: String, module: bool },
    BodyScript { js: String, module: bool },
}

/// Dodeca mounted-source link localization metadata.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaMountLocalization {
    pub segment: String,
    pub routes: BTreeSet<String>,
}

/// Source of a Dodeca code dependency.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaDependencySource {
    CratesIo,
    Git { url: String, commit: String },
    Path { path: String },
}

/// Resolved dependency metadata used by Dodeca HTML buttons and code execution.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaResolvedDependency {
    pub name: String,
    pub version: String,
    pub source: DodecaDependencySource,
}

/// Code execution metadata embedded in Dodeca HTML code buttons.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaCodeExecutionMetadata {
    pub rustc_version: String,
    pub cargo_version: String,
    pub target: String,
    pub timestamp: String,
    pub cache_hit: bool,
    pub platform: String,
    pub arch: String,
    pub dependencies: Vec<DodecaResolvedDependency>,
}

/// Responsive image variants as sent by `cell-html-proto::HtmlProcessInput`.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaResponsiveImageInfo {
    pub jxl_srcset: Vec<(String, u32)>,
    pub webp_srcset: Vec<(String, u32)>,
    pub original_width: u32,
    pub original_height: u32,
    pub thumbhash_data_url: String,
}

/// Dodeca unified HTML processing request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaHtmlProcessInput {
    pub html: String,
    pub path_map: Option<BTreeMap<String, String>>,
    pub known_routes: Option<BTreeSet<String>>,
    pub code_metadata: Option<BTreeMap<String, DodecaCodeExecutionMetadata>>,
    pub injections: Vec<DodecaInjection>,
    pub minify: Option<DodecaMinifyOptions>,
    pub source_to_route: Option<BTreeMap<String, String>>,
    pub wiki_to_route: Option<BTreeMap<String, String>>,
    pub base_route: Option<String>,
    pub image_variants: Option<BTreeMap<String, DodecaResponsiveImageInfo>>,
    pub vite_css_map: Option<BTreeMap<String, Vec<String>>>,
    pub mount: Option<DodecaMountLocalization>,
}

/// Unresolved Dodeca wiki link reference from HTML processing.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaWikiLinkRef {
    pub key: String,
    pub target: String,
}

/// Dodeca unified HTML processing response.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaHtmlProcessResult {
    Success {
        html: String,
        had_dead_links: bool,
        had_code_buttons: bool,
        hrefs: Vec<String>,
        element_ids: Vec<String>,
        unresolved_wiki_links: Vec<DodecaWikiLinkRef>,
    },
    Error {
        message: String,
    },
}

/// Dodeca dependency specification from `cell-code-execution-proto`.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaDependencySpec {
    pub name: String,
    pub version: String,
    pub git: Option<String>,
    pub rev: Option<String>,
    pub branch: Option<String>,
    pub path: Option<String>,
    pub features: Option<Vec<String>>,
}

/// Rust-specific Dodeca execution config.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaRustConfig {
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub extension: Option<String>,
    pub prepare_code: Option<bool>,
    pub auto_imports: Option<Vec<String>>,
    pub show_output: Option<bool>,
}

/// Dodeca code execution configuration.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaCodeExecutionConfig {
    pub enabled: bool,
    pub fail_on_error: bool,
    pub timeout_secs: u64,
    pub cache_dir: String,
    pub project_root: Option<String>,
    pub dependencies: Vec<DodecaDependencySpec>,
    pub rust: Option<DodecaRustConfig>,
}

/// One extracted Dodeca code sample.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaCodeSample {
    pub source_path: String,
    pub line: usize,
    pub language: String,
    pub code: String,
    pub executable: bool,
    pub expected_errors: Vec<String>,
}

/// Dodeca code sample execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DodecaExecutionStatus {
    Success,
    Failed,
    Skipped,
}

/// Dodeca build metadata captured for reproducible code execution.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaBuildMetadata {
    pub rustc_version: String,
    pub cargo_version: String,
    pub target: String,
    pub timestamp: String,
    pub cache_hit: bool,
    pub platform: String,
    pub arch: String,
    pub dependencies: Vec<DodecaResolvedDependency>,
}

/// One Dodeca code execution result.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaExecutionResult {
    pub status: DodecaExecutionStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub metadata: Option<DodecaBuildMetadata>,
}

/// Dodeca code-sample extraction request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaExtractSamplesInput {
    pub source_path: String,
    pub content: String,
}

/// Dodeca code-sample extraction response.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaExtractSamplesOutput {
    pub samples: Vec<DodecaCodeSample>,
}

/// Dodeca code-sample execution request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaExecuteSamplesInput {
    pub samples: Vec<DodecaCodeSample>,
    pub config: DodecaCodeExecutionConfig,
}

/// Dodeca code-sample execution response.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DodecaExecuteSamplesOutput {
    pub results: Vec<(DodecaCodeSample, DodecaExecutionResult)>,
}

/// Dodeca code-execution service result.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DodecaCodeExecutionResult {
    ExtractSuccess { output: DodecaExtractSamplesOutput },
    ExecuteSuccess { output: DodecaExecuteSamplesOutput },
    Error { message: String },
}

/// Styx source span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct StyxSpan {
    pub start: u32,
    pub end: u32,
}

/// Styx scalar syntax kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum StyxScalarKind {
    Bare = 0,
    Quoted = 1,
    Raw = 2,
    Heredoc = 3,
}

/// Styx value tag.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxTag {
    pub name: String,
    pub span: Option<StyxSpan>,
}

/// Styx scalar payload.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxScalar {
    pub text: String,
    pub kind: StyxScalarKind,
    pub span: Option<StyxSpan>,
}

/// Styx recursive sequence payload.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxSequence {
    pub items: Vec<StyxValue>,
    pub span: Option<StyxSpan>,
}

/// Styx recursive object payload.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxObject {
    pub entries: Vec<StyxEntry>,
    pub span: Option<StyxSpan>,
}

/// Styx object entry.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxEntry {
    pub key: StyxValue,
    pub value: StyxValue,
    pub doc_comment: Option<String>,
}

/// Styx value payload.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum StyxPayload {
    Scalar(StyxScalar) = 0,
    Sequence(StyxSequence) = 1,
    Object(StyxObject) = 2,
}

/// Styx recursive value tree.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxValue {
    pub tag: Option<StyxTag>,
    pub payload: Option<StyxPayload>,
    pub span: Option<StyxSpan>,
}

/// Styx LSP position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct StyxLspPosition {
    pub line: u32,
    pub character: u32,
}

/// Styx LSP range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct StyxLspRange {
    pub start: StyxLspPosition,
    pub end: StyxLspPosition,
}

/// Styx LSP cursor with byte offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct StyxLspCursor {
    pub line: u32,
    pub character: u32,
    pub offset: u32,
}

/// Styx LSP extension capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum StyxLspCapability {
    Completions = 0,
    Hover = 1,
    InlayHints = 2,
    Diagnostics = 3,
    CodeActions = 4,
    Definition = 5,
}

/// Styx LSP extension initialization params.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspInitializeParams {
    pub styx_version: String,
    pub document_uri: String,
    pub schema_id: String,
}

/// Styx LSP extension initialization result.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspInitializeResult {
    pub name: String,
    pub version: String,
    pub capabilities: Vec<StyxLspCapability>,
}

/// Styx LSP completion request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspCompletionParams {
    pub document_uri: String,
    pub cursor: StyxLspCursor,
    pub path: Vec<String>,
    pub prefix: String,
    pub context: Option<StyxValue>,
    pub tagged_context: Option<StyxValue>,
}

/// Styx LSP completion kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum StyxLspCompletionKind {
    Field = 0,
    Value = 1,
    Keyword = 2,
    Type = 3,
}

/// Styx LSP completion item.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspCompletionItem {
    pub label: String,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub kind: Option<StyxLspCompletionKind>,
    pub sort_text: Option<String>,
    pub insert_text: Option<String>,
}

/// Styx LSP hover request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspHoverParams {
    pub document_uri: String,
    pub cursor: StyxLspCursor,
    pub path: Vec<String>,
    pub context: Option<StyxValue>,
    pub tagged_context: Option<StyxValue>,
}

/// Styx LSP hover result.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspHoverResult {
    pub contents: String,
    pub range: Option<StyxLspRange>,
}

/// Styx LSP inlay hint request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspInlayHintParams {
    pub document_uri: String,
    pub range: StyxLspRange,
    pub context: Option<StyxValue>,
}

/// Styx LSP inlay hint kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum StyxLspInlayHintKind {
    Type = 0,
    Parameter = 1,
}

/// Styx LSP inlay hint.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspInlayHint {
    pub position: StyxLspPosition,
    pub label: String,
    pub kind: Option<StyxLspInlayHintKind>,
    pub padding_left: bool,
    pub padding_right: bool,
}

/// Styx LSP diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum StyxLspDiagnosticSeverity {
    Error = 0,
    Warning = 1,
    Info = 2,
    Hint = 3,
}

/// Styx LSP diagnostic.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspDiagnostic {
    pub span: StyxSpan,
    pub severity: StyxLspDiagnosticSeverity,
    pub message: String,
    pub source: Option<String>,
    pub code: Option<String>,
    pub data: Option<StyxValue>,
}

/// Styx LSP diagnostics request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspDiagnosticParams {
    pub document_uri: String,
    pub tree: StyxValue,
    pub content: String,
}

/// Styx LSP code action request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspCodeActionParams {
    pub document_uri: String,
    pub span: StyxSpan,
    pub diagnostics: Vec<StyxLspDiagnostic>,
}

/// Styx LSP code action kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum StyxLspCodeActionKind {
    QuickFix = 0,
    Refactor = 1,
    Source = 2,
}

/// Styx LSP workspace edit.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspWorkspaceEdit {
    pub changes: Vec<StyxLspDocumentEdit>,
}

/// Styx LSP document edit.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspDocumentEdit {
    pub uri: String,
    pub edits: Vec<StyxLspTextEdit>,
}

/// Styx LSP text edit.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspTextEdit {
    pub span: StyxSpan,
    pub new_text: String,
}

/// Styx LSP code action.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspCodeAction {
    pub title: String,
    pub kind: Option<StyxLspCodeActionKind>,
    pub edit: Option<StyxLspWorkspaceEdit>,
    pub is_preferred: bool,
}

/// Styx LSP definition request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspDefinitionParams {
    pub document_uri: String,
    pub cursor: StyxLspCursor,
    pub path: Vec<String>,
    pub context: Option<StyxValue>,
    pub tagged_context: Option<StyxValue>,
}

/// Styx LSP location.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspLocation {
    pub uri: String,
    pub span: StyxSpan,
}

/// Styx LSP schema info returned by host callbacks.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspSchemaInfo {
    pub source: String,
    pub uri: String,
}

/// Styx LSP host get_subtree request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspGetSubtreeParams {
    pub document_uri: String,
    pub path: Vec<String>,
}

/// Styx LSP host get_document request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspGetDocumentParams {
    pub document_uri: String,
}

/// Styx LSP host get_source request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspGetSourceParams {
    pub document_uri: String,
}

/// Styx LSP host get_schema request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StyxLspGetSchemaParams {
    pub document_uri: String,
}

/// Styx LSP host offset_to_position request.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct StyxLspOffsetToPositionParams {
    pub document_uri: String,
    pub offset: u32,
}

/// Styx LSP host position_to_offset request.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct StyxLspPositionToOffsetParams {
    pub document_uri: String,
    pub position: StyxLspPosition,
}

/// Stax off-CPU timing counters by blocking reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct StaxOffCpuBreakdown {
    pub idle_ns: u64,
    pub lock_ns: u64,
    pub semaphore_ns: u64,
    pub ipc_ns: u64,
    pub io_read_ns: u64,
    pub io_write_ns: u64,
    pub readiness_ns: u64,
    pub sleep_ns: u64,
    pub connect_ns: u64,
    pub other_ns: u64,
}

/// Stax query time range, in nanoseconds relative to recording start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct StaxTimeRange {
    pub start_ns: u64,
    pub end_ns: u64,
}

/// Stax symbol filter entry.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StaxSymbolRef {
    pub function_name: Option<String>,
    pub binary: Option<String>,
}

/// Stax flamegraph filter.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StaxLiveFilter {
    pub time_range: Option<StaxTimeRange>,
    pub exclude_symbols: Vec<StaxSymbolRef>,
}

/// Stax flamegraph request parameters.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StaxViewParams {
    pub tid: Option<u32>,
    pub filter: StaxLiveFilter,
}

/// Stax recursive flamegraph node.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StaxFlameNode {
    pub address: u64,
    pub function_name: Option<u32>,
    pub binary: Option<u32>,
    pub is_main: bool,
    pub language: u32,
    pub on_cpu_ns: u64,
    pub off_cpu: StaxOffCpuBreakdown,
    pub pet_samples: u64,
    pub off_cpu_intervals: u64,
    pub cycles: u64,
    pub instructions: u64,
    pub l1d_misses: u64,
    pub branch_mispreds: u64,
    pub children: Vec<StaxFlameNode>,
}

/// Stax flamegraph update payload.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StaxFlamegraphUpdate {
    pub total_on_cpu_ns: u64,
    pub total_off_cpu: StaxOffCpuBreakdown,
    pub strings: Vec<String>,
    pub root: StaxFlameNode,
}

vox_schema::impl_reborrow_owned!(StaxFlamegraphUpdate);

/// Stax Linux perf-session request configuration for the broker-control path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct StaxLinuxPerfSessionConfig {
    pub target_pid: u32,
    pub frequency_hz: u32,
    pub kernel_stacks: bool,
    pub request_waking: bool,
    pub request_pmu: bool,
    pub request_dwarf_unwind: bool,
}

/// Stax Linux waking-event field offsets discovered by the broker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct StaxLinuxWakingFieldOffsets {
    pub wakee_pid_offset: u32,
    pub wakee_pid_size: u32,
}

/// Stax Linux perf-session setup error DTO.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum StaxLinuxPerfSessionError {
    NotPrivileged {
        detail: String,
    } = 0,
    PerfEventOpen {
        cpu: u32,
        errno: i32,
        detail: String,
    } = 1,
    NoSuchTarget(u32) = 2,
    NotAuthorized {
        caller_uid: u32,
        target_uid: u32,
    } = 3,
}

/// Stax Linux daemon status DTO.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct StaxLinuxDaemonStatus {
    pub version: String,
    pub host_arch: String,
    pub privileged: bool,
    pub perf_event_paranoid: i32,
}

/// Ordinary DTO bundle around Stax's Linux fd-broker control surface.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct StaxLinuxBrokerControlFixture {
    pub config: StaxLinuxPerfSessionConfig,
    pub status: StaxLinuxDaemonStatus,
    pub errors: Vec<StaxLinuxPerfSessionError>,
    pub waking_field_offsets: Option<StaxLinuxWakingFieldOffsets>,
}

/// One macOS kdebug record. Mirrors xnu `kd_buf` on LP64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(C)]
pub struct StaxMacKdBuf {
    pub timestamp: u64,
    pub arg1: u64,
    pub arg2: u64,
    pub arg3: u64,
    pub arg4: u64,
    pub arg5: u64,
    pub debugid: u32,
    pub cpuid: u32,
    pub unused: u64,
}

/// Stax macOS perf/kdebug session configuration.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct StaxMacSessionConfig {
    pub target_pid: u32,
    pub frequency_hz: u32,
    pub buf_records: u32,
    pub samplers: u32,
    pub pmu_event_configs: Vec<u64>,
    pub class_mask: u32,
    pub filter_range_value1: u32,
    pub filter_range_value2: u32,
    pub typefilter_cscs: Vec<u16>,
}

/// One macOS kdebug drain pass streamed over Vox.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct StaxMacKdBufBatch {
    pub records: Vec<StaxMacKdBuf>,
    pub read_started_mach_ticks: u64,
    pub drained_mach_ticks: u64,
    pub queued_for_send_mach_ticks: u64,
    pub send_started_mach_ticks: u64,
    pub drained_at_unix_ns: u64,
}

vox_schema::impl_reborrow_owned!(StaxMacKdBufBatch);

/// Successful macOS record-session summary.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct StaxMacRecordSummary {
    pub records_drained: u64,
    pub session_ns: u64,
}

/// macOS record-session errors surfaced as Vox user errors.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum StaxMacRecordError {
    NotRoot = 0,
    NotAuthorized {
        caller_uid: u32,
        target_uid: u32,
    } = 1,
    Busy {
        holder_uid: u32,
        holder_pid: u32,
        since_unix_ns: u64,
    } = 2,
    NoSuchTarget(u32) = 3,
    Kperf {
        op: String,
        code: i32,
    } = 4,
    Sysctl {
        op: String,
        message: String,
    } = 5,
    Evicted = 6,
}

/// Hotmeal live-reload event.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum HotmealLiveReloadEvent {
    Reload = 0,
    Patches {
        route: String,
        patches_blob: Vec<u8>,
    } = 1,
    HeadChanged {
        route: String,
    } = 2,
}

/// Hotmeal DOM element attribute.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HotmealDomAttr {
    pub name: String,
    pub value: String,
}

/// Hotmeal browser DOM node.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[facet(recursive_type)]
#[repr(u8)]
pub enum HotmealDomNode {
    Element {
        tag: String,
        attrs: Vec<HotmealDomAttr>,
        children: Vec<HotmealDomNode>,
    } = 0,
    Text(String) = 1,
    Comment(String) = 2,
}

/// One Hotmeal browser patch-application trace step.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HotmealPatchStep {
    pub index: u32,
    pub patch_debug: String,
    pub html_after: String,
    pub dom_tree: HotmealDomNode,
    pub error: Option<String>,
}

/// Hotmeal browser apply-patches result.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HotmealApplyPatchesResult {
    pub result_html: String,
    pub normalized_old_html: String,
    pub initial_dom_tree: HotmealDomNode,
    pub patch_trace: Vec<HotmealPatchStep>,
}

/// Helix scheduler pulse identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
#[facet(transparent)]
pub struct HelixSchedulerPulseId(pub u64);

/// Helix audio token identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
#[facet(transparent)]
pub struct HelixAudioTokenId(pub u32);

/// Helix text token identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
#[facet(transparent)]
pub struct HelixTextTokenId(pub u32);

/// Helix logical prompt position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
#[facet(transparent)]
pub struct HelixLogicalPosition(pub u32);

/// Helix audio representation version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
#[facet(transparent)]
pub struct HelixAudioRepresentationVersion(pub u32);

/// Helix half-open audio-token identity range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct HelixAudioTokenRange {
    pub start: HelixAudioTokenId,
    pub end: HelixAudioTokenId,
}

/// Helix mel-frame range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct HelixMelFrameRange {
    pub start: u32,
    pub end: u32,
}

/// Helix native encoder window identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
#[facet(transparent)]
pub struct HelixNativeEncoderWindowId(pub u32);

/// Helix conv-stem chunk identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
#[facet(transparent)]
pub struct HelixConvStemChunkId(pub u32);

/// Helix admission segment identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(transparent)]
#[facet(transparent)]
pub struct HelixAdmissionSegmentId(pub u32);

/// Helix audio-token representation span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct HelixAudioRepresentationSpan {
    pub audio: HelixAudioTokenRange,
    pub audio_representation_version: HelixAudioRepresentationVersion,
}

/// Helix token-merge provenance.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum HelixAudioTokenMergeProvenance {
    NoMerge {
        pre_merge_audio_token_id: HelixAudioTokenId,
    },
    Merged {
        pre_merge: HelixAudioTokenRange,
    },
}

/// Helix admission provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum HelixAudioTokenAdmissionProvenance {
    AdmitAll {
        admission_segment: HelixAdmissionSegmentId,
    },
}

/// Helix audio-token provenance record.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixAudioTokenProvenance {
    pub audio_token_id: HelixAudioTokenId,
    pub audio_representation_version: HelixAudioRepresentationVersion,
    pub mel_frames: Vec<HelixMelFrameRange>,
    pub native_window: HelixNativeEncoderWindowId,
    pub conv_stem_chunk: HelixConvStemChunkId,
    pub post_merge_audio_token_id: HelixAudioTokenId,
    pub merge: HelixAudioTokenMergeProvenance,
    pub admission: HelixAudioTokenAdmissionProvenance,
    pub cosine_to_previous: Option<f32>,
}

/// Status of one Helix speculative draft row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum HelixVerifyDraftStatus {
    Accepted = 0,
    Divergent = 1,
    DiscardedAfterDivergence = 2,
}

/// One Helix speculative draft row.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixVerifyDraftRow {
    pub draft_index: u32,
    pub draft_token_id: u32,
    pub verified_text_token_id: HelixTextTokenId,
    pub text: String,
    pub status: HelixVerifyDraftStatus,
    pub expected_observed_audio: HelixAudioTokenRange,
    pub max_dominant_audio_mass: f32,
    pub record_count: u32,
    pub max_logit: f32,
    pub draft_logit: f32,
}

/// Helix verify seed-row evidence.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixVerifySeedRow {
    pub query_row: u32,
    pub next_token_seed: u32,
    pub expected_observed_audio: HelixAudioTokenRange,
    pub max_dominant_audio_mass: f32,
    pub record_count: u32,
    pub max_logit: f32,
}

/// Helix per-pulse verify evidence digest.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixVerifyEvidenceDigest {
    pub pulse_id: HelixSchedulerPulseId,
    pub rewind_k: u64,
    pub accepted_prefix_len: Option<u64>,
    pub divergence_row: Option<u64>,
    pub drafts: Vec<HelixVerifyDraftRow>,
    pub seed: Option<HelixVerifySeedRow>,
}

/// Helix stream metric vectors, indexed by `pulse_ids`.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixStreamMetrics {
    pub pulse_ids: Vec<HelixSchedulerPulseId>,
    pub pulse_duration_us: Vec<u64>,
    pub decoded_tokens: Vec<u64>,
    pub committed_tokens: Vec<u64>,
    pub retained_speculative_tokens: Vec<u64>,
    pub evicted_audio_tokens: Vec<u64>,
    pub evicted_committed_tokens: Vec<u64>,
    pub rewind_k: Vec<u64>,
    pub ar_token_count: Vec<u64>,
    pub rolling_wer: Vec<f64>,
    pub s2d_p50_ms: Vec<f64>,
}

/// Helix text token snapshot inside a prompt layout.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HelixTextTokenSnapshot {
    pub text_token_id: HelixTextTokenId,
    pub text: Option<String>,
    pub text_before: Option<String>,
    pub in_verify_batch: bool,
    pub decoded_this_pulse: bool,
}

/// Helix prompt layout visible to the decoder for one pulse.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HelixPromptLayout {
    pub pulse_id: HelixSchedulerPulseId,
    pub first_audio_token_id: HelixAudioTokenId,
    pub resident_audio_frames: u64,
    pub changed_audio_spans: Vec<HelixAudioRepresentationSpan>,
    pub text_token_start: HelixTextTokenId,
    pub text_token_end: HelixTextTokenId,
    pub text_tokens: Vec<HelixTextTokenSnapshot>,
}

/// Helix per-pulse text/audio attention heatmap.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixPulseAttentionHeatmap {
    pub pulse_id: HelixSchedulerPulseId,
    pub first_audio_token_id: HelixAudioTokenId,
    pub audio_token_count: u32,
    pub text_token_start: HelixTextTokenId,
    pub text_token_count: u32,
    pub record_count: u32,
    pub max_value: f32,
    pub mean_audio_mass: Vec<f32>,
    pub text_token_glyphs: Vec<String>,
}

/// Helix encoder-frontier aggregation point.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixEncoderFrontierPoint {
    pub audio_token_id: HelixAudioTokenId,
    pub mean_frontier_debt: f32,
    pub head_count: u32,
}

/// Helix encoder-frontier layer.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixEncoderFrontierLayer {
    pub encoder_layer_index: u32,
    pub points: Vec<HelixEncoderFrontierPoint>,
}

/// Helix encoder-frontier series.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixEncoderFrontierSeries {
    pub pulse_id: HelixSchedulerPulseId,
    pub layers: Vec<HelixEncoderFrontierLayer>,
    pub min_audio_token_id: HelixAudioTokenId,
    pub max_audio_token_id: HelixAudioTokenId,
    pub min_frontier_debt: f32,
    pub max_frontier_debt: f32,
}

/// Helix encoder-provenance violation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum HelixEncoderProvenanceViolationKind {
    MissingProvenance = 0,
    VersionMismatch = 1,
    EmptyMelFrames = 2,
    NonFiniteFrontierDebt = 3,
}

/// Helix encoder-provenance violation.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HelixEncoderProvenanceViolation {
    pub audio_token_id: HelixAudioTokenId,
    pub encoder_layer_index: u32,
    pub head_index: u32,
    pub observed_audio_token_id: Option<HelixAudioTokenId>,
    pub kind: HelixEncoderProvenanceViolationKind,
    pub message: String,
}

/// Helix encoder-provenance report.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HelixEncoderProvenanceReport {
    pub pulse_id: HelixSchedulerPulseId,
    pub records_checked: u64,
    pub violations: Vec<HelixEncoderProvenanceViolation>,
}

/// Helix source-audio clip.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixAudioClip {
    pub sample_rate: u32,
    pub first_sample: u64,
    pub samples: Vec<f32>,
}

/// Helix mel-feature clip.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixMelClip {
    pub num_mel_bins: u32,
    pub first_mel_frame: u32,
    pub num_mel_frames: u32,
    pub values: Vec<f32>,
    pub min_value: f32,
    pub max_value: f32,
    pub corpus_min_value: f32,
    pub corpus_max_value: f32,
}

/// Compact Helix verify outcome for a pulse rollup.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HelixVerifyOutcome {
    pub rewind_k: u64,
    pub accepted_prefix_len: Option<u64>,
    pub divergence_row: Option<u64>,
    pub discarded_speculative_tokens: Option<u64>,
}

/// Helix pulse rollup.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HelixPulseRollup {
    pub pulse_id: HelixSchedulerPulseId,
    pub pulse_start_us: Option<u64>,
    pub pulse_duration_us: Option<u64>,
    pub encoder_duration_us: Option<u64>,
    pub refresh_duration_us: Option<u64>,
    pub verify_duration_us: Option<u64>,
    pub decode_duration_us: Option<u64>,
    pub commit_duration_us: Option<u64>,
    pub pulse_mel_frames: u64,
    pub committed_tokens: u64,
    pub retained_speculative_tokens: u64,
    pub resident_committed_tokens: u64,
    pub evicted_audio_tokens: u64,
    pub evicted_committed_tokens: u64,
    pub decoded_tokens: u64,
    pub hit_eos: bool,
    pub verify: Option<HelixVerifyOutcome>,
    pub has_attention_batch: bool,
    pub ar_token_count: u64,
}

/// Helix logical-to-physical trace span.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HelixTracePositionSpan {
    pub logical_start: u64,
    pub rows: u64,
    pub physical_start: u64,
}

/// Helix AR-decode early-exit reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum HelixArDecodeEarlyExitReason {
    BudgetExhausted = 0,
    NoBudget = 1,
    SeedWasEos = 2,
    ProducedEos = 3,
}

/// Helix verify-skipped reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum HelixVerifySkippedReason {
    RewindGuardFailed = 0,
    PreCommitFullRewind = 1,
}

/// Representative Helix streaming trace event variants carried in a bundle.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum HelixStreamingTraceEvent {
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
    AudioEncoderUpdate {
        start_us: u64,
        duration_us: u64,
        pulse_id: u64,
        num_audio_frames: u64,
        first_audio_token_id: u64,
        resident_audio_frames: u64,
        changed_span_count: u64,
        changed_audio_tokens: u64,
        latest_audio_representation_version: u64,
    },
    AudioEviction {
        timestamp_us: u64,
        pulse_id: u64,
        evicted_audio_tokens: u64,
        first_audio_token_id: u64,
        resident_audio_frames: u64,
        audio_ring_capacity: u64,
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
    LayoutSnapshot {
        timestamp_us: u64,
        pulse_id: u64,
        audio_len: u64,
        audio_head: u64,
        first_audio_token_id: u64,
        text_len: u64,
        first_text_token_id: u64,
        prompt_len: u64,
        resident_committed_len: u64,
        resident_text_len: u64,
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
    TextEviction {
        timestamp_us: u64,
        pulse_id: u64,
        evicted_committed_tokens: u64,
        resident_committed_capacity: u64,
        committed_text_len: u64,
    },
}

/// Helix Chrome/Perfetto-compatible trace event.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixChromeTraceEvent {
    pub name: String,
    pub cat: String,
    pub ph: String,
    pub ts: f64,
    pub dur: Option<f64>,
    pub pid: u32,
    pub tid: u32,
    pub s: Option<String>,
    pub args: BTreeMap<String, Value>,
}

/// Helix decode fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct HelixDecodeFact {
    pub text_token_id: HelixTextTokenId,
    pub query_position: HelixLogicalPosition,
    pub input_token_id: u32,
    pub observed_audio: HelixAudioTokenRange,
}

/// Helix verify-prediction fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct HelixVerifyPredictionFact {
    pub verified_text_token_id: HelixTextTokenId,
    pub verified_draft_index: u32,
    pub draft_token_id: u32,
    pub query_row: u32,
    pub query_position: HelixLogicalPosition,
    pub observed_audio: HelixAudioTokenRange,
}

/// Helix verify-seed fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct HelixVerifySeedFact {
    pub query_row: u32,
    pub query_position: HelixLogicalPosition,
    pub next_token_seed: u32,
    pub observed_audio: HelixAudioTokenRange,
}

/// Helix prompt-prefill fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct HelixPromptPrefillFact {
    pub query_position: HelixLogicalPosition,
    pub observed_audio: HelixAudioTokenRange,
}

/// Helix decoder evidence fact counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct HelixDecoderEvidenceFactCounts {
    pub decode: u32,
    pub verify_prediction: u32,
    pub verify_seed: u32,
    pub prompt_prefill: u32,
}

/// Helix encoder facts snapshot.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixEncoderFactsSnapshot {
    pub refreshed_audio: HelixAudioTokenRange,
    pub audio_representation_version: HelixAudioRepresentationVersion,
    pub provenance: Vec<HelixAudioTokenProvenance>,
}

/// Helix scheduler evidence snapshot.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixPulseEvidenceSnapshot {
    pub pulse_id: HelixSchedulerPulseId,
    pub encoder: Option<HelixEncoderFactsSnapshot>,
    pub counts: HelixDecoderEvidenceFactCounts,
    pub decode: Vec<HelixDecodeFact>,
    pub verify_prediction: Vec<HelixVerifyPredictionFact>,
    pub verify_seed: Vec<HelixVerifySeedFact>,
    pub prompt_prefill: Vec<HelixPromptPrefillFact>,
}

/// Helix `pulse_bundle` field mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct HelixPulseBundleFields {
    pub prompt_layout: bool,
    pub audio_provenance: bool,
    pub attention_heatmap: bool,
    pub encoder_frontier: bool,
    pub encoder_provenance: bool,
    pub audio_clip: bool,
    pub mel_clip: bool,
    pub pulse_rollup: bool,
    pub timeline: bool,
    pub gpu_chrome_events: bool,
    pub verify_evidence: bool,
    pub scheduler_snapshot: bool,
}

/// Helix coherent per-pulse snapshot composed from per-panel rollups.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixPulseBundle {
    pub pulse_id: HelixSchedulerPulseId,
    pub schema_version: u32,
    pub prompt_layout: Option<HelixPromptLayout>,
    pub audio_provenance: Option<Vec<HelixAudioTokenProvenance>>,
    pub attention_heatmap: Option<HelixPulseAttentionHeatmap>,
    pub encoder_frontier: Option<HelixEncoderFrontierSeries>,
    pub encoder_provenance: Option<HelixEncoderProvenanceReport>,
    pub audio_clip: Option<HelixAudioClip>,
    pub mel_clip: Option<HelixMelClip>,
    pub pulse_rollup: Option<HelixPulseRollup>,
    pub timeline: Option<Vec<HelixStreamingTraceEvent>>,
    pub gpu_chrome_events: Option<Vec<HelixChromeTraceEvent>>,
    pub verify_evidence: Option<HelixVerifyEvidenceDigest>,
    pub scheduler_snapshot: Option<HelixPulseEvidenceSnapshot>,
}

/// Helix pulse notification streamed by `subscribe_pulses`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct HelixPulseAvailable {
    pub pulse_id: HelixSchedulerPulseId,
}

vox_schema::impl_reborrow_owned!(HelixPulseAvailable);

/// Helix trace stream metadata shared by broad trace queries.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HelixStreamMeta {
    pub schema_version: u32,
    pub pulse_ids: Vec<HelixSchedulerPulseId>,
    pub timeline_event_count: u64,
    pub attention_batch_count: u64,
}

/// Helix run configuration metadata reported by the trace service.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixRunInfo {
    pub backend: String,
    pub model_dir: String,
    pub input: String,
    pub piece: Option<String>,
    pub pulse_ms: u32,
    pub audio_ring_capacity: u32,
    pub text_ring_capacity: u32,
    pub commit_revisable_tail_text_tokens: u32,
    pub revise_logit_margin: f32,
    pub sample_rate: u32,
    pub mel_hop_samples: u32,
    pub num_mel_bins: u32,
    pub num_mel_frames: u32,
    pub audio_tokens_per_chunk: u32,
    pub native_window_tokens: u32,
    pub realtime_pacing: bool,
    pub profile_phases: bool,
    pub attention_trace_schema_version: u32,
    pub trace_server_schema_version: u32,
}

/// Compact attention support summary used by trace-service attention queries.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixAttentionSupportSummary {
    pub total_audio_mass: f32,
    pub observed_audio: HelixAudioTokenRange,
    pub dominant_audio: HelixAudioTokenRange,
    pub dominant_audio_mass: f32,
    pub center_audio_token: Option<f32>,
    pub width_audio_tokens: Option<f32>,
}

/// One text-token attention support record.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixTextAttentionSupportRecord {
    pub text_token_id: HelixTextTokenId,
    pub query_position: HelixLogicalPosition,
    pub decoder_layer_index: u32,
    pub head_index: u32,
    pub support: HelixAttentionSupportSummary,
    pub audio_weights: Vec<f32>,
}

/// One encoder audio-token support record.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixAudioEncoderSupportRecord {
    pub audio_token_id: HelixAudioTokenId,
    pub audio_representation_version: HelixAudioRepresentationVersion,
    pub encoder_layer_index: u32,
    pub head_index: u32,
    pub support: HelixAttentionSupportSummary,
    pub frontier_debt: f32,
}

/// The reason a decoder-evidence record exists.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum HelixDecoderEvidenceKind {
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

/// One decoder-evidence query result row.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixDecoderEvidenceRecord {
    pub text_token_id: Option<HelixTextTokenId>,
    pub query_position: HelixLogicalPosition,
    pub expected_observed_audio: HelixAudioTokenRange,
    pub records: Vec<HelixTextAttentionSupportRecord>,
    pub kind: HelixDecoderEvidenceKind,
}

/// One header/query-row attention support record.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixQueryRowAttentionRecord {
    pub query_position: HelixLogicalPosition,
    pub decoder_layer_index: u32,
    pub head_index: u32,
    pub support: HelixAttentionSupportSummary,
    pub audio_weights: Vec<f32>,
}

/// A full attention summary batch for one Helix pulse.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixAttentionSummaryBatch {
    pub schema_version: u32,
    pub pulse_id: HelixSchedulerPulseId,
    pub audio_context_id: u64,
    pub text_context_id: u64,
    pub audio_representation_spans: Vec<HelixAudioRepresentationSpan>,
    pub changed_audio_representation_spans: Vec<HelixAudioRepresentationSpan>,
    pub text_support: Vec<HelixTextAttentionSupportRecord>,
    pub header_text_support: Vec<HelixQueryRowAttentionRecord>,
    pub audio_encoder_support: Vec<HelixAudioEncoderSupportRecord>,
    pub decoder_evidence: Vec<HelixDecoderEvidenceRecord>,
}

/// A text token's audio-attention row.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixTextAttendanceRow {
    pub text_token_id: HelixTextTokenId,
    pub decoder_layer_index: u32,
    pub head_index: u32,
    pub dominant_audio_mass: f32,
    pub total_audio_mass: f32,
    pub observed_audio: HelixAudioTokenRange,
    pub dominant_audio: HelixAudioTokenRange,
    pub audio_weights: Vec<f32>,
    pub queried_audio_weight: f32,
}

/// An audio-token attendance row for a text query.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixAudioAttendanceRow {
    pub decoder_layer_index: u32,
    pub head_index: u32,
    pub dominant_audio_mass: f32,
    pub total_audio_mass: f32,
    pub center_audio_token: Option<f32>,
    pub width_audio_tokens: Option<f32>,
    pub observed_audio: HelixAudioTokenRange,
    pub dominant_audio: HelixAudioTokenRange,
    pub audio_weights: Vec<f32>,
}

/// A refresh-prompt attendance row.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixRefreshAttendanceRow {
    pub query_position: HelixLogicalPosition,
    pub decoder_layer_index: u32,
    pub head_index: u32,
    pub dominant_audio_mass: f32,
    pub total_audio_mass: f32,
    pub center_audio_token: Option<f32>,
    pub width_audio_tokens: Option<f32>,
    pub observed_audio: HelixAudioTokenRange,
    pub dominant_audio: HelixAudioTokenRange,
    pub audio_weights: Vec<f32>,
}

/// One encoder self-attention row over audio tokens.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixAudioSelfAttentionRow {
    pub encoder_layer_index: u32,
    pub head_index: u32,
    pub audio_representation_version: HelixAudioRepresentationVersion,
    pub dominant_audio_mass: f32,
    pub total_audio_mass: f32,
    pub center_audio_token: Option<f32>,
    pub width_audio_tokens: Option<f32>,
    pub observed_audio: HelixAudioTokenRange,
    pub dominant_audio: HelixAudioTokenRange,
    pub frontier_debt: f32,
}

/// One decoded transcript token.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HelixTranscriptToken {
    pub text_token_id: HelixTextTokenId,
    pub decoded_in_pulse: HelixSchedulerPulseId,
    pub text: String,
    pub committed: bool,
}

/// Decoder-evidence aggregate counts by variant.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HelixDecoderEvidenceVariantCounts {
    pub decode: u64,
    pub verify_prediction: u64,
    pub verify_seed: u64,
    pub prompt_prefill: u64,
}

/// Decoder-evidence coverage report.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HelixDecoderEvidenceReport {
    pub total_batches: u64,
    pub batches_without_decoder_evidence: u64,
    pub pulses_without_decoder_evidence: Vec<HelixSchedulerPulseId>,
    pub variant_evidence_counts: HelixDecoderEvidenceVariantCounts,
    pub variant_record_counts: HelixDecoderEvidenceVariantCounts,
    pub observed_decoder_layer_indices: Vec<u32>,
    pub observed_decoder_head_indices: Vec<u32>,
}

/// Rolling piece-evaluation snapshot for the trace viewer.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixPieceEvalSnapshot {
    pub audio_now_ms: f64,
    pub reference_words_available: u32,
    pub hypothesis_words: u32,
    pub substitutions: u32,
    pub deletions: u32,
    pub insertions: u32,
    pub rolling_wer: f64,
    pub s2d_matched_words: u32,
    pub s2d_new_words: u32,
    pub s2d_p50_ms: Option<f64>,
    pub s2d_p90_ms: Option<f64>,
    pub s2d_p100_ms: Option<f64>,
    pub s2d_avg_ms: Option<f64>,
    pub audio_frontier: u32,
    pub displayed_frontier: u32,
    pub committed_frontier: u32,
    pub lag_ms: f64,
}

/// Reference text metadata for piece evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct HelixPieceEvalReference {
    pub piece: String,
    pub language: String,
    pub words: Vec<String>,
}

/// Broad Helix trace-service surface mirrored as one generated bridge root.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct HelixTraceServiceSurface {
    pub meta: HelixStreamMeta,
    pub pulse_rollup: Option<HelixPulseRollup>,
    pub timeline: Vec<HelixStreamingTraceEvent>,
    pub attention_batch: Option<HelixAttentionSummaryBatch>,
    pub prompt_layout: Option<HelixPromptLayout>,
    pub audio_attended_by: Vec<HelixTextAttendanceRow>,
    pub text_attends_to: Vec<HelixAudioAttendanceRow>,
    pub refresh_attends_to: Vec<HelixRefreshAttendanceRow>,
    pub audio_token_provenance: Option<HelixAudioTokenProvenance>,
    pub audio_provenance_for_pulse: Vec<HelixAudioTokenProvenance>,
    pub audio_tokens_for_mel_frame: Vec<HelixAudioTokenId>,
    pub audio_clip_for_audio_token: Option<HelixAudioClip>,
    pub audio_clip_for_prompt: Option<HelixAudioClip>,
    pub audio_clip_for_audio_range: Option<HelixAudioClip>,
    pub mel_clip_for_prompt: Option<HelixMelClip>,
    pub audio_self_attention: Vec<HelixAudioSelfAttentionRow>,
    pub transcript: Vec<HelixTranscriptToken>,
    pub pulse_attention_heatmap: Option<HelixPulseAttentionHeatmap>,
    pub encoder_frontier: Option<HelixEncoderFrontierSeries>,
    pub stream_metrics: HelixStreamMetrics,
    pub verify_evidence: Option<HelixVerifyEvidenceDigest>,
    pub decoder_evidence_report: HelixDecoderEvidenceReport,
    pub pulse_evidence_snapshot: Option<HelixPulseEvidenceSnapshot>,
    pub gpu_chrome_events_for_pulse: Vec<HelixChromeTraceEvent>,
    pub run_info: Option<HelixRunInfo>,
    pub piece_eval_reference: Option<HelixPieceEvalReference>,
    pub piece_eval_for_pulse: Option<HelixPieceEvalSnapshot>,
    pub encoder_provenance_report: Option<HelixEncoderProvenanceReport>,
    pub pulse_bundle_fields: HelixPulseBundleFields,
    pub pulse_bundle: HelixPulseBundle,
    pub pulse_available: HelixPulseAvailable,
}

/// Tracey structured rule ID.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
pub struct TraceyRuleId {
    pub base: String,
    pub version: u32,
}

/// Tracey code reference.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyCodeRef {
    pub file: String,
    pub line: usize,
}

/// Tracey implementation status for one spec/impl pair.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyImplStatus {
    pub spec: String,
    pub impl_name: String,
    pub total_rules: usize,
    pub covered_rules: usize,
    pub stale_rules: usize,
    pub verified_rules: usize,
}

/// Tracey status response.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyStatusResponse {
    pub impls: Vec<TraceyImplStatus>,
}

/// Tracey uncovered-rules request.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyUncoveredRequest {
    pub spec: Option<String>,
    pub impl_name: Option<String>,
    pub prefix: Option<String>,
}

/// Tracey untested-rules request.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyUntestedRequest {
    pub spec: Option<String>,
    pub impl_name: Option<String>,
    pub prefix: Option<String>,
}

/// Tracey stale-references request.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyStaleRequest {
    pub spec: Option<String>,
    pub impl_name: Option<String>,
    pub prefix: Option<String>,
}

/// Tracey rule reference in grouped query responses.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyRuleRef {
    pub id: TraceyRuleId,
    pub text: Option<String>,
}

/// Tracey query rules grouped by spec section.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceySectionRules {
    pub section: String,
    pub rules: Vec<TraceyRuleRef>,
}

/// Tracey uncovered-rules response.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyUncoveredResponse {
    pub spec: String,
    pub impl_name: String,
    pub total_rules: usize,
    pub uncovered_count: usize,
    pub by_section: Vec<TraceySectionRules>,
}

/// Tracey untested-rules response.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyUntestedResponse {
    pub spec: String,
    pub impl_name: String,
    pub total_rules: usize,
    pub untested_count: usize,
    pub by_section: Vec<TraceySectionRules>,
}

/// Tracey stale-reference entry.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyStaleEntry {
    pub current_id: TraceyRuleId,
    pub file: String,
    pub line: usize,
    pub reference_id: TraceyRuleId,
}

/// Tracey stale-references response.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyStaleResponse {
    pub spec: String,
    pub impl_name: String,
    pub total_rules: usize,
    pub stale_count: usize,
    pub refs: Vec<TraceyStaleEntry>,
}

/// Tracey unmapped-code request.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyUnmappedRequest {
    pub spec: Option<String>,
    pub impl_name: Option<String>,
    pub path: Option<String>,
}

/// Tracey unmapped code unit.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyUnmappedUnit {
    pub kind: String,
    pub name: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
}

/// Tracey unmapped code tree entry.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyUnmappedEntry {
    pub path: String,
    pub is_dir: bool,
    pub total_units: usize,
    pub unmapped_units: usize,
    pub units: Vec<TraceyUnmappedUnit>,
}

/// Tracey unmapped-code response.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyUnmappedResponse {
    pub spec: String,
    pub impl_name: String,
    pub total_units: usize,
    pub unmapped_count: usize,
    pub entries: Vec<TraceyUnmappedEntry>,
}

/// Tracey daemon API configuration response.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyApiConfig {
    pub project_root: String,
    pub specs: Vec<TraceyApiSpecInfo>,
}

/// Tracey daemon API spec configuration.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyApiSpecInfo {
    pub name: String,
    pub prefix: String,
    pub source: Option<String>,
    pub source_url: Option<String>,
    pub implementations: Vec<String>,
}

/// Tracey reload response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct TraceyReloadResponse {
    pub version: u64,
    pub rebuild_time_ms: u64,
}

/// Tracey health response.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyHealthResponse {
    pub version: u64,
    pub watcher_active: bool,
    pub watcher_error: Option<String>,
    pub config_error: Option<String>,
    pub watcher_last_event_ms: Option<u64>,
    pub watcher_event_count: u64,
    pub watched_directories: Vec<String>,
    pub uptime_secs: u64,
}

/// Tracey per-rule coverage in one implementation.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyRuleCoverage {
    pub spec: String,
    pub impl_name: String,
    pub impl_refs: Vec<TraceyCodeRef>,
    pub verify_refs: Vec<TraceyCodeRef>,
}

/// Tracey rule detail response.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyRuleInfo {
    pub id: TraceyRuleId,
    pub raw: String,
    pub html: String,
    pub source_file: Option<String>,
    pub source_line: Option<usize>,
    pub coverage: Vec<TraceyRuleCoverage>,
    pub version_diff: Option<String>,
}

/// Tracey stale reference in dashboard rule data.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyApiStaleRef {
    pub file: String,
    pub line: usize,
    pub reference_id: TraceyRuleId,
}

/// Tracey dashboard rule row for forward traceability.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyApiRule {
    pub id: TraceyRuleId,
    pub raw: String,
    pub html: String,
    pub status: Option<String>,
    pub level: Option<String>,
    pub source_file: Option<String>,
    pub source_line: Option<usize>,
    pub source_column: Option<usize>,
    pub section: Option<String>,
    pub section_title: Option<String>,
    pub impl_refs: Vec<TraceyCodeRef>,
    pub verify_refs: Vec<TraceyCodeRef>,
    pub depends_refs: Vec<TraceyCodeRef>,
    pub is_stale: bool,
    pub stale_refs: Vec<TraceyApiStaleRef>,
}

/// Tracey forward traceability response for one spec.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyApiSpecForward {
    pub name: String,
    pub rules: Vec<TraceyApiRule>,
}

/// Tracey reverse traceability file tree entry.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyApiFileEntry {
    pub path: String,
    pub total_units: usize,
    pub covered_units: usize,
}

/// Tracey reverse traceability response.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyApiReverseData {
    pub total_units: usize,
    pub covered_units: usize,
    pub files: Vec<TraceyApiFileEntry>,
}

/// Tracey file-content dashboard request.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyFileRequest {
    pub spec: String,
    pub impl_name: String,
    pub path: String,
}

/// Tracey code unit with dashboard coverage details.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyApiCodeUnit {
    pub kind: String,
    pub name: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub rule_refs: Vec<String>,
}

/// Tracey file-content dashboard response.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyApiFileData {
    pub path: String,
    pub content: String,
    pub html: String,
    pub units: Vec<TraceyApiCodeUnit>,
}

/// Tracey rendered spec source section.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceySpecSection {
    pub source_file: String,
    pub html: String,
    pub weight: i32,
}

/// Tracey outline coverage counters.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyOutlineCoverage {
    pub impl_count: usize,
    pub verify_count: usize,
    pub total: usize,
}

/// Tracey rendered spec outline entry.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyOutlineEntry {
    pub title: String,
    pub slug: String,
    pub level: u8,
    pub coverage: TraceyOutlineCoverage,
    pub aggregated: TraceyOutlineCoverage,
}

/// Tracey rendered spec-content dashboard response.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyApiSpecData {
    pub name: String,
    pub sections: Vec<TraceySpecSection>,
    pub outline: Vec<TraceyOutlineEntry>,
    pub head_injections: Vec<String>,
}

/// Tracey search result item.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct TraceySearchResult {
    pub kind: String,
    pub id: String,
    pub line: usize,
    pub content: Option<String>,
    pub highlighted: Option<String>,
    pub score: f32,
}

/// Tracey inline file-range update request.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyUpdateFileRangeRequest {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub content: String,
    pub file_hash: String,
}

/// Tracey inline file update error.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyUpdateError {
    pub message: String,
}

/// Tracey config include/exclude mutation request.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyConfigPatternRequest {
    pub spec: Option<String>,
    pub impl_name: Option<String>,
    pub pattern: String,
}

/// Tracey validation request.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyValidateRequest {
    pub spec: Option<String>,
    pub impl_name: Option<String>,
}

/// Tracey validation error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum TraceyValidationErrorCode {
    CircularDependency = 0,
    InvalidNaming = 1,
    UnknownRequirement = 2,
    StaleRequirement = 3,
    DuplicateRequirement = 4,
    UnknownPrefix = 5,
    ImplInTestFile = 6,
    IncludeUnparseableFile = 7,
}

/// Tracey validation error.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyValidationError {
    pub code: TraceyValidationErrorCode,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub related_rules: Vec<TraceyRuleId>,
    pub reference_rule_id: Option<TraceyRuleId>,
    pub reference_text: Option<String>,
}

/// Tracey validation response.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyValidationResult {
    pub spec: String,
    pub impl_name: String,
    pub errors: Vec<TraceyValidationError>,
    pub warning_count: usize,
    pub error_count: usize,
}

/// Tracey LSP request with file content and cursor position.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspPositionRequest {
    pub path: String,
    pub content: String,
    pub line: u32,
    pub character: u32,
}

/// Tracey LSP references request.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspReferencesRequest {
    pub path: String,
    pub content: String,
    pub line: u32,
    pub character: u32,
    pub include_declaration: bool,
}

/// Tracey LSP document request.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspDocumentRequest {
    pub path: String,
    pub content: String,
}

/// Tracey LSP inlay-hints request.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspInlayHintsRequest {
    pub path: String,
    pub content: String,
    pub start_line: u32,
    pub end_line: u32,
}

/// Tracey LSP rename request.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspRenameRequest {
    pub path: String,
    pub content: String,
    pub line: u32,
    pub character: u32,
    pub new_name: String,
}

/// Tracey LSP location.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspLocation {
    pub path: String,
    pub line: u32,
    pub character: u32,
}

/// Tracey LSP hover response.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyHoverInfo {
    pub rule_id: TraceyRuleId,
    pub raw: String,
    pub spec_name: String,
    pub spec_url: Option<String>,
    pub source_file: Option<String>,
    pub impl_count: usize,
    pub verify_count: usize,
    pub impl_refs: Vec<TraceyCodeRef>,
    pub verify_refs: Vec<TraceyCodeRef>,
    pub range_start_line: u32,
    pub range_start_char: u32,
    pub range_end_line: u32,
    pub range_end_char: u32,
    pub version_diff: Option<String>,
}

/// Tracey LSP completion item.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspCompletionItem {
    pub label: String,
    pub kind: String,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
}

/// Tracey LSP diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspDiagnostic {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub start_line: u32,
    pub start_char: u32,
    pub end_line: u32,
    pub end_char: u32,
}

/// Tracey LSP diagnostics for one file.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspFileDiagnostics {
    pub path: String,
    pub diagnostics: Vec<TraceyLspDiagnostic>,
}

/// Tracey LSP document or workspace symbol.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspSymbol {
    pub name: String,
    pub kind: String,
    pub path: Option<String>,
    pub start_line: u32,
    pub start_char: u32,
    pub end_line: u32,
    pub end_char: u32,
}

/// Tracey LSP semantic token.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspSemanticToken {
    pub line: u32,
    pub start_char: u32,
    pub length: u32,
    pub token_type: u32,
    pub modifiers: u32,
}

/// Tracey LSP code lens.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspCodeLens {
    pub line: u32,
    pub start_char: u32,
    pub end_char: u32,
    pub title: String,
    pub command: String,
    pub arguments: Vec<String>,
}

/// Tracey LSP inlay hint.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspInlayHint {
    pub line: u32,
    pub character: u32,
    pub label: String,
}

/// Tracey LSP prepare-rename response.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyPrepareRenameResult {
    pub start_line: u32,
    pub start_char: u32,
    pub end_line: u32,
    pub end_char: u32,
    pub placeholder: String,
}

/// Tracey LSP text edit.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspTextEdit {
    pub path: String,
    pub start_line: u32,
    pub start_char: u32,
    pub end_line: u32,
    pub end_char: u32,
    pub new_text: String,
}

/// Tracey LSP code action.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyLspCodeAction {
    pub title: String,
    pub kind: String,
    pub command: String,
    pub arguments: Vec<String>,
    pub is_preferred: bool,
}

/// Tracey coverage change in a rebuild delta.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyCoverageChange {
    pub rule_id: TraceyRuleId,
    pub file: String,
    pub line: usize,
}

/// Tracey rebuild delta summary.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyDeltaSummary {
    pub newly_covered: Vec<TraceyCoverageChange>,
    pub newly_uncovered: Vec<TraceyRuleId>,
}

/// Tracey daemon data update streamed to bridge clients.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct TraceyDataUpdate {
    pub version: u64,
    pub delta: Option<TraceyDeltaSummary>,
}

vox_schema::impl_reborrow_owned!(TraceyDataUpdate);

/// Dibs schema column metadata.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsColumnInfo {
    pub name: String,
    pub sql_type: String,
    pub rust_type: Option<String>,
    pub nullable: bool,
    pub default: Option<String>,
    pub primary_key: bool,
    pub unique: bool,
    pub auto_generated: bool,
    pub long: bool,
    pub label: bool,
    pub enum_variants: Vec<String>,
    pub doc: Option<String>,
    pub lang: Option<String>,
    pub icon: Option<String>,
    pub subtype: Option<String>,
}

/// Dibs schema foreign-key metadata.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsForeignKeyInfo {
    pub columns: Vec<String>,
    pub references_table: String,
    pub references_columns: Vec<String>,
}

/// Dibs schema index column metadata.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsIndexColumnInfo {
    pub name: String,
    pub order: String,
    pub nulls: String,
}

/// Dibs schema index metadata.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsIndexInfo {
    pub name: String,
    pub columns: Vec<DibsIndexColumnInfo>,
    pub unique: bool,
    pub where_clause: Option<String>,
}

/// Dibs schema table metadata.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsTableInfo {
    pub name: String,
    pub columns: Vec<DibsColumnInfo>,
    pub foreign_keys: Vec<DibsForeignKeyInfo>,
    pub indices: Vec<DibsIndexInfo>,
    pub source_file: Option<String>,
    pub source_line: Option<u32>,
    pub doc: Option<String>,
    pub icon: Option<String>,
}

/// Dibs schema response.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsSchemaInfo {
    pub tables: Vec<DibsTableInfo>,
}

/// Dibs/Squel runtime value for backoffice queries.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DibsValue {
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

/// A row of Dibs/Squel data as field name/value pairs.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsRow {
    pub fields: Vec<DibsRowField>,
}

/// A single Dibs/Squel row field.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsRowField {
    pub name: String,
    pub value: DibsValue,
}

/// Dibs/Squel filter operators.
#[derive(Debug, Clone, Copy, PartialEq, Facet)]
#[repr(u8)]
pub enum DibsFilterOp {
    Eq = 0,
    Ne = 1,
    Lt = 2,
    Lte = 3,
    Gt = 4,
    Gte = 5,
    Like = 6,
    ILike = 7,
    IsNull = 8,
    IsNotNull = 9,
    In = 10,
    JsonGet = 11,
    JsonGetText = 12,
    Contains = 13,
    KeyExists = 14,
}

/// A single Dibs/Squel filter condition.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsFilter {
    pub field: String,
    pub op: DibsFilterOp,
    pub value: DibsValue,
    pub values: Vec<DibsValue>,
}

/// Dibs/Squel sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Facet)]
#[repr(u8)]
pub enum DibsSortDir {
    Asc = 0,
    Desc = 1,
}

/// Dibs/Squel sort clause.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsSort {
    pub field: String,
    pub dir: DibsSortDir,
}

/// Dibs/Squel list request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsListRequest {
    pub table: String,
    pub filters: Vec<DibsFilter>,
    pub sort: Vec<DibsSort>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub select: Vec<String>,
}

/// Dibs/Squel list response.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsListResponse {
    pub rows: Vec<DibsRow>,
    pub total: Option<u64>,
}

/// Dibs/Squel single-row lookup request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsGetRequest {
    pub table: String,
    pub pk: DibsValue,
}

/// Dibs/Squel row creation request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsCreateRequest {
    pub table: String,
    pub data: DibsRow,
}

/// Dibs/Squel row update request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsUpdateRequest {
    pub table: String,
    pub pk: DibsValue,
    pub data: DibsRow,
}

/// Dibs/Squel row deletion request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsDeleteRequest {
    pub table: String,
    pub pk: DibsValue,
}

/// Dibs migration-status request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsMigrationStatusRequest {
    pub database_url: String,
}

/// Dibs migration-status row.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsMigrationInfo {
    pub version: String,
    pub name: String,
    pub applied: bool,
    pub applied_at: Option<String>,
    pub source_file: Option<String>,
    pub source: Option<String>,
}

/// Dibs migration log level.
#[derive(Debug, Clone, Copy, PartialEq, Facet)]
#[repr(u8)]
pub enum DibsLogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
}

/// Dibs migration log message streamed over Vox.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsMigrationLog {
    pub level: DibsLogLevel,
    pub message: String,
    pub migration: Option<String>,
}

vox_schema::impl_reborrow_owned!(DibsMigrationLog);

/// Dibs migration request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsMigrateRequest {
    pub database_url: String,
    pub migration: Option<String>,
}

/// Dibs migration that was already applied.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsAppliedMigration {
    pub version: String,
    pub applied_at: String,
}

/// Dibs migration that ran during this request.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsRanMigration {
    pub version: String,
    pub duration_ms: u64,
}

/// Dibs migration result.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsMigrateResult {
    pub total_defined: u32,
    pub already_applied: Vec<DibsAppliedMigration>,
    pub applied: Vec<DibsRanMigration>,
    pub setup_ms: u64,
    pub total_time_ms: u64,
}

/// Dibs SQL error context.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct DibsSqlError {
    pub message: String,
    pub sql: Option<String>,
    pub position: Option<u32>,
    pub hint: Option<String>,
    pub detail: Option<String>,
    pub caller: Option<String>,
}

/// Dibs service errors.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum DibsError {
    ConnectionFailed(String) = 0,
    MigrationFailed(DibsSqlError) = 1,
    InvalidRequest(String) = 2,
    UnknownTable(String) = 3,
    UnknownColumn(String) = 4,
    QueryError(String) = 5,
}

/// A struct with various field types.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Person {
    pub name: String,
    pub age: u8,
    pub email: Option<String>,
}

/// A nested struct containing other structs.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Rectangle {
    pub top_left: Point,
    pub bottom_right: Point,
    pub label: Option<String>,
}

/// A simple enum with unit variants.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum Color {
    Red = 0,
    Green = 1,
    Blue = 2,
}

/// An enum with different payload types.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum Shape {
    Circle { radius: f64 } = 0,
    Rectangle { width: f64, height: f64 } = 1,
    Point = 2,
}

/// A deeply nested structure for testing.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Canvas {
    pub name: String,
    pub shapes: Vec<Shape>,
    pub background: Color,
}

/// A key/value attribute for the gnarly payload benchmark.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct GnarlyAttr {
    pub key: String,
    pub value: String,
}

/// A nested enum used by the gnarly payload benchmark.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum GnarlyKind {
    File {
        mime: String,
        tags: Vec<String>,
    } = 0,
    Directory {
        child_count: u32,
        children: Vec<String>,
    } = 1,
    Symlink {
        target: String,
        hops: Vec<u32>,
    } = 2,
}

/// An entry inside the gnarly payload benchmark fixture.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct GnarlyEntry {
    pub id: u64,
    pub parent: Option<u64>,
    pub name: String,
    pub path: String,
    pub attrs: Vec<GnarlyAttr>,
    pub chunks: Vec<Vec<u8>>,
    pub kind: GnarlyKind,
}

/// A deep, heterogenous payload for transport and codec benchmarking.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct GnarlyPayload {
    pub revision: u64,
    pub mount: String,
    pub entries: Vec<GnarlyEntry>,
    pub footer: Option<String>,
    pub digest: Vec<u8>,
}

/// An enum with newtype variants.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum Message {
    Text(String) = 0,
    Number(i64) = 1,
    Data(Vec<u8>) = 2,
}

// ============================================================================
// Schema evolution types (v1 — the "original" definitions)
// ============================================================================

/// Tests added optional field: v1 has {name, bio}, v2 adds {avatar: `Option<String>`}.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Profile {
    pub name: String,
    pub bio: String,
}

/// Tests field reordering: v1 has {alpha, beta, gamma}, v2 reorders to {gamma, alpha, beta}.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Record {
    pub alpha: i32,
    pub beta: String,
    pub gamma: f64,
}

/// Tests added enum variant: v1 has {Active, Inactive}, v2 adds {Suspended}.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum Status {
    Active = 0,
    Inactive = 1,
}

/// Tests removed field: v1 has {label, priority, note}, v2 drops {note}.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Tag {
    pub label: String,
    pub priority: u32,
    pub note: String,
}

/// Tests incompatible type change: v1 has {value: f64}, v2 changes to {value: String}.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Measurement {
    pub unit: String,
    pub value: f64,
}

/// Tests missing required field: v1 has {key, value}, v2 adds required {owner: String}.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Config {
    pub key: String,
    pub value: String,
}

// ============================================================================
// Error types for testing User(E) error path
// ============================================================================

/// Error from math operations.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum MathError {
    DivisionByZero = 0,
    Overflow = 1,
}

/// Error from lookup operations.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum LookupError {
    NotFound = 0,
    AccessDenied = 1,
}

pub fn all_services() -> Vec<&'static vox::ServiceDescriptor> {
    vec![testbed_service_descriptor()]
}

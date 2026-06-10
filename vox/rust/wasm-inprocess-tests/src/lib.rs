//! Browser tests for vox in-process transport (Rust WASM acceptor).
//!
//! This crate only compiles for wasm32 target. Build with:
//! ```
//! wasm-pack build --target web rust/wasm-inprocess-tests
//! ```

#![cfg(target_arch = "wasm32")]

use spec_proto::*;
use vox_core::acceptor_on;
use vox_inprocess::JsInProcessLink;
use vox_types::{Rx, Tx};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    fn error(s: &str);
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format!($($t)*)))
}

macro_rules! console_error {
    ($($t:tt)*) => (error(&format!($($t)*)))
}

macro_rules! unsupported_smoke_method {
    ($name:literal) => {
        panic!(concat!(
            $name,
            " is not part of the wasm in-process browser smoke fixture"
        ))
    };
}

#[derive(Clone)]
struct TestbedService;

impl Testbed for TestbedService {
    async fn echo(&self, message: String) -> String {
        message
    }

    async fn reverse(&self, message: String) -> String {
        message.chars().rev().collect()
    }

    async fn divide(&self, dividend: i64, divisor: i64) -> Result<i64, MathError> {
        if divisor == 0 {
            Err(MathError::DivisionByZero)
        } else {
            Ok(dividend / divisor)
        }
    }

    async fn lookup(&self, id: u32) -> Result<Person, LookupError> {
        match id {
            1 => Ok(Person {
                name: "Alice".to_string(),
                age: 30,
                email: Some("alice@example.com".to_string()),
            }),
            2 => Ok(Person {
                name: "Bob".to_string(),
                age: 25,
                email: None,
            }),
            3 => Ok(Person {
                name: "Charlie".to_string(),
                age: 35,
                email: Some("charlie@example.com".to_string()),
            }),
            _ => Err(LookupError::NotFound),
        }
    }

    async fn sum(&self, mut numbers: Rx<i32>) -> i64 {
        let mut total = 0_i64;
        while let Ok(Some(n)) = numbers.recv().await {
            total += *n.get() as i64;
        }
        total
    }

    async fn generate(&self, count: u32, output: Tx<i32>) {
        for value in 0..count {
            let _ = output.send(value as i32).await;
        }
        let _ = output.close(Default::default()).await;
    }

    async fn transform(&self, mut input: Rx<String>, output: Tx<String>) {
        while let Ok(Some(item)) = input.recv().await {
            let _ = output.send(item.get().to_uppercase()).await;
        }
        let _ = output.close(Default::default()).await;
    }

    async fn post_reply_generate(&self, output: Tx<i32>) {
        wasm_bindgen_futures::spawn_local(async move {
            for value in 0..5 {
                let _ = output.send(value).await;
            }
            let _ = output.close(Default::default()).await;
        });
    }

    async fn post_reply_sum(&self, mut input: Rx<i32>, result: Tx<i64>) {
        wasm_bindgen_futures::spawn_local(async move {
            let mut total = 0_i64;
            while let Ok(Some(value)) = input.recv().await {
                total += *value.get() as i64;
            }
            let _ = result.send(total).await;
            let _ = result.close(Default::default()).await;
        });
    }

    async fn echo_point(&self, point: Point) -> Point {
        point
    }

    async fn create_person(&self, name: String, age: u8, email: Option<String>) -> Person {
        Person { name, age, email }
    }

    async fn rectangle_area(&self, rect: Rectangle) -> f64 {
        let width = (rect.bottom_right.x - rect.top_left.x).abs() as f64;
        let height = (rect.bottom_right.y - rect.top_left.y).abs() as f64;
        width * height
    }

    async fn parse_color(&self, name: String) -> Option<Color> {
        match name.to_lowercase().as_str() {
            "red" => Some(Color::Red),
            "green" => Some(Color::Green),
            "blue" => Some(Color::Blue),
            _ => None,
        }
    }

    async fn shape_area(&self, shape: Shape) -> f64 {
        match shape {
            Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Point => 0.0,
        }
    }

    async fn create_canvas(&self, name: String, shapes: Vec<Shape>, background: Color) -> Canvas {
        Canvas {
            name,
            shapes,
            background,
        }
    }

    async fn process_message(&self, msg: Message) -> Message {
        match msg {
            Message::Text(text) => Message::Text(format!("processed: {}", text)),
            Message::Number(n) => Message::Number(n * 2),
            Message::Data(data) => Message::Data(data.into_iter().rev().collect()),
        }
    }

    async fn get_points(&self, count: u32) -> Vec<Point> {
        (0..count as i32)
            .map(|i| Point { x: i, y: i * 2 })
            .collect()
    }

    async fn swap_pair(&self, pair: (i32, String)) -> (String, i32) {
        (pair.1, pair.0)
    }

    async fn echo_bytes(&self, data: Vec<u8>) -> Vec<u8> {
        data
    }

    async fn echo_bool(&self, b: bool) -> bool {
        b
    }

    async fn echo_u64(&self, n: u64) -> u64 {
        n
    }

    async fn echo_option_string(&self, s: Option<String>) -> Option<String> {
        s
    }

    async fn sum_large(&self, mut numbers: Rx<i32>) -> i64 {
        let mut total = 0_i64;
        while let Ok(Some(n)) = numbers.recv().await {
            total += *n.get() as i64;
        }
        total
    }

    async fn generate_large(&self, count: u32, output: Tx<i32>) {
        self.generate(count, output).await;
    }

    async fn all_colors(&self) -> Vec<Color> {
        vec![Color::Red, Color::Green, Color::Blue]
    }

    async fn describe_point(&self, label: String, x: i32, y: i32, active: bool) -> TaggedPoint {
        TaggedPoint {
            label,
            x,
            y,
            active,
        }
    }

    async fn echo_shape(&self, shape: Shape) -> Shape {
        shape
    }

    async fn echo_status_v1(&self, status: Status) -> Status {
        status
    }

    async fn echo_tag_v1(&self, tag: Tag) -> Tag {
        tag
    }

    async fn echo_profile(&self, profile: Profile) -> Profile {
        profile
    }

    async fn echo_record(&self, record: Record) -> Record {
        record
    }

    async fn echo_status(&self, status: Status) -> Status {
        status
    }

    async fn echo_tag(&self, tag: Tag) -> Tag {
        tag
    }

    async fn echo_measurement(&self, m: Measurement) -> Measurement {
        m
    }

    async fn echo_config(&self, c: Config) -> Config {
        c
    }

    async fn echo_gnarly(&self, payload: GnarlyPayload) -> GnarlyPayload {
        payload
    }

    async fn echo_tree(&self, tree: Tree) -> Tree {
        tree
    }

    async fn echo_ecosystem_bridge(
        &self,
        payload: EcosystemBridgePayload,
    ) -> EcosystemBridgePayload {
        payload
    }

    async fn dodeca_byte_tunnel(&self, _inbound: Rx<Vec<u8>>, _outbound: Tx<Vec<u8>>) {
        unsupported_smoke_method!("dodeca_byte_tunnel")
    }

    async fn dodeca_devtools_lsp(
        &self,
        _token: String,
        _client_to_server: Rx<String>,
        _server_to_client: Tx<String>,
    ) {
        unsupported_smoke_method!("dodeca_devtools_lsp")
    }

    async fn dibs_schema(&self) -> DibsSchemaInfo {
        unsupported_smoke_method!("dibs_schema")
    }

    async fn dibs_list(&self, _request: DibsListRequest) -> Result<DibsListResponse, DibsError> {
        unsupported_smoke_method!("dibs_list")
    }

    async fn dibs_get(&self, _request: DibsGetRequest) -> Result<Option<DibsRow>, DibsError> {
        unsupported_smoke_method!("dibs_get")
    }

    async fn dibs_create(&self, _request: DibsCreateRequest) -> Result<DibsRow, DibsError> {
        unsupported_smoke_method!("dibs_create")
    }

    async fn dibs_update(&self, _request: DibsUpdateRequest) -> Result<DibsRow, DibsError> {
        unsupported_smoke_method!("dibs_update")
    }

    async fn dibs_delete(&self, _request: DibsDeleteRequest) -> Result<u64, DibsError> {
        unsupported_smoke_method!("dibs_delete")
    }

    async fn dibs_migration_status(
        &self,
        _request: DibsMigrationStatusRequest,
    ) -> Result<Vec<DibsMigrationInfo>, DibsError> {
        unsupported_smoke_method!("dibs_migration_status")
    }

    async fn dibs_migrate(
        &self,
        _request: DibsMigrateRequest,
        _logs: Tx<DibsMigrationLog>,
    ) -> Result<DibsMigrateResult, DibsError> {
        unsupported_smoke_method!("dibs_migrate")
    }

    async fn echo_dodeca_template_call(&self, call: DodecaTemplateCall) -> DodecaTemplateCall {
        call
    }

    async fn dodeca_html_process(&self, _input: DodecaHtmlProcessInput) -> DodecaHtmlProcessResult {
        unsupported_smoke_method!("dodeca_html_process")
    }

    async fn dodeca_execute_code_samples(
        &self,
        _input: DodecaExecuteSamplesInput,
    ) -> DodecaCodeExecutionResult {
        unsupported_smoke_method!("dodeca_execute_code_samples")
    }

    async fn dodeca_load_data(
        &self,
        _content: String,
        _format: DodecaDataFormat,
    ) -> DodecaLoadDataResult {
        unsupported_smoke_method!("dodeca_load_data")
    }

    async fn dodeca_parse_and_render(
        &self,
        _source_path: String,
        _content: String,
        _source_map: bool,
    ) -> DodecaParseResult {
        unsupported_smoke_method!("dodeca_parse_and_render")
    }

    async fn echo_dodeca_image_processor_fixture(
        &self,
        fixture: DodecaImageProcessorFixture,
    ) -> DodecaImageProcessorFixture {
        fixture
    }

    async fn echo_dodeca_search_indexer_fixture(
        &self,
        fixture: DodecaSearchIndexerFixture,
    ) -> DodecaSearchIndexerFixture {
        fixture
    }

    async fn echo_dodeca_asset_processing_fixture(
        &self,
        fixture: DodecaAssetProcessingFixture,
    ) -> DodecaAssetProcessingFixture {
        fixture
    }

    async fn echo_dodeca_small_cell_services_fixture(
        &self,
        fixture: DodecaSmallCellServicesFixture,
    ) -> DodecaSmallCellServicesFixture {
        fixture
    }

    async fn echo_dodeca_devtools_event(&self, event: DodecaDevtoolsEvent) -> DodecaDevtoolsEvent {
        event
    }

    async fn dodeca_devtools_get_scope(&self, _path: Option<Vec<String>>) -> Vec<DodecaScopeEntry> {
        unsupported_smoke_method!("dodeca_devtools_get_scope")
    }

    async fn dodeca_devtools_eval(
        &self,
        _snapshot_id: String,
        _expression: String,
    ) -> DodecaEvalResult {
        unsupported_smoke_method!("dodeca_devtools_eval")
    }

    async fn dodeca_devtools_open_dead_link(
        &self,
        _route: String,
        _target: DodecaDeadLinkTarget,
    ) -> DodecaOpenSourceResult {
        unsupported_smoke_method!("dodeca_devtools_open_dead_link")
    }

    async fn dodeca_devtools_edit_load(&self, _token: String, _route: String) -> DodecaEditLoad {
        unsupported_smoke_method!("dodeca_devtools_edit_load")
    }

    async fn dodeca_devtools_edit_preview(
        &self,
        _token: String,
        _source_key: String,
        _buffer: String,
    ) -> DodecaEditPreview {
        unsupported_smoke_method!("dodeca_devtools_edit_preview")
    }

    async fn dodeca_devtools_edit_save(
        &self,
        _token: String,
        _req: DodecaEditSaveReq,
    ) -> DodecaEditSave {
        unsupported_smoke_method!("dodeca_devtools_edit_save")
    }

    async fn dodeca_devtools_edit_upload(
        &self,
        _token: String,
        _req: DodecaEditUploadReq,
    ) -> DodecaEditUpload {
        unsupported_smoke_method!("dodeca_devtools_edit_upload")
    }

    async fn dodeca_devtools_edit_read(&self, _token: String, _uri: String) -> DodecaEditRead {
        unsupported_smoke_method!("dodeca_devtools_edit_read")
    }

    async fn dodeca_devtools_edit_list(&self, _token: String) -> DodecaEditList {
        unsupported_smoke_method!("dodeca_devtools_edit_list")
    }

    async fn echo_styx_value(&self, value: StyxValue) -> StyxValue {
        value
    }

    async fn styx_lsp_initialize(
        &self,
        _params: StyxLspInitializeParams,
    ) -> StyxLspInitializeResult {
        unsupported_smoke_method!("styx_lsp_initialize")
    }

    async fn styx_lsp_completions(
        &self,
        _params: StyxLspCompletionParams,
    ) -> Vec<StyxLspCompletionItem> {
        unsupported_smoke_method!("styx_lsp_completions")
    }

    async fn styx_lsp_hover(&self, _params: StyxLspHoverParams) -> Option<StyxLspHoverResult> {
        unsupported_smoke_method!("styx_lsp_hover")
    }

    async fn styx_lsp_inlay_hints(&self, _params: StyxLspInlayHintParams) -> Vec<StyxLspInlayHint> {
        unsupported_smoke_method!("styx_lsp_inlay_hints")
    }

    async fn styx_lsp_diagnostics(
        &self,
        _params: StyxLspDiagnosticParams,
    ) -> Vec<StyxLspDiagnostic> {
        unsupported_smoke_method!("styx_lsp_diagnostics")
    }

    async fn styx_lsp_code_actions(
        &self,
        _params: StyxLspCodeActionParams,
    ) -> Vec<StyxLspCodeAction> {
        unsupported_smoke_method!("styx_lsp_code_actions")
    }

    async fn styx_lsp_definition(&self, _params: StyxLspDefinitionParams) -> Vec<StyxLspLocation> {
        unsupported_smoke_method!("styx_lsp_definition")
    }

    async fn styx_lsp_shutdown(&self) {}

    async fn styx_host_get_subtree(&self, _params: StyxLspGetSubtreeParams) -> Option<StyxValue> {
        unsupported_smoke_method!("styx_host_get_subtree")
    }

    async fn styx_host_get_document(&self, _params: StyxLspGetDocumentParams) -> Option<StyxValue> {
        unsupported_smoke_method!("styx_host_get_document")
    }

    async fn styx_host_get_source(&self, _params: StyxLspGetSourceParams) -> Option<String> {
        unsupported_smoke_method!("styx_host_get_source")
    }

    async fn styx_host_get_schema(
        &self,
        _params: StyxLspGetSchemaParams,
    ) -> Option<StyxLspSchemaInfo> {
        unsupported_smoke_method!("styx_host_get_schema")
    }

    async fn styx_host_offset_to_position(
        &self,
        _params: StyxLspOffsetToPositionParams,
    ) -> Option<StyxLspPosition> {
        unsupported_smoke_method!("styx_host_offset_to_position")
    }

    async fn styx_host_position_to_offset(
        &self,
        _params: StyxLspPositionToOffsetParams,
    ) -> Option<u32> {
        unsupported_smoke_method!("styx_host_position_to_offset")
    }

    async fn stax_flamegraph(&self, _params: StaxViewParams) -> StaxFlamegraphUpdate {
        unsupported_smoke_method!("stax_flamegraph")
    }

    async fn echo_stax_flamegraph_update(
        &self,
        update: StaxFlamegraphUpdate,
    ) -> StaxFlamegraphUpdate {
        update
    }

    async fn stax_subscribe_flamegraph_updates(&self, _output: Tx<StaxFlamegraphUpdate>) {
        unsupported_smoke_method!("stax_subscribe_flamegraph_updates")
    }

    async fn echo_stax_linux_broker_control(
        &self,
        fixture: StaxLinuxBrokerControlFixture,
    ) -> StaxLinuxBrokerControlFixture {
        fixture
    }

    async fn stax_macos_record(
        &self,
        _config: StaxMacSessionConfig,
        _records: Tx<StaxMacKdBufBatch>,
    ) -> Result<StaxMacRecordSummary, StaxMacRecordError> {
        unsupported_smoke_method!("stax_macos_record")
    }

    async fn echo_hotmeal_live_reload_event(
        &self,
        event: HotmealLiveReloadEvent,
    ) -> HotmealLiveReloadEvent {
        event
    }

    async fn echo_hotmeal_apply_patches_result(
        &self,
        result: HotmealApplyPatchesResult,
    ) -> HotmealApplyPatchesResult {
        result
    }

    async fn hotmeal_live_reload_subscribe(&self, _route: String) {
        unsupported_smoke_method!("hotmeal_live_reload_subscribe")
    }

    async fn hotmeal_live_reload_on_event(&self, _event: HotmealLiveReloadEvent) {
        unsupported_smoke_method!("hotmeal_live_reload_on_event")
    }

    async fn echo_helix_stream_metrics(&self, metrics: HelixStreamMetrics) -> HelixStreamMetrics {
        metrics
    }

    async fn echo_helix_verify_evidence(
        &self,
        digest: HelixVerifyEvidenceDigest,
    ) -> HelixVerifyEvidenceDigest {
        digest
    }

    async fn helix_subscribe_pulses(&self, _output: Tx<HelixPulseAvailable>) {
        unsupported_smoke_method!("helix_subscribe_pulses")
    }

    async fn helix_pulse_bundle(
        &self,
        _pulse_id: HelixSchedulerPulseId,
        _fields: HelixPulseBundleFields,
    ) -> HelixPulseBundle {
        unsupported_smoke_method!("helix_pulse_bundle")
    }

    async fn helix_trace_service_surface(&self) -> HelixTraceServiceSurface {
        unsupported_smoke_method!("helix_trace_service_surface")
    }

    async fn tracey_status(&self) -> TraceyStatusResponse {
        unsupported_smoke_method!("tracey_status")
    }

    async fn tracey_uncovered(&self, _req: TraceyUncoveredRequest) -> TraceyUncoveredResponse {
        unsupported_smoke_method!("tracey_uncovered")
    }

    async fn tracey_untested(&self, _req: TraceyUntestedRequest) -> TraceyUntestedResponse {
        unsupported_smoke_method!("tracey_untested")
    }

    async fn tracey_stale(&self, _req: TraceyStaleRequest) -> TraceyStaleResponse {
        unsupported_smoke_method!("tracey_stale")
    }

    async fn tracey_unmapped(&self, _req: TraceyUnmappedRequest) -> TraceyUnmappedResponse {
        unsupported_smoke_method!("tracey_unmapped")
    }

    async fn tracey_rule(&self, _rule_id: TraceyRuleId) -> Option<TraceyRuleInfo> {
        unsupported_smoke_method!("tracey_rule")
    }

    async fn tracey_forward(
        &self,
        _spec: String,
        _impl_name: String,
    ) -> Option<TraceyApiSpecForward> {
        unsupported_smoke_method!("tracey_forward")
    }

    async fn tracey_reverse(
        &self,
        _spec: String,
        _impl_name: String,
    ) -> Option<TraceyApiReverseData> {
        unsupported_smoke_method!("tracey_reverse")
    }

    async fn tracey_file(&self, _req: TraceyFileRequest) -> Option<TraceyApiFileData> {
        unsupported_smoke_method!("tracey_file")
    }

    async fn tracey_spec_content(
        &self,
        _spec: String,
        _impl_name: String,
    ) -> Option<TraceyApiSpecData> {
        unsupported_smoke_method!("tracey_spec_content")
    }

    async fn tracey_search(&self, _query: String, _limit: u32) -> Vec<TraceySearchResult> {
        unsupported_smoke_method!("tracey_search")
    }

    async fn tracey_update_file_range(
        &self,
        _req: TraceyUpdateFileRangeRequest,
    ) -> Result<(), TraceyUpdateError> {
        unsupported_smoke_method!("tracey_update_file_range")
    }

    async fn tracey_config_add_exclude(
        &self,
        _req: TraceyConfigPatternRequest,
    ) -> Result<(), String> {
        unsupported_smoke_method!("tracey_config_add_exclude")
    }

    async fn tracey_config_add_include(
        &self,
        _req: TraceyConfigPatternRequest,
    ) -> Result<(), String> {
        unsupported_smoke_method!("tracey_config_add_include")
    }

    async fn tracey_config(&self) -> TraceyApiConfig {
        unsupported_smoke_method!("tracey_config")
    }

    async fn tracey_vfs_open(&self, _path: String, _content: String) {
        unsupported_smoke_method!("tracey_vfs_open")
    }

    async fn tracey_vfs_change(&self, _path: String, _content: String) {
        unsupported_smoke_method!("tracey_vfs_change")
    }

    async fn tracey_vfs_close(&self, _path: String) {
        unsupported_smoke_method!("tracey_vfs_close")
    }

    async fn tracey_reload(&self) -> TraceyReloadResponse {
        unsupported_smoke_method!("tracey_reload")
    }

    async fn tracey_version(&self) -> u64 {
        unsupported_smoke_method!("tracey_version")
    }

    async fn tracey_health(&self) -> TraceyHealthResponse {
        unsupported_smoke_method!("tracey_health")
    }

    async fn tracey_shutdown(&self) {}

    async fn tracey_validate(&self, _req: TraceyValidateRequest) -> TraceyValidationResult {
        unsupported_smoke_method!("tracey_validate")
    }

    async fn tracey_is_test_file(&self, _path: String) -> bool {
        unsupported_smoke_method!("tracey_is_test_file")
    }

    async fn tracey_lsp_hover(&self, _req: TraceyLspPositionRequest) -> Option<TraceyHoverInfo> {
        unsupported_smoke_method!("tracey_lsp_hover")
    }

    async fn tracey_lsp_definition(
        &self,
        _req: TraceyLspPositionRequest,
    ) -> Vec<TraceyLspLocation> {
        unsupported_smoke_method!("tracey_lsp_definition")
    }

    async fn tracey_lsp_implementation(
        &self,
        _req: TraceyLspPositionRequest,
    ) -> Vec<TraceyLspLocation> {
        unsupported_smoke_method!("tracey_lsp_implementation")
    }

    async fn tracey_lsp_references(
        &self,
        _req: TraceyLspReferencesRequest,
    ) -> Vec<TraceyLspLocation> {
        unsupported_smoke_method!("tracey_lsp_references")
    }

    async fn tracey_lsp_completions(
        &self,
        _req: TraceyLspPositionRequest,
    ) -> Vec<TraceyLspCompletionItem> {
        unsupported_smoke_method!("tracey_lsp_completions")
    }

    async fn tracey_lsp_workspace_diagnostics(&self) -> Vec<TraceyLspFileDiagnostics> {
        unsupported_smoke_method!("tracey_lsp_workspace_diagnostics")
    }

    async fn tracey_lsp_document_symbols(
        &self,
        _req: TraceyLspDocumentRequest,
    ) -> Vec<TraceyLspSymbol> {
        unsupported_smoke_method!("tracey_lsp_document_symbols")
    }

    async fn tracey_lsp_workspace_symbols(&self, _query: String) -> Vec<TraceyLspSymbol> {
        unsupported_smoke_method!("tracey_lsp_workspace_symbols")
    }

    async fn tracey_lsp_semantic_tokens(
        &self,
        _req: TraceyLspDocumentRequest,
    ) -> Vec<TraceyLspSemanticToken> {
        unsupported_smoke_method!("tracey_lsp_semantic_tokens")
    }

    async fn tracey_lsp_code_lens(&self, _req: TraceyLspDocumentRequest) -> Vec<TraceyLspCodeLens> {
        unsupported_smoke_method!("tracey_lsp_code_lens")
    }

    async fn tracey_lsp_inlay_hints(
        &self,
        _req: TraceyLspInlayHintsRequest,
    ) -> Vec<TraceyLspInlayHint> {
        unsupported_smoke_method!("tracey_lsp_inlay_hints")
    }

    async fn tracey_lsp_prepare_rename(
        &self,
        _req: TraceyLspPositionRequest,
    ) -> Option<TraceyPrepareRenameResult> {
        unsupported_smoke_method!("tracey_lsp_prepare_rename")
    }

    async fn tracey_lsp_rename(&self, _req: TraceyLspRenameRequest) -> Vec<TraceyLspTextEdit> {
        unsupported_smoke_method!("tracey_lsp_rename")
    }

    async fn tracey_lsp_code_actions(
        &self,
        _req: TraceyLspPositionRequest,
    ) -> Vec<TraceyLspCodeAction> {
        unsupported_smoke_method!("tracey_lsp_code_actions")
    }

    async fn tracey_lsp_document_highlight(
        &self,
        _req: TraceyLspPositionRequest,
    ) -> Vec<TraceyLspLocation> {
        unsupported_smoke_method!("tracey_lsp_document_highlight")
    }

    async fn tracey_subscribe_updates(&self, _updates: Tx<TraceyDataUpdate>) {
        unsupported_smoke_method!("tracey_subscribe_updates")
    }
}

/// Start a vox acceptor (server) using the in-process transport.
///
/// Returns a `JsInProcessLink` that JS should wire to an `InProcessTransport`.
/// The acceptor runs in the background via `wasm_bindgen_futures::spawn_local`.
// r[verify transport.inprocess]
// r[verify transport.inprocess.platforms]
#[wasm_bindgen]
pub fn start_acceptor(on_message: js_sys::Function) -> JsInProcessLink {
    let mut js_link = JsInProcessLink::new(on_message);
    let link = js_link
        .take_link()
        .expect("take_link should succeed on fresh JsInProcessLink");

    wasm_bindgen_futures::spawn_local(async move {
        console_log!("In-process acceptor: starting handshake...");

        match acceptor_on(link)
            .on_connection(TestbedDispatcher::new(TestbedService))
            .establish::<TestbedClient>()
            .await
        {
            Ok(_root_caller_guard) => {
                console_log!("In-process acceptor: session established");
                // Keep the session alive
                std::future::pending::<()>().await;
            }
            Err(e) => {
                console_error!("In-process acceptor: handshake failed: {:?}", e);
            }
        }
    });

    js_link
}

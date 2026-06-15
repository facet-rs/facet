/// Swift subject binary for the vox compliance suite.
///
/// This uses the vox-runtime library to validate that the Swift implementation
/// is compliant with the vox protocol spec.

import Foundation
import PhonSchema
import VoxRuntime

extension String: @retroactive Error {}

extension TraceyUpdateError: Error {}

// MARK: - Testbed Service Implementation

/// Implementation of the Testbed service.
struct TestbedService: TestbedHandler {
    private func streamValues(
        count: UInt32,
        output: Tx<Int32>
    ) async throws {
        for i in 0..<Int32(count) {
            log("  sending: \(i)")
            try await output.send(i)
        }
    }

    func echo(message: String) async throws -> String {
        return message
    }

    func reverse(message: String) async throws -> String {
        log("reverse called: \(message)")
        return String(message.reversed())
    }

    func divide(dividend: Int64, divisor: Int64) async throws -> Result<Int64, MathError> {
        log("divide called: \(dividend) / \(divisor)")
        if divisor == 0 {
            return .failure(.divisionByZero)
        }
        if dividend == .min && divisor == -1 {
            return .failure(.overflow)
        }
        return .success(dividend / divisor)
    }

    func lookup(id: UInt32) async throws -> Result<Person, LookupError> {
        log("lookup called: \(id)")
        switch id {
        case 1:
            return .success(Person(name: "Alice", age: 30, email: "alice@example.com"))
        case 2:
            return .success(Person(name: "Bob", age: 25, email: nil))
        case 3:
            return .success(Person(name: "Charlie", age: 35, email: "charlie@example.com"))
        case 100...199:
            return .failure(.accessDenied)
        default:
            return .failure(.notFound)
        }
    }

    func sum(numbers: Rx<Int32>) async throws -> Int64 {
        log("sum called, starting to receive numbers")
        var total: Int64 = 0
        for try await n in numbers {
            log("  received: \(n)")
            total += Int64(n)
        }
        log("sum complete: \(total)")
        return total
    }

    func generate(count: UInt32, output: Tx<Int32>) async throws {
        log("generate called: count=\(count)")
        try await streamValues(count: count, output: output)
        log("generate complete, about to return (close will be sent by dispatcher)")
    }

    func transform(input: Rx<String>, output: Tx<String>) async throws {
        log("transform called")
        for try await s in input {
            log("  transforming: \(s)")
            try await output.send(s)
        }
        log("transform complete")
    }

    func dodecaByteTunnel(inbound: Rx<Data>, outbound: Tx<Data>) async throws {
        log("dodecaByteTunnel called")
        for try await chunk in inbound {
            try await outbound.send(chunk)
        }
        log("dodecaByteTunnel complete")
    }

    func dodecaDevtoolsLsp(
        token: String,
        clientToServer: Rx<String>,
        serverToClient: Tx<String>
    ) async throws {
        log("dodecaDevtoolsLsp called")
        guard token == "editor-token" else {
            serverToClient.close()
            return
        }
        for try await chunk in clientToServer {
            try await serverToClient.send("lsp:\(chunk)")
        }
        log("dodecaDevtoolsLsp complete")
    }

    func dibsList(request: DibsListRequest) async throws -> Result<DibsListResponse, DibsError> {
        guard sameReflecting(request, sampleDibsListRequest()) else {
            return .failure(.unknownTable(request.table))
        }
        return .success(sampleDibsListResponse())
    }

    func dibsSchema() async throws -> DibsSchemaInfo {
        sampleDibsSchema()
    }

    func dibsGet(request: DibsGetRequest) async throws -> Result<DibsRow?, DibsError> {
        guard sameReflecting(request, sampleDibsGetRequest()) else {
            return .failure(.invalidRequest("unexpected get request"))
        }
        return .success(sampleDibsRowOne())
    }

    func dibsCreate(request: DibsCreateRequest) async throws -> Result<DibsRow, DibsError> {
        guard sameReflecting(request, sampleDibsCreateRequest()) else {
            return .failure(.invalidRequest("unexpected create request"))
        }
        return .success(sampleDibsCreateResponse())
    }

    func dibsUpdate(request: DibsUpdateRequest) async throws -> Result<DibsRow, DibsError> {
        guard sameReflecting(request, sampleDibsUpdateRequest()) else {
            return .failure(.invalidRequest("unexpected update request"))
        }
        return .success(sampleDibsUpdateResponse())
    }

    func dibsDelete(request: DibsDeleteRequest) async throws -> Result<UInt64, DibsError> {
        guard sameReflecting(request, sampleDibsDeleteRequest()) else {
            return .failure(.invalidRequest("unexpected delete request"))
        }
        return .success(1)
    }

    func dibsMigrationStatus(
        request: DibsMigrationStatusRequest
    ) async throws -> Result<[DibsMigrationInfo], DibsError> {
        guard sameReflecting(request, sampleDibsMigrationStatusRequest()) else {
            return .failure(.invalidRequest("unexpected migration status request"))
        }
        return .success(sampleDibsMigrationStatus())
    }

    func dibsMigrate(
        request: DibsMigrateRequest,
        logs: Tx<DibsMigrationLog>
    ) async throws -> Result<DibsMigrateResult, DibsError> {
        guard sameReflecting(request, sampleDibsMigrateRequest()) else {
            return .failure(.invalidRequest("unexpected migrate request"))
        }
        for logEntry in sampleDibsLogs() {
            do {
                try await logs.send(logEntry)
            } catch {
                break
            }
        }
        logs.close()
        return .success(sampleDibsMigrateResult())
    }

    func postReplyGenerate(output: Tx<Int32>) async throws {
        log("postReplyGenerate called")
        Task {
            try? await Task.sleep(nanoseconds: 10_000_000)
            for i in 0..<Int32(5) {
                do {
                    try await output.send(i)
                } catch {
                    log("postReplyGenerate send failed: \(error)")
                    break
                }
            }
            output.close()
        }
    }

    func postReplySum(input: Rx<Int32>, result: Tx<Int64>) async throws {
        log("postReplySum called")
        Task {
            var total: Int64 = 0
            do {
                for try await n in input {
                    total += Int64(n)
                }
                try await result.send(total)
            } catch {
                log("postReplySum failed: \(error)")
            }
            result.close()
        }
    }

    func echoPoint(point: Point) async throws -> Point {
        return point
    }

    func createPerson(name: String, age: UInt8, email: String?) async throws -> Person {
        return Person(name: name, age: age, email: email)
    }

    func rectangleArea(rect: Rectangle) async throws -> Double {
        let width = abs(Double(rect.bottomRight.x - rect.topLeft.x))
        let height = abs(Double(rect.bottomRight.y - rect.topLeft.y))
        return width * height
    }

    func parseColor(name: String) async throws -> Color? {
        switch name.lowercased() {
        case "red": return .red
        case "green": return .green
        case "blue": return .blue
        default: return nil
        }
    }

    func shapeArea(shape: Shape) async throws -> Double {
        switch shape {
        case .circle(let radius):
            return Double.pi * radius * radius
        case .rectangle(let width, let height):
            return width * height
        case .point:
            return 0.0
        }
    }

    func createCanvas(name: String, shapes: [Shape], background: Color) async throws -> Canvas {
        return Canvas(name: name, shapes: shapes, background: background)
    }

    func echoGnarly(payload: GnarlyPayload) async throws -> GnarlyPayload {
        payload
    }

    func processMessage(msg: Message) async throws -> Message {
        switch msg {
        case .text(let s):
            return .text("processed: \(s)")
        case .number(let n):
            return .number(n * 2)
        case .data(let d):
            return .data(Data(d.reversed()))
        }
    }

    func getPoints(count: UInt32) async throws -> [Point] {
        return (0..<Int32(count)).map { Point(x: $0, y: $0 * 2) }
    }

    func swapPair(pair: (Int32, String)) async throws -> (String, Int32) {
        return (pair.1, pair.0)
    }

    func echoBytes(data: Data) async throws -> Data {
        data
    }

    func echoBool(b: Bool) async throws -> Bool {
        b
    }

    func echoU64(n: UInt64) async throws -> UInt64 {
        n
    }

    func echoOptionString(s: String?) async throws -> String? {
        s
    }

    func sumLarge(numbers: Rx<Int32>) async throws -> Int64 {
        try await sum(numbers: numbers)
    }

    func generateLarge(count: UInt32, output: Tx<Int32>) async throws {
        try await generate(count: count, output: output)
    }

    func allColors() async throws -> [Color] {
        [.red, .green, .blue]
    }

    func describePoint(label: String, x: Int32, y: Int32, active: Bool) async throws -> TaggedPoint
    {
        TaggedPoint(label: label, x: x, y: y, active: active)
    }

    func echoShape(shape: Shape) async throws -> Shape {
        shape
    }

    func echoStatusV1(status: Status) async throws -> Status {
        status
    }

    func echoTagV1(tag: Tag) async throws -> Tag {
        tag
    }

    func echoProfile(profile: Profile) async throws -> Profile {
        profile
    }

    func echoRecord(record: Record) async throws -> Record {
        record
    }

    func echoStatus(status: Status) async throws -> Status {
        status
    }

    func echoTag(tag: Tag) async throws -> Tag {
        tag
    }

    func echoMeasurement(m: Measurement) async throws -> Measurement {
        m
    }

    func echoConfig(c: Config) async throws -> Config {
        c
    }

    func echoTree(tree: Tree) async throws -> Tree {
        tree
    }

    func echoEcosystemBridge(payload: EcosystemBridgePayload) async throws -> EcosystemBridgePayload
    {
        payload
    }

    func echoDodecaTemplateCall(call: DodecaTemplateCall) async throws -> DodecaTemplateCall {
        call
    }

    func dodecaHtmlProcess(input: DodecaHtmlProcessInput) async throws
        -> DodecaHtmlProcessResult
    {
        if sameDodecaHtmlProcessInput(input, sampleDodecaHtmlProcessInput()) {
            return sampleDodecaHtmlProcessResult()
        }
        return .error(message: "unexpected input")
    }

    func dodecaExecuteCodeSamples(input: DodecaExecuteSamplesInput) async throws
        -> DodecaCodeExecutionResult
    {
        if sameDodecaExecuteSamplesInput(input, sampleDodecaExecuteSamplesInput()) {
            return sampleDodecaCodeExecutionResult()
        }
        return .error(message: "unexpected input")
    }

    func dodecaLoadData(content: String, format: DodecaDataFormat) async throws
        -> DodecaLoadDataResult
    {
        if content == sampleDodecaDataContent()
            && sameReflecting(format, sampleDodecaDataFormat())
        {
            return sampleDodecaLoadDataResult()
        }
        return .error(message: "unexpected load_data input")
    }

    func dodecaParseAndRender(sourcePath: String, content: String, sourceMap: Bool) async throws
        -> DodecaParseResult
    {
        if sourcePath == sampleDodecaMarkdownSourcePath()
            && content == sampleDodecaMarkdownContent()
            && sourceMap
        {
            return sampleDodecaParseResult()
        }
        return .error(message: "unexpected parse input")
    }

    func echoDodecaImageProcessorFixture(fixture: DodecaImageProcessorFixture) async throws
        -> DodecaImageProcessorFixture
    {
        fixture
    }

    func echoDodecaSearchIndexerFixture(fixture: DodecaSearchIndexerFixture) async throws
        -> DodecaSearchIndexerFixture
    {
        fixture
    }

    func echoDodecaAssetProcessingFixture(fixture: DodecaAssetProcessingFixture) async throws
        -> DodecaAssetProcessingFixture
    {
        fixture
    }


    func echoDodecaSmallCellServicesFixture(fixture: DodecaSmallCellServicesFixture) async throws
        -> DodecaSmallCellServicesFixture
    {
        fixture
    }

    func echoDodecaDevtoolsEvent(event: DodecaDevtoolsEvent) async throws
        -> DodecaDevtoolsEvent
    {
        event
    }

    func dodecaDevtoolsGetScope(path: [String]?) async throws -> [DodecaScopeEntry] {
        if path == ["page"] {
            return sampleDodecaScopeEntries()
        }
        return []
    }

    func dodecaDevtoolsEval(snapshotId: String, expression: String) async throws
        -> DodecaEvalResult
    {
        if snapshotId == "snap-devtools-42" && expression == "page.title" {
            return sampleDodecaEvalResult()
        }
        return .err("unknown expression")
    }

    func dodecaDevtoolsOpenDeadLink(
        route: String,
        target: DodecaDeadLinkTarget
    ) async throws -> DodecaOpenSourceResult {
        if route == "/guide/" && sameReflecting(target, sampleDodecaDeadLinkTarget()) {
            return sampleDodecaOpenSourceResult()
        }
        return .err("unexpected dead link target")
    }

    func dodecaDevtoolsEditLoad(token: String, route: String) async throws -> DodecaEditLoad {
        if token == "editor-token" && route == "/guide/" {
            return sampleDodecaEditLoad()
        }
        return .denied
    }

    func dodecaDevtoolsEditPreview(
        token: String,
        sourceKey: String,
        buffer: String
    ) async throws -> DodecaEditPreview {
        if token == "editor-token"
            && sourceKey == "content/guide.md"
            && buffer == "# Guide\n\nUpdated from browser."
        {
            return sampleDodecaEditPreview()
        }
        return .denied
    }

    func dodecaDevtoolsEditSave(token: String, req: DodecaEditSaveReq) async throws
        -> DodecaEditSave
    {
        if token == "editor-token" && sameReflecting(req, sampleDodecaEditSaveReq()) {
            return sampleDodecaEditSave()
        }
        return .denied
    }

    func dodecaDevtoolsEditUpload(token: String, req: DodecaEditUploadReq) async throws
        -> DodecaEditUpload
    {
        if token == "editor-token" && sameReflecting(req, sampleDodecaEditUploadReq()) {
            return sampleDodecaEditUpload()
        }
        return .denied
    }

    func dodecaDevtoolsEditRead(token: String, uri: String) async throws -> DodecaEditRead {
        if token == "editor-token" && uri == "file:///workspace/content/guide.md" {
            return sampleDodecaEditRead()
        }
        return .denied
    }

    func dodecaDevtoolsEditList(token: String) async throws -> DodecaEditList {
        if token == "editor-token" {
            return sampleDodecaEditList()
        }
        return .denied
    }

    func echoStyxValue(value: StyxValue) async throws -> StyxValue {
        value
    }

    func styxLspInitialize(params: StyxLspInitializeParams) async throws
        -> StyxLspInitializeResult
    {
        guard sameReflecting(params, sampleStyxLspInitializeParams()) else {
            throw SubjectError.invalidResponse
        }
        return sampleStyxLspInitializeResult()
    }

    func styxLspCompletions(params: StyxLspCompletionParams) async throws
        -> [StyxLspCompletionItem]
    {
        guard sameReflecting(params, sampleStyxLspCompletionParams()) else {
            throw SubjectError.invalidResponse
        }
        return sampleStyxLspCompletions()
    }

    func styxLspHover(params: StyxLspHoverParams) async throws -> StyxLspHoverResult? {
        guard sameReflecting(params, sampleStyxLspHoverParams()) else {
            throw SubjectError.invalidResponse
        }
        return sampleStyxLspHoverResult()
    }

    func styxLspInlayHints(params: StyxLspInlayHintParams) async throws
        -> [StyxLspInlayHint]
    {
        guard sameReflecting(params, sampleStyxLspInlayHintParams()) else {
            throw SubjectError.invalidResponse
        }
        return sampleStyxLspInlayHints()
    }

    func styxLspDiagnostics(params: StyxLspDiagnosticParams) async throws
        -> [StyxLspDiagnostic]
    {
        guard sameReflecting(params, sampleStyxLspDiagnosticParams()) else {
            throw SubjectError.invalidResponse
        }
        return sampleStyxLspDiagnostics()
    }

    func styxLspCodeActions(params: StyxLspCodeActionParams) async throws
        -> [StyxLspCodeAction]
    {
        guard sameReflecting(params, sampleStyxLspCodeActionParams()) else {
            throw SubjectError.invalidResponse
        }
        return sampleStyxLspCodeActions()
    }

    func styxLspDefinition(params: StyxLspDefinitionParams) async throws
        -> [StyxLspLocation]
    {
        guard sameReflecting(params, sampleStyxLspDefinitionParams()) else {
            throw SubjectError.invalidResponse
        }
        return sampleStyxLspLocations()
    }

    func styxLspShutdown() async throws {}

    func styxHostGetSubtree(params: StyxLspGetSubtreeParams) async throws -> StyxValue? {
        guard sameReflecting(params, sampleStyxLspGetSubtreeParams()) else {
            throw SubjectError.invalidResponse
        }
        return sampleStyxValue()
    }

    func styxHostGetDocument(params: StyxLspGetDocumentParams) async throws -> StyxValue? {
        guard sameReflecting(params, sampleStyxLspGetDocumentParams()) else {
            throw SubjectError.invalidResponse
        }
        return sampleStyxValue()
    }

    func styxHostGetSource(params: StyxLspGetSourceParams) async throws -> String? {
        guard sameReflecting(params, sampleStyxLspGetSourceParams()) else {
            throw SubjectError.invalidResponse
        }
        return sampleStyxLspSource()
    }

    func styxHostGetSchema(params: StyxLspGetSchemaParams) async throws -> StyxLspSchemaInfo? {
        guard sameReflecting(params, sampleStyxLspGetSchemaParams()) else {
            throw SubjectError.invalidResponse
        }
        return sampleStyxLspSchemaInfo()
    }

    func styxHostOffsetToPosition(params: StyxLspOffsetToPositionParams) async throws
        -> StyxLspPosition?
    {
        guard sameReflecting(params, sampleStyxLspOffsetToPositionParams()) else {
            throw SubjectError.invalidResponse
        }
        return sampleStyxLspPosition()
    }

    func styxHostPositionToOffset(params: StyxLspPositionToOffsetParams) async throws
        -> UInt32?
    {
        guard sameReflecting(params, sampleStyxLspPositionToOffsetParams()) else {
            throw SubjectError.invalidResponse
        }
        return 16
    }

    func staxFlamegraph(params: StaxViewParams) async throws -> StaxFlamegraphUpdate {
        sampleStaxFlamegraphUpdate(params)
    }

    func echoStaxFlamegraphUpdate(update: StaxFlamegraphUpdate) async throws
        -> StaxFlamegraphUpdate
    {
        update
    }

    func staxSubscribeFlamegraphUpdates(output: Tx<StaxFlamegraphUpdate>) async throws {
        for update in sampleStaxFlamegraphUpdates() {
            do {
                try await output.send(update)
            } catch {
                break
            }
        }
        output.close()
    }

    func echoStaxLinuxBrokerControl(fixture: StaxLinuxBrokerControlFixture) async throws
        -> StaxLinuxBrokerControlFixture
    {
        fixture
    }

    func staxMacosRecord(
        config: StaxMacSessionConfig,
        records: Tx<StaxMacKdBufBatch>
    ) async throws -> Result<StaxMacRecordSummary, StaxMacRecordError> {
        guard sameReflecting(config, sampleStaxMacosConfig()) else {
            return .failure(.sysctl(op: "config", message: "unexpected macOS Stax config"))
        }
        for batch in sampleStaxMacosBatches() {
            do {
                try await records.send(batch)
            } catch {
                break
            }
        }
        records.close()
        return .success(sampleStaxMacosRecordSummary())
    }

    func echoHotmealLiveReloadEvent(event: HotmealLiveReloadEvent) async throws
        -> HotmealLiveReloadEvent
    {
        event
    }

    func echoHotmealApplyPatchesResult(result: HotmealApplyPatchesResult) async throws
        -> HotmealApplyPatchesResult
    {
        result
    }

    func hotmealLiveReloadSubscribe(route: String) async throws {
        guard route == sampleHotmealRoute() else {
            throw SubjectError.invalidResponse
        }
    }

    func hotmealLiveReloadOnEvent(event: HotmealLiveReloadEvent) async throws {
        guard sampleHotmealLiveReloadEvents().contains(where: { sameHotmealLiveReloadEvent($0, event) }) else {
            throw SubjectError.invalidResponse
        }
    }

    func echoHelixStreamMetrics(metrics: HelixStreamMetrics) async throws -> HelixStreamMetrics {
        metrics
    }

    func echoHelixVerifyEvidence(digest: HelixVerifyEvidenceDigest) async throws
        -> HelixVerifyEvidenceDigest
    {
        digest
    }

    func helixSubscribePulses(output: Tx<HelixPulseAvailable>) async throws {
        for pulse in sampleHelixPulses() {
            do {
                try await output.send(pulse)
            } catch {
                break
            }
        }
        output.close()
    }

    func helixPulseBundle(pulseId _: UInt64, fields _: HelixPulseBundleFields) async throws
        -> HelixPulseBundle
    {
        sampleHelixPulseBundle()
    }

    func helixTraceServiceSurface() async throws -> HelixTraceServiceSurface {
        sampleHelixTraceServiceSurface()
    }

    func traceyStatus() async throws -> TraceyStatusResponse {
        sampleTraceyStatusResponse()
    }

    func traceyUncovered(req: TraceyUncoveredRequest) async throws -> TraceyUncoveredResponse {
        guard sameTraceyUncoveredRequest(req, sampleTraceyQueryRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyUncoveredResponse()
    }

    func traceyUntested(req: TraceyUntestedRequest) async throws -> TraceyUntestedResponse {
        guard sameTraceyUntestedRequest(req, sampleTraceyUntestedRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyUntestedResponse()
    }

    func traceyStale(req: TraceyStaleRequest) async throws -> TraceyStaleResponse {
        guard sameTraceyStaleRequest(req, sampleTraceyStaleRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyStaleResponse()
    }

    func traceyUnmapped(req: TraceyUnmappedRequest) async throws -> TraceyUnmappedResponse {
        guard sameTraceyUnmappedRequest(req, sampleTraceyUnmappedRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyUnmappedResponse()
    }

    func traceyRule(ruleId: TraceyRuleId) async throws -> TraceyRuleInfo? {
        sameTraceyRuleId(ruleId, traceyRuleId("rpc.channel.direct-args", 1))
            ? sampleTraceyRuleInfo()
            : nil
    }

    func traceyForward(spec: String, implName: String) async throws -> TraceyApiSpecForward? {
        guard implName == "rust" else {
            throw SubjectError.invalidResponse
        }
        return spec == "vox" ? sampleTraceyForwardResponse() : nil
    }

    func traceyReverse(spec: String, implName: String) async throws -> TraceyApiReverseData? {
        guard spec == "vox" && implName == "rust" else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyReverseResponse()
    }

    func traceyFile(req: TraceyFileRequest) async throws -> TraceyApiFileData? {
        guard sameTraceyDashboardValue(req, sampleTraceyFileRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyFileResponse()
    }

    func traceySpecContent(spec: String, implName: String) async throws -> TraceyApiSpecData? {
        guard spec == "vox" && implName == "rust" else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceySpecContentResponse()
    }

    func traceySearch(query: String, limit: UInt32) async throws -> [TraceySearchResult] {
        guard query == "channel" && limit == 10 else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceySearchResults()
    }

    func traceyUpdateFileRange(req: TraceyUpdateFileRangeRequest) async throws -> Result<
        Void, TraceyUpdateError
    > {
        if sameTraceyDashboardValue(req, sampleTraceyUpdateFileRangeRequest()) {
            return .success(())
        }
        if sameTraceyDashboardValue(req, sampleTraceyUpdateFileRangeConflictRequest()) {
            return .failure(sampleTraceyUpdateError())
        }
        throw SubjectError.invalidResponse
    }

    func traceyConfigAddExclude(req: TraceyConfigPatternRequest) async throws -> Result<
        Void, String
    > {
        if sameTraceyDashboardValue(req, sampleTraceyConfigPatternRequest()) {
            return .success(())
        }
        if sameTraceyDashboardValue(req, sampleTraceyBadConfigPatternRequest()) {
            return .failure("invalid pattern")
        }
        throw SubjectError.invalidResponse
    }

    func traceyConfigAddInclude(req: TraceyConfigPatternRequest) async throws -> Result<
        Void, String
    > {
        guard sameTraceyDashboardValue(req, sampleTraceyConfigPatternRequest()) else {
            throw SubjectError.invalidResponse
        }
        return .success(())
    }

    func traceyConfig() async throws -> TraceyApiConfig {
        sampleTraceyApiConfig()
    }

    func traceyVfsOpen(path: String, content: String) async throws {
        guard path == "src/lib.rs" && content == sampleTraceyLspContent() else {
            throw SubjectError.invalidResponse
        }
    }

    func traceyVfsChange(path: String, content: String) async throws {
        guard path == "src/lib.rs" && content == "// r[verify rpc.channel.direct-args]\n" else {
            throw SubjectError.invalidResponse
        }
    }

    func traceyVfsClose(path: String) async throws {
        guard path == "src/lib.rs" else {
            throw SubjectError.invalidResponse
        }
    }

    func traceyReload() async throws -> TraceyReloadResponse {
        sampleTraceyReloadResponse()
    }

    func traceyVersion() async throws -> UInt64 {
        13
    }

    func traceyHealth() async throws -> TraceyHealthResponse {
        sampleTraceyHealthResponse()
    }

    func traceyShutdown() async throws {}

    func traceyValidate(req: TraceyValidateRequest) async throws -> TraceyValidationResult {
        _ = req
        return sampleTraceyValidationResult()
    }

    func traceyIsTestFile(path: String) async throws -> Bool {
        path.hasSuffix("_test.rs") || path.contains("/tests/")
    }

    func traceyLspHover(req: TraceyLspPositionRequest) async throws -> TraceyHoverInfo? {
        guard sameTraceyLspPositionRequest(req, sampleTraceyLspPositionRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyHoverInfo()
    }

    func traceyLspDefinition(req: TraceyLspPositionRequest) async throws -> [TraceyLspLocation] {
        guard sameTraceyLspPositionRequest(req, sampleTraceyLspPositionRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyLspLocations()
    }

    func traceyLspImplementation(req: TraceyLspPositionRequest) async throws
        -> [TraceyLspLocation]
    {
        guard sameTraceyLspPositionRequest(req, sampleTraceyLspPositionRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyLspLocations()
    }

    func traceyLspReferences(req: TraceyLspReferencesRequest) async throws
        -> [TraceyLspLocation]
    {
        guard sameTraceyLspReferencesRequest(req, sampleTraceyLspReferencesRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyLspLocations()
    }

    func traceyLspCompletions(req: TraceyLspPositionRequest) async throws
        -> [TraceyLspCompletionItem]
    {
        guard sameTraceyLspPositionRequest(req, sampleTraceyLspPositionRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyLspCompletions()
    }

    func traceyLspWorkspaceDiagnostics() async throws -> [TraceyLspFileDiagnostics] {
        sampleTraceyLspWorkspaceDiagnostics()
    }

    func traceyLspDocumentSymbols(req: TraceyLspDocumentRequest) async throws
        -> [TraceyLspSymbol]
    {
        guard sameTraceyLspDocumentRequest(req, sampleTraceyLspDocumentRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyLspSymbols()
    }

    func traceyLspWorkspaceSymbols(query: String) async throws -> [TraceyLspSymbol] {
        guard query == "rpc.channel" else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyLspSymbols()
    }

    func traceyLspSemanticTokens(req: TraceyLspDocumentRequest) async throws
        -> [TraceyLspSemanticToken]
    {
        guard sameTraceyLspDocumentRequest(req, sampleTraceyLspDocumentRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyLspSemanticTokens()
    }

    func traceyLspCodeLens(req: TraceyLspDocumentRequest) async throws -> [TraceyLspCodeLens] {
        guard sameTraceyLspDocumentRequest(req, sampleTraceyLspDocumentRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyLspCodeLens()
    }

    func traceyLspInlayHints(req: TraceyLspInlayHintsRequest) async throws
        -> [TraceyLspInlayHint]
    {
        guard sameTraceyLspInlayHintsRequest(req, sampleTraceyLspInlayHintsRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyLspInlayHints()
    }

    func traceyLspPrepareRename(req: TraceyLspPositionRequest) async throws
        -> TraceyPrepareRenameResult?
    {
        guard sameTraceyLspPositionRequest(req, sampleTraceyLspPositionRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyPrepareRenameResult()
    }

    func traceyLspRename(req: TraceyLspRenameRequest) async throws -> [TraceyLspTextEdit] {
        guard sameTraceyLspRenameRequest(req, sampleTraceyLspRenameRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyLspTextEdits()
    }

    func traceyLspCodeActions(req: TraceyLspPositionRequest) async throws
        -> [TraceyLspCodeAction]
    {
        guard sameTraceyLspPositionRequest(req, sampleTraceyLspPositionRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyLspCodeActions()
    }

    func traceyLspDocumentHighlight(req: TraceyLspPositionRequest) async throws
        -> [TraceyLspLocation]
    {
        guard sameTraceyLspPositionRequest(req, sampleTraceyLspPositionRequest()) else {
            throw SubjectError.invalidResponse
        }
        return sampleTraceyLspLocations()
    }

    func traceySubscribeUpdates(updates: Tx<TraceyDataUpdate>) async throws {
        for update in sampleTraceyUpdates() {
            do {
                try await updates.send(update)
            } catch {
                break
            }
        }
        updates.close()
    }
}

// MARK: - Logging

func log(_ message: String) {
    let pid = ProcessInfo.processInfo.processIdentifier
    NSLog("%@", "[\(pid)] \(message)")
}

let defaultSubjectInactivityTimeoutSecs: UInt64 = 60

// r[impl hosted.subject.lifecycle]
func subjectInactivityTimeoutNanoseconds() -> UInt64? {
    let raw = ProcessInfo.processInfo.environment["SUBJECT_INACTIVITY_TIMEOUT_SECS"]
    let secs = raw.flatMap(UInt64.init) ?? defaultSubjectInactivityTimeoutSecs
    if secs == 0 {
        return nil
    }
    return secs * 1_000_000_000
}

// r[impl hosted.subject.lifecycle]
func runWithSubjectTimeout(
    mode: String,
    operation: @Sendable () async throws -> Void
) async throws {
    guard let timeout = subjectInactivityTimeoutNanoseconds() else {
        try await operation()
        return
    }

    let timeoutTask = Task {
        try? await Task.sleep(nanoseconds: timeout)
        if !Task.isCancelled {
            log("subject \(mode) timed out after \(timeout / 1_000_000_000)s without exiting")
            exit(124)
        }
    }
    defer {
        timeoutTask.cancel()
    }

    try await operation()
}

func sameShape(_ lhs: Shape, _ rhs: Shape) -> Bool {
    switch (lhs, rhs) {
    case (.point, .point):
        true
    case (.circle(let lRadius), .circle(let rRadius)):
        lRadius == rRadius
    case (.rectangle(let lWidth, let lHeight), .rectangle(let rWidth, let rHeight)):
        lWidth == rWidth && lHeight == rHeight
    default:
        false
    }
}

/// Structural equality for the recursive `Tree` (generated as `Sendable`, not
/// `Equatable`) — value + same children in order.
func sameTree(_ lhs: Tree, _ rhs: Tree) -> Bool {
    lhs.value == rhs.value && lhs.children.count == rhs.children.count
        && zip(lhs.children, rhs.children).allSatisfy { sameTree($0, $1) }
}

func sampleEcosystemBridgePayload() -> EcosystemBridgePayload {
    EcosystemBridgePayload(
        html: "<main><img src=\"/hero.png\"></main>",
        pathMap: ["/old.css": "/assets/new.css"],
        knownRoutes: Set(["/", "/guide/"]),
        imageVariants: [
            "/hero.png": BridgeResponsiveImageInfo(
                jxlSrcset: [("/hero-640.jxl", 640)],
                webpSrcset: [("/hero-640.webp", 640)]
            )
        ],
        blobs: [Data([0, 1, 2, 3, 255]), Data()]
    )
}

func sameSrcset(_ lhs: [(String, UInt32)], _ rhs: [(String, UInt32)]) -> Bool {
    lhs.count == rhs.count
        && zip(lhs, rhs).allSatisfy { left, right in
            left.0 == right.0 && left.1 == right.1
        }
}

func sameResponsiveImageInfo(
    _ lhs: BridgeResponsiveImageInfo,
    _ rhs: BridgeResponsiveImageInfo
) -> Bool {
    sameSrcset(lhs.jxlSrcset, rhs.jxlSrcset) && sameSrcset(lhs.webpSrcset, rhs.webpSrcset)
}

func sameEcosystemBridgePayload(
    _ lhs: EcosystemBridgePayload,
    _ rhs: EcosystemBridgePayload
) -> Bool {
    guard lhs.html == rhs.html,
        lhs.pathMap == rhs.pathMap,
        lhs.knownRoutes == rhs.knownRoutes,
        lhs.blobs == rhs.blobs,
        lhs.imageVariants.count == rhs.imageVariants.count
    else {
        return false
    }
    return lhs.imageVariants.allSatisfy { key, value in
        guard let other = rhs.imageVariants[key] else {
            return false
        }
        return sameResponsiveImageInfo(value, other)
    }
}

func sameDodecaTemplateCall(_ lhs: DodecaTemplateCall, _ rhs: DodecaTemplateCall) -> Bool {
    lhs.contextId == rhs.contextId
        && lhs.name == rhs.name
        && lhs.args == rhs.args
        && lhs.kwargs.count == rhs.kwargs.count
        && zip(lhs.kwargs, rhs.kwargs).allSatisfy { left, right in
            left.0 == right.0 && left.1 == right.1
        }
}

func sampleDodecaTemplateCall() -> DodecaTemplateCall {
    let context: Value = .object([
        .init(key: "sidebar", value: .bool(true)),
        .init(key: "title", value: .string("Phon migration")),
        .init(key: "count", value: .number(.i64(42))),
    ])
    return DodecaTemplateCall(
        contextId: "ctx-docs",
        name: "render-card",
        args: [context, .string("docs")],
        kwargs: [("path", .string("/guide/"))]
    )
}

func sampleDodecaDataContent() -> String {
    "{\"title\":\"Phon\",\"sidebar\":true,\"count\":42}"
}

func sampleDodecaDataFormat() -> DodecaDataFormat {
    .json
}

func sampleDodecaLoadDataResult() -> DodecaLoadDataResult {
    .success(
        value: .object([
            .init(key: "title", value: .string("Phon")),
            .init(key: "sidebar", value: .bool(true)),
            .init(key: "count", value: .number(.i64(42))),
        ])
    )
}

func sampleDodecaMarkdownSourcePath() -> String {
    "content/guide.md"
}

func sampleDodecaMarkdownContent() -> String {
    "+++\ntitle = \"Phon migration\"\n+++\n\n# Intro\n\nr[vox.dodeca.markdown]\n"
}

func sampleDodecaParseResult() -> DodecaParseResult {
    .success(
        frontmatter: DodecaFrontmatter(
            title: "Phon migration",
            weight: 10,
            description: "Generated fixture for Dodeca markdown",
            template: "page.html",
            extra: .object([
                .init(key: "sidebar", value: .bool(true)),
                .init(key: "icon", value: .string("book")),
                .init(key: "custom_value", value: .number(.i64(42))),
            ])
        ),
        html: "<h1 data-sid=\"h1\">Intro</h1><p data-sid=\"p1\">Generated fixture</p>",
        headings: [DodecaMarkdownHeading(title: "Intro", id: "intro", level: 1)],
        reqs: [DodecaReqDefinition(id: "vox.dodeca.markdown", anchorId: "r-vox-dodeca-markdown")],
        headInjections: ["<link rel=\"stylesheet\" href=\"/assets/arborium.css\">"],
        sourceMap: DodecaSourceMap(
            sourcePath: sampleDodecaMarkdownSourcePath(),
            entries: [
                DodecaSourceMapEntry(
                    id: "h1",
                    kind: .heading,
                    lineStart: 5,
                    lineEnd: 5,
                    byteStart: 38,
                    byteEnd: 45
                ),
                DodecaSourceMapEntry(
                    id: "p1",
                    kind: .paragraph,
                    lineStart: 7,
                    lineEnd: 7,
                    byteStart: 47,
                    byteEnd: 71
                ),
            ]
        )
    )
}

func byteRamp(_ length: Int, seed: UInt8) -> Data {
    Data((0..<length).map { UInt8((Int(seed) + $0) & 0xff) })
}

func sampleDodecaDecodedImage(seed: UInt8, width: UInt32, height: UInt32) -> DodecaDecodedImage {
    DodecaDecodedImage(
        pixels: byteRamp(Int(width) * Int(height) * 4, seed: seed),
        width: width,
        height: height,
        channels: 4
    )
}

func sampleDodecaImageProcessorFixture() -> DodecaImageProcessorFixture {
    let decoded = sampleDodecaDecodedImage(seed: 0x20, width: 96, height: 64)
    let resized = sampleDodecaDecodedImage(seed: 0x80, width: 48, height: 32)
    return DodecaImageProcessorFixture(
        pngData: byteRamp(16_384, seed: 0),
        decodedResult: .success(image: decoded),
        resizeInput: DodecaResizeInput(
            pixels: decoded.pixels,
            width: decoded.width,
            height: decoded.height,
            channels: decoded.channels,
            targetWidth: resized.width
        ),
        resizeResult: .success(image: resized),
        thumbhashInput: DodecaThumbhashInput(
            pixels: decoded.pixels,
            width: decoded.width,
            height: decoded.height
        ),
        thumbhashResult: .thumbhashSuccess(
            dataUrl: "data:image/thumbhash;base64,BwgJCgsMDQ4PEA=="
        ),
        errorResult: .error(message: "unsupported color profile in source image")
    )
}

func sampleDodecaSearchIndexerFixture() -> DodecaSearchIndexerFixture {
    let pages = (0..<32).map { i in
        DodecaSearchPage(
            url: "/guide/topic-\(i)/",
            source: "content/guide/topic-\(i).md",
            html: "<article><h1>Topic \(i)</h1><p>Search body \(i)</p></article>"
        )
    }
    let files = (0..<8).map { i in
        DodecaSearchFile(
            path: "public/search/chunk-\(i).json",
            contents: byteRamp(1_024, seed: UInt8(i * 17))
        )
    }
    return DodecaSearchIndexerFixture(
        pages: pages,
        result: .success(files: files),
        errorResult: .error(message: "search index could not write public/search/index.json")
    )
}

func sampleDodecaAssetProcessingFixture() -> DodecaAssetProcessingFixture {
    DodecaAssetProcessingFixture(
        cssSource: "body { background: url('/old/bg.png'); color: red; }",
        cssPathMap: [
            "/old/bg.png": "/assets/bg.abcd.png",
            "/old/font.woff2": "/assets/font.woff2",
        ],
        cssResult: .success(css: "body{background:url('/assets/bg.abcd.png');color:red}"),
        sassEntrypoint: "styles/app.scss",
        sassFiles: [
            "styles/app.scss": "$brand: #c0ffee; @import 'partials/buttons'; body { color: $brand; }",
            "styles/partials/_buttons.scss": ".button { padding: 4px; }",
        ],
        sassLoadPaths: ["styles", "vendor"],
        sassResult: .success(css: "body{color:#c0ffee}.button{padding:4px}"),
        svgSource: "<svg viewBox=\"0 0 10 10\"><rect width=\"10\" height=\"10\" fill=\"red\"/></svg>",
        svgoResult: .success(
            svg: "<svg viewBox=\"0 0 10 10\"><path fill=\"red\" d=\"M0 0h10v10H0z\"/></svg>"
        )
    )
}


func sampleDodecaTaskProgress(
    _ name: String,
    total: UInt32,
    completed: UInt32,
    status: DodecaTaskStatus
) -> DodecaTaskProgress {
    let message: String?
    switch status {
    case .error:
        message = "\(name) failed"
    default:
        message = nil
    }

    return DodecaTaskProgress(
        name: name,
        total: total,
        completed: completed,
        status: status,
        message: message
    )
}

func sampleDodecaSmallCellServicesFixture() -> DodecaSmallCellServicesFixture {
    DodecaSmallCellServicesFixture(
        readyMsg: DodecaReadyMsg(
            peerId: 42,
            cellName: "ddc-cell-fonts",
            pid: 12_345,
            version: "1.0.0-dev",
            features: ["woff2", "subset"]
        ),
        readyAck: DodecaReadyAck(ok: true, hostTimeUnixMs: 1_778_000_000_000),
        minifyResult: .success(content: "<main><h1>Hi</h1></main>"),
        jsInput: DodecaJsRewriteInput(
            js: "import '/assets/theme.css'; console.log('/assets/app.js')",
            pathMap: [
                "/assets/app.js": "/assets/app.1234.js",
                "/assets/theme.css": "/assets/theme.abcd.css",
            ]
        ),
        jsResult: .success("import '/assets/theme.abcd.css'; console.log('/assets/app.1234.js')"),
        htmlDiffInput: DodecaHtmlDiffInput(
            oldHtml: "<main><h1>Old</h1></main>",
            newHtml: "<main><h1>New</h1><p>body</p></main>"
        ),
        htmlDiffResult: .success(
            DodecaHtmlDiffOutcome(patchesBlob: Data([0x91, 0xa4, 0x70, 0x61, 0x74, 0x68]))
        ),
        subsetFontInput: DodecaSubsetFontInput(
            data: Data([0x77, 0x4f, 0x46, 0x32]),
            chars: [UnicodeScalar(0x41)!, UnicodeScalar(0x00e9)!, UnicodeScalar(0x1f41d)!]
        ),
        fontResults: [
            .decompressSuccess(data: Data([0x00, 0x01, 0x00, 0x00])),
            .subsetSuccess(data: Data([0xde, 0xad, 0xbe, 0xef])),
            .compressSuccess(data: Data([0x77, 0x4f, 0x46, 0x32, 0x01])),
        ],
        webpEncodeInput: DodecaWebpEncodeInput(
            pixels: Data([0, 32, 64, 255, 255, 128, 0, 255]),
            width: 2,
            height: 1,
            quality: 82
        ),
        webpResults: [
            .decodeSuccess(pixels: Data([0, 32, 64, 255]), width: 1, height: 1, channels: 4),
            .encodeSuccess(data: Data([0x52, 0x49, 0x46, 0x46])),
        ],
        jxlEncodeInput: DodecaJxlEncodeInput(
            pixels: Data([0, 0, 0, 255, 255, 255, 255, 255]),
            width: 2,
            height: 1,
            quality: 90
        ),
        jxlResults: [
            .decodeSuccess(pixels: Data([255, 0, 255, 255]), width: 1, height: 1, channels: 4),
            .error(message: "unsupported color profile"),
        ],
        selectResult: .selected(index: 2),
        confirmResult: .yes,
        recordConfig: DodecaRecordConfig(shell: "/bin/zsh"),
        termResult: .success(html: "<t-b>cargo nextest</t-b>"),
        startDevServerResult: .success(port: 5173),
        runBuildResult: .error(message: "vite config missing"),
        linkCheckInput: DodecaLinkCheckInput(
            urls: ["https://example.com/ok", "https://example.com/missing"],
            delayMs: 250,
            timeoutSecs: 15
        ),
        linkCheckResult: .success(
            output: DodecaLinkCheckOutput(results: [
                "https://example.com/ok": .ok,
                "https://example.com/missing": .httpError(
                    code: 404,
                    diagnostics: DodecaLinkDiagnostics(
                        requestHeaders: [("accept", "text/html")],
                        responseHeaders: [("content-type", "text/html")],
                        responseBody: "<h1>not found</h1>"
                    )
                ),
                "https://slow.example.com": .skipped,
            ])
        ),
        buildProgress: DodecaBuildProgress(
            parse: sampleDodecaTaskProgress("parse", total: 12, completed: 12, status: .done),
            render: sampleDodecaTaskProgress("render", total: 48, completed: 40, status: .running),
            sass: sampleDodecaTaskProgress("sass", total: 3, completed: 3, status: .done),
            links: sampleDodecaTaskProgress("links", total: 10, completed: 7, status: .running),
            search: sampleDodecaTaskProgress("search", total: 1, completed: 0, status: .pending)
        ),
        logEvent: DodecaLogEvent(
            level: .warn,
            kind: .http(status: 404),
            message: "dead link",
            fields: [("route", "/guide/"), ("href", "/missing/")]
        ),
        serverStatus: DodecaServerStatus(
            urls: ["http://127.0.0.1:5173", "http://192.168.1.42:5173"],
            isRunning: true,
            bindMode: .lan,
            picanteCacheSize: 4_096,
            casCacheSize: 8_192,
            codeExecCacheSize: 1_024
        ),
        serverCommand: .setLogFilter(filter: "dodeca=debug,cell=trace"),
        commandResult: .ok
    )
}

func sampleDodecaSourceLines() -> [DodecaSourceLine] {
    [
        DodecaSourceLine(number: 12, content: "{% for item in data.items %}"),
        DodecaSourceLine(number: 13, content: "{{ item.title }}"),
    ]
}

func sampleDodecaSourceSnippet() -> DodecaSourceSnippet {
    DodecaSourceSnippet(lines: sampleDodecaSourceLines(), errorLine: 13)
}

func sampleDodecaErrorInfo() -> DodecaErrorInfo {
    DodecaErrorInfo(
        route: "/guide/",
        message: "unknown filter `slugify`",
        template: "templates/page.html",
        line: 13,
        column: 8,
        sourceSnippet: sampleDodecaSourceSnippet(),
        snapshotId: "snap-devtools-42",
        availableVariables: ["page", "root", "data"]
    )
}

func sampleDodecaDevtoolsEvent() -> DodecaDevtoolsEvent {
    .error(sampleDodecaErrorInfo())
}

func sampleDodecaScopeEntries() -> [DodecaScopeEntry] {
    [
        DodecaScopeEntry(name: "title", value: .string("Phon migration"), expandable: false),
        DodecaScopeEntry(
            name: "items",
            value: .array(length: 3, preview: "[intro, install, api]"),
            expandable: true
        ),
        DodecaScopeEntry(
            name: "metrics",
            value: .object(fields: 2, preview: "{views, updated_at}"),
            expandable: true
        ),
        DodecaScopeEntry(name: "score", value: .number(42.5), expandable: false),
    ]
}

func sampleDodecaEvalResult() -> DodecaEvalResult {
    .ok(.object(fields: 2, preview: "{title, route}"))
}

func sampleDodecaDeadLinkTarget() -> DodecaDeadLinkTarget {
    .wiki(key: "missing-page", title: "Missing Page")
}

func sampleDodecaOpenSourceResult() -> DodecaOpenSourceResult {
    .ok
}

func sampleDodecaSidLines() -> [DodecaSidLine] {
    [
        DodecaSidLine(sid: "p-1", line: 5),
        DodecaSidLine(sid: "code-1", line: 17),
    ]
}

func sampleDodecaEditLoad() -> DodecaEditLoad {
    .ok(
        sourceKey: "content/guide.md",
        route: "/guide/",
        uri: "file:///workspace/content/guide.md",
        content: "# Guide\n\nWelcome to Phon.",
        base: "a1b2c3d4"
    )
}

func sampleDodecaEditPreview() -> DodecaEditPreview {
    .ok(
        html: "<article><h1>Guide</h1><p>Welcome to Phon.</p></article>",
        sourceMap: sampleDodecaSidLines()
    )
}

func sampleDodecaEditSaveReq() -> DodecaEditSaveReq {
    DodecaEditSaveReq(
        sourceKey: "content/guide.md",
        buffer: "# Guide\n\nUpdated from browser.",
        base: "a1b2c3d4",
        message: "Update guide"
    )
}

func sampleDodecaEditSave() -> DodecaEditSave {
    .ok(commit: "deadbeef1234", base: "b4c3d2a1")
}

func sampleDodecaEditUploadReq() -> DodecaEditUploadReq {
    DodecaEditUploadReq(
        sourceKey: "content/guide.md",
        filename: "diagram.png",
        bytes: byteRamp(128, seed: 31)
    )
}

func sampleDodecaEditUpload() -> DodecaEditUpload {
    .ok(markdown: "![diagram](./diagram.png)", path: "diagram.png")
}

func sampleDodecaEditRead() -> DodecaEditRead {
    .ok(content: "# Guide\n\nWelcome to Phon.", base: "a1b2c3d4")
}

func sampleDodecaEditList() -> DodecaEditList {
    .ok(entries: [
        DodecaEditEntry(
            sourceKey: "content/guide.md",
            route: "/guide/",
            uri: "file:///workspace/content/guide.md",
            title: "Guide"
        ),
        DodecaEditEntry(
            sourceKey: "content/reference.md",
            route: "/reference/",
            uri: "file:///workspace/content/reference.md",
            title: "Reference"
        ),
    ])
}

func sampleDodecaResolvedDependency() -> DodecaResolvedDependency {
    DodecaResolvedDependency(
        name: "facet",
        version: "0.46.0",
        source: .git(url: "https://github.com/facet-rs/facet", commit: "abc1234")
    )
}

func sampleDodecaCodeMetadata() -> DodecaCodeExecutionMetadata {
    DodecaCodeExecutionMetadata(
        rustcVersion: "rustc 1.89.0",
        cargoVersion: "cargo 1.89.0",
        target: "aarch64-apple-darwin",
        timestamp: "2026-06-05T00:00:00Z",
        cacheHit: true,
        platform: "macos",
        arch: "aarch64",
        dependencies: [sampleDodecaResolvedDependency()]
    )
}

func sampleDodecaResponsiveImageInfo() -> DodecaResponsiveImageInfo {
    DodecaResponsiveImageInfo(
        jxlSrcset: [
            ("/assets/hero-640.jxl", 640),
            ("/assets/hero-1280.jxl", 1280),
        ],
        webpSrcset: [("/assets/hero-640.webp", 640)],
        originalWidth: 1920,
        originalHeight: 1080,
        thumbhashDataUrl: "data:image/png;base64,dGh1bWI="
    )
}

func sampleDodecaHtmlProcessInput() -> DodecaHtmlProcessInput {
    DodecaHtmlProcessInput(
        html: "<main><a href=\"/missing\">missing</a><img src=\"/hero.png\"></main>",
        pathMap: ["/old/hero.png": "/assets/hero.png"],
        knownRoutes: Set(["/", "/guide/"]),
        codeMetadata: ["sample-1": sampleDodecaCodeMetadata()],
        injections: [
            .headStyle(css: "body { color: oklch(0.2 0.03 240); }"),
            .headScript(js: "console.log('dodeca')", module: true),
            .bodyScript(js: "window.__dodeca = true", module: false),
        ],
        minify: DodecaMinifyOptions(
            minifyInlineCss: true,
            minifyInlineJs: true,
            minifyHtml: false
        ),
        sourceToRoute: ["content/guide.md": "/guide/"],
        wikiToRoute: ["getting-started": "/guide/"],
        baseRoute: "/guide/intro/",
        imageVariants: ["/hero.png": sampleDodecaResponsiveImageInfo()],
        viteCssMap: ["/src/main.ts": ["/assets/main.css", "/assets/theme.css"]],
        mount: DodecaMountLocalization(
            segment: "wiki",
            routes: Set(["/exec/", "/guide/"])
        )
    )
}

func sampleDodecaHtmlProcessResult() -> DodecaHtmlProcessResult {
    .success(
        html: "<main data-processed=\"true\"><a data-dead href=\"/missing\">missing</a></main>",
        hadDeadLinks: true,
        hadCodeButtons: true,
        hrefs: ["/missing", "/guide/"],
        elementIds: ["intro", "sample-1"],
        unresolvedWikiLinks: [DodecaWikiLinkRef(key: "unknown", target: "Missing Page")]
    )
}

func sampleDodecaDependencySpec() -> DodecaDependencySpec {
    DodecaDependencySpec(
        name: "facet",
        version: "0.46",
        git: "https://github.com/facet-rs/facet",
        rev: nil,
        branch: "main",
        path: nil,
        features: ["derive"]
    )
}

func sampleDodecaRustConfig() -> DodecaRustConfig {
    DodecaRustConfig(
        command: "cargo",
        args: ["run", "--quiet"],
        extension: "rs",
        prepareCode: true,
        autoImports: ["use std::collections::HashMap;", "use facet::Facet;"],
        showOutput: true
    )
}

func sampleDodecaCodeExecutionConfig() -> DodecaCodeExecutionConfig {
    DodecaCodeExecutionConfig(
        enabled: true,
        failOnError: true,
        timeoutSecs: 30,
        cacheDir: ".cache/code-execution",
        projectRoot: "/workspace/docs",
        dependencies: [sampleDodecaDependencySpec()],
        rust: sampleDodecaRustConfig()
    )
}

func sampleDodecaCodeSample() -> DodecaCodeSample {
    DodecaCodeSample(
        sourcePath: "content/guide.md",
        line: 42,
        language: "rust",
        code: "#[derive(Facet)]\nstruct Card { title: String }",
        executable: true,
        expectedErrors: []
    )
}

func sampleDodecaBuildMetadata() -> DodecaBuildMetadata {
    DodecaBuildMetadata(
        rustcVersion: "rustc 1.89.0",
        cargoVersion: "cargo 1.89.0",
        target: "aarch64-apple-darwin",
        timestamp: "2026-06-05T00:00:00Z",
        cacheHit: false,
        platform: "macos",
        arch: "aarch64",
        dependencies: [sampleDodecaResolvedDependency()]
    )
}

func sampleDodecaExecuteSamplesInput() -> DodecaExecuteSamplesInput {
    DodecaExecuteSamplesInput(
        samples: [sampleDodecaCodeSample()],
        config: sampleDodecaCodeExecutionConfig()
    )
}

func sampleDodecaCodeExecutionResult() -> DodecaCodeExecutionResult {
    .executeSuccess(
        output: DodecaExecuteSamplesOutput(results: [
            (
                sampleDodecaCodeSample(),
                DodecaExecutionResult(
                    status: .success,
                    exitCode: 0,
                    stdout: "Card { title: \"Phon\" }",
                    stderr: "",
                    durationMs: 128,
                    error: nil,
                    metadata: sampleDodecaBuildMetadata()
                )
            )
        ])
    )
}

func sameReflecting<T>(_ lhs: T, _ rhs: T) -> Bool {
    String(reflecting: lhs) == String(reflecting: rhs)
}

func sameOptionalReflecting<T>(_ lhs: T?, _ rhs: T?) -> Bool {
    switch (lhs, rhs) {
    case (.none, .none):
        return true
    case (.some(let left), .some(let right)):
        return sameReflecting(left, right)
    default:
        return false
    }
}

func sameReflectingMap<T>(_ lhs: [String: T]?, _ rhs: [String: T]?) -> Bool {
    switch (lhs, rhs) {
    case (.none, .none):
        return true
    case (.some(let left), .some(let right)):
        guard left.keys.sorted() == right.keys.sorted() else {
            return false
        }
        return left.keys.allSatisfy { key in
            guard let leftValue = left[key], let rightValue = right[key] else {
                return false
            }
            return sameReflecting(leftValue, rightValue)
        }
    default:
        return false
    }
}

func sameDodecaAssetProcessingFixture(
    _ lhs: DodecaAssetProcessingFixture,
    _ rhs: DodecaAssetProcessingFixture
) -> Bool {
    lhs.cssSource == rhs.cssSource
        && lhs.cssPathMap == rhs.cssPathMap
        && sameReflecting(lhs.cssResult, rhs.cssResult)
        && lhs.sassEntrypoint == rhs.sassEntrypoint
        && lhs.sassFiles == rhs.sassFiles
        && lhs.sassLoadPaths == rhs.sassLoadPaths
        && sameReflecting(lhs.sassResult, rhs.sassResult)
        && lhs.svgSource == rhs.svgSource
        && sameReflecting(lhs.svgoResult, rhs.svgoResult)
}

func sameDodecaSmallCellServicesFixture(
    _ lhs: DodecaSmallCellServicesFixture,
    _ rhs: DodecaSmallCellServicesFixture
) -> Bool {
    encodeVoxTyped(lhs, testbed_echoDodecaSmallCellServicesFixture_ArgsEncoder)
        == encodeVoxTyped(rhs, testbed_echoDodecaSmallCellServicesFixture_ArgsEncoder)
}

func sameDodecaMountLocalization(
    _ lhs: DodecaMountLocalization?,
    _ rhs: DodecaMountLocalization?
) -> Bool {
    switch (lhs, rhs) {
    case (.none, .none):
        return true
    case (.some(let left), .some(let right)):
        return left.segment == right.segment && left.routes == right.routes
    default:
        return false
    }
}

func sameDodecaHtmlProcessInput(
    _ lhs: DodecaHtmlProcessInput,
    _ rhs: DodecaHtmlProcessInput
) -> Bool {
    lhs.html == rhs.html
        && lhs.pathMap == rhs.pathMap
        && lhs.knownRoutes == rhs.knownRoutes
        && sameReflectingMap(lhs.codeMetadata, rhs.codeMetadata)
        && sameReflecting(lhs.injections, rhs.injections)
        && sameOptionalReflecting(lhs.minify, rhs.minify)
        && lhs.sourceToRoute == rhs.sourceToRoute
        && lhs.wikiToRoute == rhs.wikiToRoute
        && lhs.baseRoute == rhs.baseRoute
        && sameReflectingMap(lhs.imageVariants, rhs.imageVariants)
        && sameReflectingMap(lhs.viteCssMap, rhs.viteCssMap)
        && sameDodecaMountLocalization(lhs.mount, rhs.mount)
}

func sameDodecaExecuteSamplesInput(
    _ lhs: DodecaExecuteSamplesInput,
    _ rhs: DodecaExecuteSamplesInput
) -> Bool {
    sameReflecting(lhs, rhs)
}

func sameDodecaHtmlProcessResult(
    _ lhs: DodecaHtmlProcessResult,
    _ rhs: DodecaHtmlProcessResult
) -> Bool {
    sameReflecting(lhs, rhs)
}

func sameDodecaCodeExecutionResult(
    _ lhs: DodecaCodeExecutionResult,
    _ rhs: DodecaCodeExecutionResult
) -> Bool {
    sameReflecting(lhs, rhs)
}

func sameDodecaLoadDataResult(
    _ lhs: DodecaLoadDataResult,
    _ rhs: DodecaLoadDataResult
) -> Bool {
    sameReflecting(lhs, rhs)
}

func sameDodecaParseResult(
    _ lhs: DodecaParseResult,
    _ rhs: DodecaParseResult
) -> Bool {
    sameReflecting(lhs, rhs)
}

func styxSpan(_ start: UInt32, _ end: UInt32) -> StyxSpan? {
    StyxSpan(start: start, end: end)
}

func styxScalar(_ text: String, _ kind: StyxScalarKind, _ start: UInt32, _ end: UInt32)
    -> StyxValue
{
    StyxValue(
        tag: nil,
        payload: .scalar(StyxScalar(text: text, kind: kind, span: styxSpan(start, end))),
        span: styxSpan(start, end)
    )
}

func sampleStyxValue() -> StyxValue {
    StyxValue(
        tag: StyxTag(name: "schema", span: styxSpan(0, 7)),
        payload: .object(StyxObject(
            entries: [
                StyxEntry(
                    key: styxScalar("title", .bare, 9, 14),
                    value: styxScalar("Phon migration", .quoted, 15, 31),
                    docComment: "page title"
                ),
                StyxEntry(
                    key: styxScalar("features", .bare, 33, 41),
                    value: StyxValue(
                        tag: StyxTag(name: "seq", span: styxSpan(42, 46)),
                        payload: .sequence(StyxSequence(
                            items: [
                                styxScalar("jit", .bare, 47, 50),
                                StyxValue(
                                    tag: StyxTag(name: "object", span: styxSpan(51, 58)),
                                    payload: .object(StyxObject(
                                        entries: [
                                            StyxEntry(
                                                key: styxScalar("lang", .bare, 59, 63),
                                                value: styxScalar("rust", .raw, 64, 70),
                                                docComment: nil
                                            )
                                        ],
                                        span: styxSpan(58, 71)
                                    )),
                                    span: styxSpan(51, 71)
                                ),
                            ],
                            span: styxSpan(46, 72)
                        )),
                        span: styxSpan(42, 72)
                    ),
                    docComment: nil
                ),
            ],
            span: styxSpan(8, 73)
        )),
        span: styxSpan(0, 73)
    )
}

func sampleStyxLspUri() -> String {
    "file:///workspace/queries.styx"
}

func sampleStyxLspSource() -> String {
    "@query { from products select (id name) }"
}

func sampleStyxLspPosition() -> StyxLspPosition {
    StyxLspPosition(line: 0, character: 16)
}

func sampleStyxLspCursor() -> StyxLspCursor {
    StyxLspCursor(line: 0, character: 16, offset: 16)
}

func sampleStyxLspRange() -> StyxLspRange {
    StyxLspRange(
        start: StyxLspPosition(line: 0, character: 0),
        end: StyxLspPosition(line: 0, character: 38)
    )
}

func sampleStyxLspInitializeParams() -> StyxLspInitializeParams {
    StyxLspInitializeParams(
        styxVersion: "4.0",
        documentUri: sampleStyxLspUri(),
        schemaId: "crate:dibs-queries@1"
    )
}

func sampleStyxLspInitializeResult() -> StyxLspInitializeResult {
    StyxLspInitializeResult(
        name: "dibs-styx-extension",
        version: "0.1.0",
        capabilities: [
            .completions,
            .hover,
            .diagnostics,
            .codeActions,
            .definition,
        ]
    )
}

func sampleStyxLspCompletionParams() -> StyxLspCompletionParams {
    StyxLspCompletionParams(
        documentUri: sampleStyxLspUri(),
        cursor: sampleStyxLspCursor(),
        path: ["AllProducts", "@query", "select"],
        prefix: "na",
        context: sampleStyxValue(),
        taggedContext: sampleStyxValue()
    )
}

func sampleStyxLspCompletions() -> [StyxLspCompletionItem] {
    [
        StyxLspCompletionItem(
            label: "name",
            detail: "TEXT",
            documentation: "Product display name",
            kind: .field,
            sortText: "0001",
            insertText: nil
        ),
        StyxLspCompletionItem(
            label: "metadata",
            detail: "JSONB",
            documentation: nil,
            kind: .field,
            sortText: "0002",
            insertText: "metadata"
        ),
    ]
}

func sampleStyxLspHoverParams() -> StyxLspHoverParams {
    StyxLspHoverParams(
        documentUri: sampleStyxLspUri(),
        cursor: sampleStyxLspCursor(),
        path: ["AllProducts", "@query", "from"],
        context: sampleStyxValue(),
        taggedContext: sampleStyxValue()
    )
}

func sampleStyxLspHoverResult() -> StyxLspHoverResult {
    StyxLspHoverResult(
        contents: "**products** table\n\nBacked by `Product`.",
        range: StyxLspRange(
            start: StyxLspPosition(line: 0, character: 14),
            end: StyxLspPosition(line: 0, character: 22)
        )
    )
}

func sampleStyxLspInlayHintParams() -> StyxLspInlayHintParams {
    StyxLspInlayHintParams(
        documentUri: sampleStyxLspUri(),
        range: sampleStyxLspRange(),
        context: sampleStyxValue()
    )
}

func sampleStyxLspInlayHints() -> [StyxLspInlayHint] {
    [
        StyxLspInlayHint(
            position: StyxLspPosition(line: 0, character: 9),
            label: "Product",
            kind: .type,
            paddingLeft: true,
            paddingRight: false
        )
    ]
}

func sampleStyxLspDiagnostic() -> StyxLspDiagnostic {
    StyxLspDiagnostic(
        span: StyxSpan(start: 23, end: 29),
        severity: .warning,
        message: "column `legacy` is deprecated",
        source: "dibs",
        code: "deprecated-column",
        data: sampleStyxValue()
    )
}

func sampleStyxLspDiagnosticParams() -> StyxLspDiagnosticParams {
    StyxLspDiagnosticParams(
        documentUri: sampleStyxLspUri(),
        tree: sampleStyxValue(),
        content: sampleStyxLspSource()
    )
}

func sampleStyxLspDiagnostics() -> [StyxLspDiagnostic] {
    [sampleStyxLspDiagnostic()]
}

func sampleStyxLspCodeActionParams() -> StyxLspCodeActionParams {
    StyxLspCodeActionParams(
        documentUri: sampleStyxLspUri(),
        span: StyxSpan(start: 23, end: 29),
        diagnostics: sampleStyxLspDiagnostics()
    )
}

func sampleStyxLspCodeActions() -> [StyxLspCodeAction] {
    [
        StyxLspCodeAction(
            title: "Replace legacy column",
            kind: .quickFix,
            edit: StyxLspWorkspaceEdit(
                changes: [
                    StyxLspDocumentEdit(
                        uri: sampleStyxLspUri(),
                        edits: [
                            StyxLspTextEdit(
                                span: StyxSpan(start: 23, end: 29),
                                newText: "name"
                            )
                        ]
                    )
                ]
            ),
            isPreferred: true
        )
    ]
}

func sampleStyxLspDefinitionParams() -> StyxLspDefinitionParams {
    StyxLspDefinitionParams(
        documentUri: sampleStyxLspUri(),
        cursor: sampleStyxLspCursor(),
        path: ["AllProducts", "@query", "from"],
        context: sampleStyxValue(),
        taggedContext: sampleStyxValue()
    )
}

func sampleStyxLspLocations() -> [StyxLspLocation] {
    [
        StyxLspLocation(
            uri: "file:///workspace/schema.styx",
            span: StyxSpan(start: 120, end: 128)
        )
    ]
}

func sampleStyxLspGetSubtreeParams() -> StyxLspGetSubtreeParams {
    StyxLspGetSubtreeParams(
        documentUri: sampleStyxLspUri(),
        path: ["AllProducts", "@query"]
    )
}

func sampleStyxLspGetDocumentParams() -> StyxLspGetDocumentParams {
    StyxLspGetDocumentParams(documentUri: sampleStyxLspUri())
}

func sampleStyxLspGetSourceParams() -> StyxLspGetSourceParams {
    StyxLspGetSourceParams(documentUri: sampleStyxLspUri())
}

func sampleStyxLspGetSchemaParams() -> StyxLspGetSchemaParams {
    StyxLspGetSchemaParams(documentUri: sampleStyxLspUri())
}

func sampleStyxLspSchemaInfo() -> StyxLspSchemaInfo {
    StyxLspSchemaInfo(
        source: "@schema { @ @object{ name @string } }",
        uri: "styx-embedded://crate:dibs-queries@1"
    )
}

func sampleStyxLspOffsetToPositionParams() -> StyxLspOffsetToPositionParams {
    StyxLspOffsetToPositionParams(
        documentUri: sampleStyxLspUri(),
        offset: 16
    )
}

func sampleStyxLspPositionToOffsetParams() -> StyxLspPositionToOffsetParams {
    StyxLspPositionToOffsetParams(
        documentUri: sampleStyxLspUri(),
        position: sampleStyxLspPosition()
    )
}

func sameStyxSpan(_ lhs: StyxSpan?, _ rhs: StyxSpan?) -> Bool {
    switch (lhs, rhs) {
    case (nil, nil):
        true
    case (.some(let l), .some(let r)):
        l.start == r.start && l.end == r.end
    default:
        false
    }
}

func sameStyxTag(_ lhs: StyxTag?, _ rhs: StyxTag?) -> Bool {
    switch (lhs, rhs) {
    case (nil, nil):
        true
    case (.some(let l), .some(let r)):
        l.name == r.name && sameStyxSpan(l.span, r.span)
    default:
        false
    }
}

func sameStyxScalarKind(_ lhs: StyxScalarKind, _ rhs: StyxScalarKind) -> Bool {
    switch (lhs, rhs) {
    case (.bare, .bare), (.quoted, .quoted), (.raw, .raw), (.heredoc, .heredoc):
        true
    default:
        false
    }
}

func sameStyxScalar(_ lhs: StyxScalar, _ rhs: StyxScalar) -> Bool {
    lhs.text == rhs.text && sameStyxScalarKind(lhs.kind, rhs.kind)
        && sameStyxSpan(lhs.span, rhs.span)
}

func sameStyxSequence(_ lhs: StyxSequence, _ rhs: StyxSequence) -> Bool {
    lhs.items.count == rhs.items.count
        && zip(lhs.items, rhs.items).allSatisfy { sameStyxValue($0, $1) }
        && sameStyxSpan(lhs.span, rhs.span)
}

func sameStyxEntry(_ lhs: StyxEntry, _ rhs: StyxEntry) -> Bool {
    sameStyxValue(lhs.key, rhs.key) && sameStyxValue(lhs.value, rhs.value)
        && lhs.docComment == rhs.docComment
}

func sameStyxObject(_ lhs: StyxObject, _ rhs: StyxObject) -> Bool {
    lhs.entries.count == rhs.entries.count
        && zip(lhs.entries, rhs.entries).allSatisfy { sameStyxEntry($0, $1) }
        && sameStyxSpan(lhs.span, rhs.span)
}

func sameStyxPayload(_ lhs: StyxPayload?, _ rhs: StyxPayload?) -> Bool {
    switch (lhs, rhs) {
    case (nil, nil):
        true
    case (.some(.scalar(let l)), .some(.scalar(let r))):
        sameStyxScalar(l, r)
    case (.some(.sequence(let l)), .some(.sequence(let r))):
        sameStyxSequence(l, r)
    case (.some(.object(let l)), .some(.object(let r))):
        sameStyxObject(l, r)
    default:
        false
    }
}

func sameStyxValue(_ lhs: StyxValue, _ rhs: StyxValue) -> Bool {
    sameStyxTag(lhs.tag, rhs.tag) && sameStyxPayload(lhs.payload, rhs.payload)
        && sameStyxSpan(lhs.span, rhs.span)
}

func staxOffCpu(_ seed: UInt64) -> StaxOffCpuBreakdown {
    StaxOffCpuBreakdown(
        idleNs: seed + 1,
        lockNs: seed + 2,
        semaphoreNs: seed + 3,
        ipcNs: seed + 4,
        ioReadNs: seed + 5,
        ioWriteNs: seed + 6,
        readinessNs: seed + 7,
        sleepNs: seed + 8,
        connectNs: seed + 9,
        otherNs: seed + 10
    )
}

func sampleStaxViewParams() -> StaxViewParams {
    StaxViewParams(
        tid: 42,
        filter: StaxLiveFilter(
            timeRange: StaxTimeRange(startNs: 1_000, endNs: 8_500),
            excludeSymbols: [
                StaxSymbolRef(
                    functionName: "malloc_zone_malloc",
                    binary: "libsystem_malloc.dylib"
                ),
                StaxSymbolRef(
                    functionName: nil,
                    binary: "libswift_Concurrency.dylib"
                ),
            ]
        )
    )
}

func sampleStaxFlamegraphUpdate(_ params: StaxViewParams) -> StaxFlamegraphUpdate {
    let tid = params.tid ?? 0
    let filterCount = UInt64(params.filter.excludeSymbols.count)
    let rangeNs: UInt64
    if let range = params.filter.timeRange {
        rangeNs = range.endNs >= range.startNs ? range.endNs - range.startNs : 0
    } else {
        rangeNs = 0
    }
    let totalOnCpuNs = 120_000 + UInt64(tid) + min(rangeNs, 1_000)

    return StaxFlamegraphUpdate(
        totalOnCpuNs: totalOnCpuNs,
        totalOffCpu: staxOffCpu(100 + filterCount),
        strings: [
            "root",
            "bee::decode",
            "libbee.dylib",
            "rust",
            "phon::jit",
            "libphon.dylib",
        ],
        root: StaxFlameNode(
            address: 0,
            functionName: 0,
            binary: nil,
            isMain: true,
            language: 3,
            onCpuNs: totalOnCpuNs,
            offCpu: staxOffCpu(200 + filterCount),
            petSamples: 64,
            offCpuIntervals: 3,
            cycles: 900_000,
            instructions: 600_000,
            l1dMisses: 42,
            branchMispreds: 7,
            children: [
                StaxFlameNode(
                    address: 0x1000 + UInt64(tid),
                    functionName: 1,
                    binary: 2,
                    isMain: true,
                    language: 3,
                    onCpuNs: 80_000 + filterCount,
                    offCpu: staxOffCpu(300 + filterCount),
                    petSamples: 48,
                    offCpuIntervals: 2,
                    cycles: 500_000,
                    instructions: 350_000,
                    l1dMisses: 30,
                    branchMispreds: 5,
                    children: [
                        StaxFlameNode(
                            address: 0x2000 + UInt64(tid),
                            functionName: 4,
                            binary: 5,
                            isMain: false,
                            language: 3,
                            onCpuNs: 45_000,
                            offCpu: staxOffCpu(400 + filterCount),
                            petSamples: 32,
                            offCpuIntervals: 1,
                            cycles: 250_000,
                            instructions: 180_000,
                            l1dMisses: 18,
                            branchMispreds: 3,
                            children: []
                        )
                    ]
                ),
                StaxFlameNode(
                    address: 0x3000 + UInt64(tid),
                    functionName: nil,
                    binary: 2,
                    isMain: false,
                    language: 3,
                    onCpuNs: 20_000,
                    offCpu: staxOffCpu(500 + filterCount),
                    petSamples: 12,
                    offCpuIntervals: 0,
                    cycles: 120_000,
                    instructions: 70_000,
                    l1dMisses: 4,
                    branchMispreds: 1,
                    children: []
                ),
            ]
        )
    )
}

func sampleStaxSecondaryViewParams() -> StaxViewParams {
    StaxViewParams(
        tid: nil,
        filter: StaxLiveFilter(
            timeRange: StaxTimeRange(startNs: 9_000, endNs: 9_640),
            excludeSymbols: [
                StaxSymbolRef(
                    functionName: "mach_msg2_trap",
                    binary: nil
                )
            ]
        )
    )
}

func sampleStaxFlamegraphUpdates() -> [StaxFlamegraphUpdate] {
    [
        sampleStaxFlamegraphUpdate(sampleStaxViewParams()),
        sampleStaxFlamegraphUpdate(sampleStaxSecondaryViewParams()),
    ]
}

func sameStaxOffCpu(_ lhs: StaxOffCpuBreakdown, _ rhs: StaxOffCpuBreakdown) -> Bool {
    lhs.idleNs == rhs.idleNs && lhs.lockNs == rhs.lockNs
        && lhs.semaphoreNs == rhs.semaphoreNs && lhs.ipcNs == rhs.ipcNs
        && lhs.ioReadNs == rhs.ioReadNs && lhs.ioWriteNs == rhs.ioWriteNs
        && lhs.readinessNs == rhs.readinessNs && lhs.sleepNs == rhs.sleepNs
        && lhs.connectNs == rhs.connectNs && lhs.otherNs == rhs.otherNs
}

func sameStaxFlameNode(_ lhs: StaxFlameNode, _ rhs: StaxFlameNode) -> Bool {
    lhs.address == rhs.address && lhs.functionName == rhs.functionName
        && lhs.binary == rhs.binary && lhs.isMain == rhs.isMain
        && lhs.language == rhs.language && lhs.onCpuNs == rhs.onCpuNs
        && sameStaxOffCpu(lhs.offCpu, rhs.offCpu) && lhs.petSamples == rhs.petSamples
        && lhs.offCpuIntervals == rhs.offCpuIntervals && lhs.cycles == rhs.cycles
        && lhs.instructions == rhs.instructions && lhs.l1dMisses == rhs.l1dMisses
        && lhs.branchMispreds == rhs.branchMispreds && lhs.children.count == rhs.children.count
        && zip(lhs.children, rhs.children).allSatisfy { sameStaxFlameNode($0, $1) }
}

func sameStaxFlamegraphUpdate(
    _ lhs: StaxFlamegraphUpdate,
    _ rhs: StaxFlamegraphUpdate
) -> Bool {
    lhs.totalOnCpuNs == rhs.totalOnCpuNs && sameStaxOffCpu(lhs.totalOffCpu, rhs.totalOffCpu)
        && lhs.strings == rhs.strings && sameStaxFlameNode(lhs.root, rhs.root)
}

func sampleStaxLinuxBrokerControlFixture() -> StaxLinuxBrokerControlFixture {
    StaxLinuxBrokerControlFixture(
        config: StaxLinuxPerfSessionConfig(
            targetPid: 42_424,
            frequencyHz: 997,
            kernelStacks: true,
            requestWaking: true,
            requestPmu: true,
            requestDwarfUnwind: false
        ),
        status: StaxLinuxDaemonStatus(
            version: "1.0.0-dev",
            hostArch: "x86_64",
            privileged: true,
            perfEventParanoid: 1
        ),
        errors: [
            .notPrivileged(detail: "perf_event_paranoid=3 without CAP_PERFMON"),
            .perfEventOpen(cpu: 3, errno: 24, detail: "EMFILE while opening PMU sibling"),
            .noSuchTarget(99_999),
            .notAuthorized(callerUid: 501, targetUid: 0),
        ],
        wakingFieldOffsets: StaxLinuxWakingFieldOffsets(
            wakeePidOffset: 16,
            wakeePidSize: 4
        )
    )
}

func sampleStaxMacosConfig() -> StaxMacSessionConfig {
    StaxMacSessionConfig(
        targetPid: 42_424,
        frequencyHz: 997,
        bufRecords: 1_048_576,
        samplers: 0x13,
        pmuEventConfigs: [0xfeed_beef, 0x1_0000_0001],
        classMask: 0b1011,
        filterRangeValue1: 0x3100_0000,
        filterRangeValue2: 0x31ff_ffff,
        typefilterCscs: [0x3101, 0x3102, 0x3108]
    )
}

func sampleStaxMacosBatches() -> [StaxMacKdBufBatch] {
    [
        StaxMacKdBufBatch(
            records: [
                StaxMacKdBuf(
                    timestamp: 900_000,
                    arg1: 0x1000,
                    arg2: 0x2000,
                    arg3: 0x3000,
                    arg4: 0x4000,
                    arg5: 0xfeed_face,
                    debugid: 0x3101_0004,
                    cpuid: 3,
                    unused: 0
                ),
                StaxMacKdBuf(
                    timestamp: 900_128,
                    arg1: 0x1008,
                    arg2: 0x2008,
                    arg3: 0x3008,
                    arg4: 0x4008,
                    arg5: 0xfeed_face,
                    debugid: 0x3101_0008,
                    cpuid: 4,
                    unused: 0
                ),
            ],
            readStartedMachTicks: 899_900,
            drainedMachTicks: 900_140,
            queuedForSendMachTicks: 900_150,
            sendStartedMachTicks: 900_180,
            drainedAtUnixNs: 1_801_000_000_123_456_789
        ),
        StaxMacKdBufBatch(
            records: [
                StaxMacKdBuf(
                    timestamp: 900_256,
                    arg1: 0x1010,
                    arg2: 0x2010,
                    arg3: 0x3010,
                    arg4: 0x4010,
                    arg5: 0xfeed_face,
                    debugid: 0x3101_000c,
                    cpuid: 5,
                    unused: 0
                )
            ],
            readStartedMachTicks: 900_200,
            drainedMachTicks: 900_270,
            queuedForSendMachTicks: 900_290,
            sendStartedMachTicks: 900_310,
            drainedAtUnixNs: 1_801_000_000_123_556_789
        ),
    ]
}

func sampleStaxMacosRecordSummary() -> StaxMacRecordSummary {
    StaxMacRecordSummary(
        recordsDrained: UInt64(sampleStaxMacosBatches().reduce(0) { $0 + $1.records.count }),
        sessionNs: 240_000
    )
}

func sameStaxMacKdBuf(_ lhs: StaxMacKdBuf, _ rhs: StaxMacKdBuf) -> Bool {
    lhs.timestamp == rhs.timestamp && lhs.arg1 == rhs.arg1 && lhs.arg2 == rhs.arg2
        && lhs.arg3 == rhs.arg3 && lhs.arg4 == rhs.arg4 && lhs.arg5 == rhs.arg5
        && lhs.debugid == rhs.debugid && lhs.cpuid == rhs.cpuid && lhs.unused == rhs.unused
}

func sameStaxMacBatch(_ lhs: StaxMacKdBufBatch, _ rhs: StaxMacKdBufBatch) -> Bool {
    lhs.records.count == rhs.records.count
        && zip(lhs.records, rhs.records).allSatisfy { sameStaxMacKdBuf($0, $1) }
        && lhs.readStartedMachTicks == rhs.readStartedMachTicks
        && lhs.drainedMachTicks == rhs.drainedMachTicks
        && lhs.queuedForSendMachTicks == rhs.queuedForSendMachTicks
        && lhs.sendStartedMachTicks == rhs.sendStartedMachTicks
        && lhs.drainedAtUnixNs == rhs.drainedAtUnixNs
}

func sameStaxMacBatches(_ lhs: [StaxMacKdBufBatch], _ rhs: [StaxMacKdBufBatch]) -> Bool {
    lhs.count == rhs.count && zip(lhs, rhs).allSatisfy { sameStaxMacBatch($0, $1) }
}

func sampleHotmealLiveReloadEvents() -> [HotmealLiveReloadEvent] {
    [
        .reload,
        .patches(route: "/guide/", patchesBlob: Data([0, 1, 2, 3, 255])),
        .headChanged(route: "/guide/"),
    ]
}

func sampleHotmealRoute() -> String {
    "/guide/"
}

func sampleHotmealDomNode() -> HotmealDomNode {
    .element(
        tag: "main",
        attrs: [
            HotmealDomAttr(name: "id", value: "app"),
            HotmealDomAttr(name: "data-route", value: "/guide/"),
        ],
        children: [
            .text("Hello "),
            .element(
                tag: "button",
                attrs: [HotmealDomAttr(name: "class", value: "primary")],
                children: [.text("Reload")]
            ),
            .comment("hotmeal-marker"),
        ]
    )
}

func sampleHotmealApplyPatchesResult() -> HotmealApplyPatchesResult {
    let initial = sampleHotmealDomNode()
    return HotmealApplyPatchesResult(
        resultHtml: "<main id=\"app\"><button class=\"primary\">Reload</button></main>",
        normalizedOldHtml: "<main id=\"app\">Hello</main>",
        initialDomTree: initial,
        patchTrace: [
            HotmealPatchStep(
                index: 0,
                patchDebug: "ReplaceText(path=[0], text=\"Hello \")",
                htmlAfter: "<main id=\"app\">Hello </main>",
                domTree: initial,
                error: nil
            ),
            HotmealPatchStep(
                index: 1,
                patchDebug: "InsertChild(path=[1], tag=\"button\")",
                htmlAfter: "<main id=\"app\">Hello <button>Reload</button></main>",
                domTree: .element(
                    tag: "main",
                    attrs: [HotmealDomAttr(name: "id", value: "app")],
                    children: [
                        .text("Hello "),
                        .element(tag: "button", attrs: [], children: [.text("Reload")]),
                    ]
                ),
                error: "sample recoverable mismatch"
            ),
        ]
    )
}

func sameHotmealDomAttr(_ lhs: HotmealDomAttr, _ rhs: HotmealDomAttr) -> Bool {
    lhs.name == rhs.name && lhs.value == rhs.value
}

func sameHotmealDomNode(_ lhs: HotmealDomNode, _ rhs: HotmealDomNode) -> Bool {
    switch (lhs, rhs) {
    case (
        .element(let lTag, let lAttrs, let lChildren),
        .element(let rTag, let rAttrs, let rChildren)
    ):
        return lTag == rTag && lAttrs.count == rAttrs.count && lChildren.count == rChildren.count
            && zip(lAttrs, rAttrs).allSatisfy { sameHotmealDomAttr($0, $1) }
            && zip(lChildren, rChildren).allSatisfy { sameHotmealDomNode($0, $1) }
    case (.text(let lText), .text(let rText)):
        return lText == rText
    case (.comment(let lText), .comment(let rText)):
        return lText == rText
    default:
        return false
    }
}

func sameHotmealPatchStep(_ lhs: HotmealPatchStep, _ rhs: HotmealPatchStep) -> Bool {
    lhs.index == rhs.index && lhs.patchDebug == rhs.patchDebug && lhs.htmlAfter == rhs.htmlAfter
        && sameHotmealDomNode(lhs.domTree, rhs.domTree) && lhs.error == rhs.error
}

func sameHotmealApplyPatchesResult(
    _ lhs: HotmealApplyPatchesResult,
    _ rhs: HotmealApplyPatchesResult
) -> Bool {
    lhs.resultHtml == rhs.resultHtml && lhs.normalizedOldHtml == rhs.normalizedOldHtml
        && sameHotmealDomNode(lhs.initialDomTree, rhs.initialDomTree)
        && lhs.patchTrace.count == rhs.patchTrace.count
        && zip(lhs.patchTrace, rhs.patchTrace).allSatisfy { sameHotmealPatchStep($0, $1) }
}

func sameHotmealLiveReloadEvent(
    _ lhs: HotmealLiveReloadEvent,
    _ rhs: HotmealLiveReloadEvent
) -> Bool {
    switch (lhs, rhs) {
    case (.reload, .reload):
        return true
    case (.patches(let lRoute, let lBlob), .patches(let rRoute, let rBlob)):
        return lRoute == rRoute && lBlob == rBlob
    case (.headChanged(let lRoute), .headChanged(let rRoute)):
        return lRoute == rRoute
    default:
        return false
    }
}

func sampleHelixStreamMetrics() -> HelixStreamMetrics {
    HelixStreamMetrics(
        pulseIds: [101, 102, 103],
        pulseDurationUs: [8_100, 8_250, 8_400],
        decodedTokens: [4, 5, 3],
        committedTokens: [2, 4, 3],
        retainedSpeculativeTokens: [1, 2, 1],
        evictedAudioTokens: [0, 16, 0],
        evictedCommittedTokens: [0, 0, 1],
        rewindK: [0, 2, 1],
        arTokenCount: [4, 6, 3],
        rollingWer: [0.25, 0.20, 0.18],
        s2dP50Ms: [41.5, 39.0, 37.25]
    )
}

func helixAudioRange(_ start: UInt32, _ end: UInt32) -> HelixAudioTokenRange {
    HelixAudioTokenRange(start: start, end: end)
}

func sampleHelixVerifyEvidence() -> HelixVerifyEvidenceDigest {
    HelixVerifyEvidenceDigest(
        pulseId: 102,
        rewindK: 2,
        acceptedPrefixLen: 1,
        divergenceRow: 1,
        drafts: [
            HelixVerifyDraftRow(
                draftIndex: 0,
                draftTokenId: 812,
                verifiedTextTokenId: 44,
                text: "hel",
                status: .accepted,
                expectedObservedAudio: helixAudioRange(10, 18),
                maxDominantAudioMass: 0.73,
                recordCount: 8,
                maxLogit: 12.5,
                draftLogit: 12.4
            ),
            HelixVerifyDraftRow(
                draftIndex: 1,
                draftTokenId: 927,
                verifiedTextTokenId: 45,
                text: "ix",
                status: .divergent,
                expectedObservedAudio: helixAudioRange(18, 26),
                maxDominantAudioMass: 0.61,
                recordCount: 8,
                maxLogit: 11.2,
                draftLogit: 9.9
            ),
            HelixVerifyDraftRow(
                draftIndex: 2,
                draftTokenId: 415,
                verifiedTextTokenId: 46,
                text: "",
                status: .discardedAfterDivergence,
                expectedObservedAudio: helixAudioRange(26, 32),
                maxDominantAudioMass: 0.0,
                recordCount: 0,
                maxLogit: 0.0,
                draftLogit: 0.0
            ),
        ],
        seed: HelixVerifySeedRow(
            queryRow: 3,
            nextTokenSeed: 1401,
            expectedObservedAudio: helixAudioRange(32, 40),
            maxDominantAudioMass: 0.58,
            recordCount: 8,
            maxLogit: 10.75
        )
    )
}

func sampleHelixPulses() -> [HelixPulseAvailable] {
    [
        HelixPulseAvailable(pulseId: 101),
        HelixPulseAvailable(pulseId: 102),
        HelixPulseAvailable(pulseId: 103),
    ]
}

func sampleHelixPulseBundleFields() -> HelixPulseBundleFields {
    HelixPulseBundleFields(
        promptLayout: true,
        audioProvenance: true,
        attentionHeatmap: true,
        encoderFrontier: true,
        encoderProvenance: true,
        audioClip: true,
        melClip: true,
        pulseRollup: true,
        timeline: true,
        gpuChromeEvents: true,
        verifyEvidence: true,
        schedulerSnapshot: true
    )
}

func helixAudioSpan(_ start: UInt32, _ end: UInt32, _ version: UInt32)
    -> HelixAudioRepresentationSpan
{
    HelixAudioRepresentationSpan(
        audio: helixAudioRange(start, end),
        audioRepresentationVersion: version
    )
}

func sampleHelixAudioProvenance() -> [HelixAudioTokenProvenance] {
    [
        HelixAudioTokenProvenance(
            audioTokenId: 16,
            audioRepresentationVersion: 7,
            melFrames: [HelixMelFrameRange(start: 128, end: 136)],
            nativeWindow: 2,
            convStemChunk: 4,
            postMergeAudioTokenId: 16,
            merge: .noMerge(preMergeAudioTokenId: 16),
            admission: .admitAll(admissionSegment: 12),
            cosineToPrevious: 0.9825
        ),
        HelixAudioTokenProvenance(
            audioTokenId: 17,
            audioRepresentationVersion: 7,
            melFrames: [
                HelixMelFrameRange(start: 136, end: 144),
                HelixMelFrameRange(start: 144, end: 152),
            ],
            nativeWindow: 2,
            convStemChunk: 4,
            postMergeAudioTokenId: 17,
            merge: .merged(preMerge: helixAudioRange(17, 19)),
            admission: .admitAll(admissionSegment: 13),
            cosineToPrevious: nil
        ),
    ]
}

func sampleHelixTimeline() -> [HelixStreamingTraceEvent] {
    [
        .pulse(
            startUs: 1_000_000,
            durationUs: 8_250,
            pulseId: 102,
            previousConsumedMelFrames: 1_632,
            consumedMelFrames: 1_648,
            pulseMelFrames: 16,
            committedTextLenStart: 36,
            speculativeLenStart: 3,
            committedTokens: 4,
            retainedSpeculativeTokens: 2,
            residentCommittedTokens: 38,
            evictedAudioTokens: 16,
            evictedCommittedTokens: 0
        ),
        .audioEncoderUpdate(
            startUs: 1_000_200,
            durationUs: 2_100,
            pulseId: 102,
            numAudioFrames: 64,
            firstAudioTokenId: 10,
            residentAudioFrames: 32,
            changedSpanCount: 2,
            changedAudioTokens: 8,
            latestAudioRepresentationVersion: 7
        ),
        .audioEviction(
            timestampUs: 1_000_300,
            pulseId: 102,
            evictedAudioTokens: 16,
            firstAudioTokenId: 10,
            residentAudioFrames: 32,
            audioRingCapacity: 96
        ),
        .refreshPrompt(
            startUs: 1_002_500,
            durationUs: 1_400,
            pulseId: 102,
            firstAudioTokenId: 10,
            residentAudioFrames: 32,
            committedTextLen: 36,
            residentCommittedLen: 32,
            residentTextLen: 35,
            logicalStart: 80,
            logicalEnd: 117,
            textTokenStart: 40,
            textTokenEnd: 44,
            spans: [HelixTracePositionSpan(logicalStart: 80, rows: 16, physicalStart: 12)]
        ),
        .layoutSnapshot(
            timestampUs: 1_003_950,
            pulseId: 102,
            audioLen: 32,
            audioHead: 4,
            firstAudioTokenId: 10,
            textLen: 35,
            firstTextTokenId: 40,
            promptLen: 67,
            residentCommittedLen: 32,
            residentTextLen: 35
        ),
        .verify(
            startUs: 1_004_000,
            durationUs: 900,
            pulseId: 102,
            rewindK: 2,
            postRewindTextLen: 37,
            textTokenStart: 44,
            textTokenEnd: 47,
            logicalStart: 114,
            logicalEnd: 117,
            spans: [HelixTracePositionSpan(logicalStart: 114, rows: 3, physicalStart: 46)],
            acceptedPrefixLen: 1,
            divergenceRow: 1,
            nextTokenSeed: 1401,
            discardedSpeculativeTokens: 1,
            invalidatedSpeculativeSlots: 2
        ),
        .arDecode(
            startUs: 1_005_000,
            durationUs: 2_300,
            pulseId: 102,
            decodeSteps: 5,
            decodedTokens: 5,
            speculativeLenEntering: 1,
            liveSpeculativeTokens: 6,
            hitEos: false,
            seedTokenId: 1401,
            seedTokenText: "hel",
            earlyExitReason: .budgetExhausted,
            nextAfterTail: 1502
        ),
        .arToken(
            startUs: 1_005_100,
            durationUs: 300,
            pulseId: 102,
            stepIndex: 0,
            inputTokenId: 1401,
            inputText: "hel",
            textTokenId: 47,
            queryPosition: 118,
            physicalStart: 49,
            summaryRecords: 64,
            nextTokenId: 1502,
            nextText: "ix"
        ),
        .commit(
            startUs: 1_007_500,
            durationUs: 250,
            pulseId: 102,
            speculativeLenPre: 6,
            revisableTailTarget: 2,
            committedTokens: 4,
            retainedSpeculativeTokens: 2,
            committedTextLen: 40,
            nextAfterCommitted: 1502
        ),
        .verifySkipped(
            timestampUs: 1_007_800,
            pulseId: 102,
            reason: .preCommitFullRewind,
            rewindK: 0,
            residentCommittedLen: 0,
            speculativeLen: 2
        ),
        .textEviction(
            timestampUs: 1_007_900,
            pulseId: 102,
            evictedCommittedTokens: 0,
            residentCommittedCapacity: 128,
            committedTextLen: 40
        ),
    ]
}

func sampleHelixPulseBundle() -> HelixPulseBundle {
    let provenance = sampleHelixAudioProvenance()
    return HelixPulseBundle(
        pulseId: 102,
        schemaVersion: 1,
        promptLayout: HelixPromptLayout(
            pulseId: 102,
            firstAudioTokenId: 10,
            residentAudioFrames: 32,
            changedAudioSpans: [helixAudioSpan(16, 20, 7), helixAudioSpan(24, 28, 8)],
            textTokenStart: 40,
            textTokenEnd: 44,
            textTokens: [
                HelixTextTokenSnapshot(
                    textTokenId: 40,
                    text: "hel",
                    textBefore: "he",
                    inVerifyBatch: true,
                    decodedThisPulse: false
                ),
                HelixTextTokenSnapshot(
                    textTokenId: 41,
                    text: "ix",
                    textBefore: nil,
                    inVerifyBatch: false,
                    decodedThisPulse: true
                ),
            ]
        ),
        audioProvenance: provenance,
        attentionHeatmap: HelixPulseAttentionHeatmap(
            pulseId: 102,
            firstAudioTokenId: 10,
            audioTokenCount: 6,
            textTokenStart: 40,
            textTokenCount: 2,
            recordCount: 16,
            maxValue: 0.42,
            meanAudioMass: [
                0.02, 0.04, 0.08, 0.16, 0.28, 0.42, 0.03, 0.05, 0.09, 0.15, 0.24,
                0.31,
            ],
            textTokenGlyphs: ["hel", "ix"]
        ),
        encoderFrontier: HelixEncoderFrontierSeries(
            pulseId: 102,
            layers: [
                HelixEncoderFrontierLayer(
                    encoderLayerIndex: 0,
                    points: [
                        HelixEncoderFrontierPoint(
                            audioTokenId: 16,
                            meanFrontierDebt: 0.12,
                            headCount: 4
                        ),
                        HelixEncoderFrontierPoint(
                            audioTokenId: 17,
                            meanFrontierDebt: 0.18,
                            headCount: 4
                        ),
                    ]
                ),
                HelixEncoderFrontierLayer(
                    encoderLayerIndex: 1,
                    points: [
                        HelixEncoderFrontierPoint(
                            audioTokenId: 16,
                            meanFrontierDebt: 0.09,
                            headCount: 4
                        )
                    ]
                ),
            ],
            minAudioTokenId: 16,
            maxAudioTokenId: 17,
            minFrontierDebt: 0.09,
            maxFrontierDebt: 0.18
        ),
        encoderProvenance: HelixEncoderProvenanceReport(
            pulseId: 102,
            recordsChecked: 32,
            violations: [
                HelixEncoderProvenanceViolation(
                    audioTokenId: 18,
                    encoderLayerIndex: 2,
                    headIndex: 3,
                    observedAudioTokenId: 21,
                    kind: .versionMismatch,
                    message: "observed audio provenance version lagged refresh"
                )
            ]
        ),
        audioClip: HelixAudioClip(
            sampleRate: 16_000,
            firstSample: 262_144,
            samples: [-0.25, -0.10, 0.0, 0.10, 0.25, 0.50, 0.25, 0.0]
        ),
        melClip: HelixMelClip(
            numMelBins: 4,
            firstMelFrame: 128,
            numMelFrames: 3,
            values: [
                0.10, 0.20, 0.30, 0.40, 0.15, 0.25, 0.35, 0.45, 0.05, 0.12, 0.18,
                0.22,
            ],
            minValue: 0.05,
            maxValue: 0.45,
            corpusMinValue: -1.25,
            corpusMaxValue: 2.75
        ),
        pulseRollup: HelixPulseRollup(
            pulseId: 102,
            pulseStartUs: 1_000_000,
            pulseDurationUs: 8_250,
            encoderDurationUs: 2_100,
            refreshDurationUs: 1_400,
            verifyDurationUs: 900,
            decodeDurationUs: 2_300,
            commitDurationUs: 250,
            pulseMelFrames: 16,
            committedTokens: 4,
            retainedSpeculativeTokens: 2,
            residentCommittedTokens: 38,
            evictedAudioTokens: 16,
            evictedCommittedTokens: 0,
            decodedTokens: 5,
            hitEos: false,
            verify: HelixVerifyOutcome(
                rewindK: 2,
                acceptedPrefixLen: 1,
                divergenceRow: 1,
                discardedSpeculativeTokens: 1
            ),
            hasAttentionBatch: true,
            arTokenCount: 6
        ),
        timeline: sampleHelixTimeline(),
        gpuChromeEvents: [
            HelixChromeTraceEvent(
                name: "metal.dispatch",
                cat: "gpu",
                ph: "X",
                ts: 1_006_000.0,
                dur: 420.0,
                pid: 2,
                tid: 7,
                s: nil,
                args: [:]
            ),
            HelixChromeTraceEvent(
                name: "pulse_marker",
                cat: "scheduler",
                ph: "i",
                ts: 1_007_950.0,
                dur: nil,
                pid: 1,
                tid: 0,
                s: "p",
                args: [:]
            ),
        ],
        verifyEvidence: sampleHelixVerifyEvidence(),
        schedulerSnapshot: HelixPulseEvidenceSnapshot(
            pulseId: 102,
            encoder: HelixEncoderFactsSnapshot(
                refreshedAudio: helixAudioRange(16, 18),
                audioRepresentationVersion: 7,
                provenance: provenance
            ),
            counts: HelixDecoderEvidenceFactCounts(
                decode: 1,
                verifyPrediction: 1,
                verifySeed: 1,
                promptPrefill: 1
            ),
            decode: [
                HelixDecodeFact(
                    textTokenId: 47,
                    queryPosition: 118,
                    inputTokenId: 1401,
                    observedAudio: helixAudioRange(10, 18)
                )
            ],
            verifyPrediction: [
                HelixVerifyPredictionFact(
                    verifiedTextTokenId: 45,
                    verifiedDraftIndex: 1,
                    draftTokenId: 927,
                    queryRow: 2,
                    queryPosition: 116,
                    observedAudio: helixAudioRange(18, 26)
                )
            ],
            verifySeed: [
                HelixVerifySeedFact(
                    queryRow: 3,
                    queryPosition: 117,
                    nextTokenSeed: 1401,
                    observedAudio: helixAudioRange(32, 40)
                )
            ],
            promptPrefill: [
                HelixPromptPrefillFact(
                    queryPosition: 80,
                    observedAudio: helixAudioRange(10, 18)
                )
            ]
        )
    )
}

func sampleHelixAudioClip() -> HelixAudioClip {
    HelixAudioClip(
        sampleRate: 16_000,
        firstSample: 262_144,
        samples: [-0.25, -0.10, 0.0, 0.10, 0.25, 0.50, 0.25, 0.0]
    )
}

func sampleHelixMelClip() -> HelixMelClip {
    HelixMelClip(
        numMelBins: 4,
        firstMelFrame: 128,
        numMelFrames: 3,
        values: [
            0.10, 0.20, 0.30, 0.40, 0.15, 0.25, 0.35, 0.45, 0.05, 0.12, 0.18,
            0.22,
        ],
        minValue: 0.05,
        maxValue: 0.45,
        corpusMinValue: -1.25,
        corpusMaxValue: 2.75
    )
}

func sampleHelixChromeEvents() -> [HelixChromeTraceEvent] {
    [
        HelixChromeTraceEvent(
            name: "metal.dispatch",
            cat: "gpu",
            ph: "X",
            ts: 1_006_000.0,
            dur: 420.0,
            pid: 2,
            tid: 7,
            s: nil,
            args: [:]
        )
    ]
}

func sampleHelixSupport() -> HelixAttentionSupportSummary {
    HelixAttentionSupportSummary(
        totalAudioMass: 0.42,
        observedAudio: helixAudioRange(10, 18),
        dominantAudio: helixAudioRange(16, 18),
        dominantAudioMass: 0.21,
        centerAudioToken: 17.25,
        widthAudioTokens: 3.5
    )
}

func sampleHelixTextSupport() -> [HelixTextAttentionSupportRecord] {
    [
        HelixTextAttentionSupportRecord(
            textTokenId: 47,
            queryPosition: 118,
            decoderLayerIndex: 2,
            headIndex: 3,
            support: sampleHelixSupport(),
            audioWeights: [0.03125, 0.0625, 0.125, 0.25, 0.5]
        )
    ]
}

func sampleHelixAttentionBatch() -> HelixAttentionSummaryBatch {
    HelixAttentionSummaryBatch(
        schemaVersion: 2,
        pulseId: 102,
        audioContextId: 77,
        textContextId: 99,
        audioRepresentationSpans: [helixAudioSpan(10, 18, 7)],
        changedAudioRepresentationSpans: [helixAudioSpan(16, 18, 8)],
        textSupport: sampleHelixTextSupport(),
        headerTextSupport: [
            HelixQueryRowAttentionRecord(
                queryPosition: 80,
                decoderLayerIndex: 1,
                headIndex: 0,
                support: sampleHelixSupport(),
                audioWeights: [0.125, 0.25, 0.375, 0.25]
            )
        ],
        audioEncoderSupport: [
            HelixAudioEncoderSupportRecord(
                audioTokenId: 16,
                audioRepresentationVersion: 7,
                encoderLayerIndex: 0,
                headIndex: 1,
                support: sampleHelixSupport(),
                frontierDebt: 0.125
            )
        ],
        decoderEvidence: [
            HelixDecoderEvidenceRecord(
                textTokenId: 47,
                queryPosition: 118,
                expectedObservedAudio: helixAudioRange(10, 18),
                records: sampleHelixTextSupport(),
                kind: .decode(inputTokenId: 1401)
            ),
            HelixDecoderEvidenceRecord(
                textTokenId: 45,
                queryPosition: 116,
                expectedObservedAudio: helixAudioRange(18, 26),
                records: sampleHelixTextSupport(),
                kind: .verifyPrediction(
                    verifiedDraftIndex: 1,
                    draftTokenId: 927,
                    queryRow: 2,
                    maxLogit: 11.25,
                    draftLogit: 9.875
                )
            ),
            HelixDecoderEvidenceRecord(
                textTokenId: nil,
                queryPosition: 117,
                expectedObservedAudio: helixAudioRange(32, 40),
                records: sampleHelixTextSupport(),
                kind: .verifySeed(queryRow: 3, nextTokenSeed: 1401, maxLogit: 10.75)
            ),
            HelixDecoderEvidenceRecord(
                textTokenId: nil,
                queryPosition: 80,
                expectedObservedAudio: helixAudioRange(10, 18),
                records: sampleHelixTextSupport(),
                kind: .promptPrefill
            ),
        ]
    )
}

func sampleHelixTraceServiceSurface() -> HelixTraceServiceSurface {
    HelixTraceServiceSurface(
        meta: HelixStreamMeta(
            schemaVersion: 2,
            pulseIds: [101, 102],
            timelineEventCount: 420,
            attentionBatchCount: 17
        ),
        pulseRollup: sampleHelixPulseBundle().pulseRollup,
        timeline: sampleHelixTimeline(),
        attentionBatch: sampleHelixAttentionBatch(),
        promptLayout: sampleHelixPulseBundle().promptLayout,
        audioAttendedBy: [
            HelixTextAttendanceRow(
                textTokenId: 47,
                decoderLayerIndex: 2,
                headIndex: 3,
                dominantAudioMass: 0.21,
                totalAudioMass: 0.42,
                observedAudio: helixAudioRange(10, 18),
                dominantAudio: helixAudioRange(16, 18),
                audioWeights: [0.03125, 0.0625, 0.125, 0.25, 0.5],
                queriedAudioWeight: 0.25
            )
        ],
        textAttendsTo: [
            HelixAudioAttendanceRow(
                decoderLayerIndex: 2,
                headIndex: 3,
                dominantAudioMass: 0.21,
                totalAudioMass: 0.42,
                centerAudioToken: 17.25,
                widthAudioTokens: 3.5,
                observedAudio: helixAudioRange(10, 18),
                dominantAudio: helixAudioRange(16, 18),
                audioWeights: [0.03125, 0.0625, 0.125, 0.25, 0.5]
            )
        ],
        refreshAttendsTo: [
            HelixRefreshAttendanceRow(
                queryPosition: 80,
                decoderLayerIndex: 1,
                headIndex: 0,
                dominantAudioMass: 0.375,
                totalAudioMass: 1.0,
                centerAudioToken: 15.5,
                widthAudioTokens: 4.0,
                observedAudio: helixAudioRange(10, 18),
                dominantAudio: helixAudioRange(14, 18),
                audioWeights: [0.125, 0.25, 0.375, 0.25]
            )
        ],
        audioTokenProvenance: sampleHelixAudioProvenance().first,
        audioProvenanceForPulse: sampleHelixAudioProvenance(),
        audioTokensForMelFrame: [16, 17],
        audioClipForAudioToken: sampleHelixAudioClip(),
        audioClipForPrompt: sampleHelixAudioClip(),
        audioClipForAudioRange: sampleHelixAudioClip(),
        melClipForPrompt: sampleHelixMelClip(),
        audioSelfAttention: [
            HelixAudioSelfAttentionRow(
                encoderLayerIndex: 0,
                headIndex: 1,
                audioRepresentationVersion: 7,
                dominantAudioMass: 0.25,
                totalAudioMass: 0.5,
                centerAudioToken: 16.5,
                widthAudioTokens: 2.0,
                observedAudio: helixAudioRange(10, 18),
                dominantAudio: helixAudioRange(16, 18),
                frontierDebt: 0.125
            )
        ],
        transcript: [
            HelixTranscriptToken(textTokenId: 40, decodedInPulse: 101, text: "hel", committed: true),
            HelixTranscriptToken(textTokenId: 41, decodedInPulse: 102, text: "ix", committed: false),
        ],
        pulseAttentionHeatmap: sampleHelixPulseBundle().attentionHeatmap,
        encoderFrontier: sampleHelixPulseBundle().encoderFrontier,
        streamMetrics: sampleHelixStreamMetrics(),
        verifyEvidence: sampleHelixVerifyEvidence(),
        decoderEvidenceReport: HelixDecoderEvidenceReport(
            totalBatches: 7,
            batchesWithoutDecoderEvidence: 1,
            pulsesWithoutDecoderEvidence: [101],
            variantEvidenceCounts: HelixDecoderEvidenceVariantCounts(
                decode: 12,
                verifyPrediction: 6,
                verifySeed: 3,
                promptPrefill: 4
            ),
            variantRecordCounts: HelixDecoderEvidenceVariantCounts(
                decode: 96,
                verifyPrediction: 48,
                verifySeed: 24,
                promptPrefill: 32
            ),
            observedDecoderLayerIndices: [0, 1, 2],
            observedDecoderHeadIndices: [0, 1, 2, 3]
        ),
        pulseEvidenceSnapshot: sampleHelixPulseBundle().schedulerSnapshot,
        gpuChromeEventsForPulse: sampleHelixChromeEvents(),
        runInfo: HelixRunInfo(
            backend: "metal",
            modelDir: "/models/helix-mini",
            input: "helix fixture",
            piece: "demo",
            pulseMs: 8,
            audioRingCapacity: 4096,
            textRingCapacity: 512,
            commitRevisableTailTextTokens: 4,
            reviseLogitMargin: 0.75,
            sampleRate: 16_000,
            melHopSamples: 160,
            numMelBins: 80,
            numMelFrames: 384,
            audioTokensPerChunk: 2,
            nativeWindowTokens: 16,
            realtimePacing: true,
            profilePhases: true,
            attentionTraceSchemaVersion: 3,
            traceServerSchemaVersion: 5
        ),
        pieceEvalReference: HelixPieceEvalReference(
            piece: "demo",
            language: "en",
            words: ["helix", "fixture"]
        ),
        pieceEvalForPulse: HelixPieceEvalSnapshot(
            audioNowMs: 1234.5,
            referenceWordsAvailable: 16,
            hypothesisWords: 15,
            substitutions: 1,
            deletions: 0,
            insertions: 1,
            rollingWer: 0.125,
            s2dMatchedWords: 14,
            s2dNewWords: 2,
            s2dP50Ms: 41.5,
            s2dP90Ms: 75.0,
            s2dP100Ms: 101.25,
            s2dAvgMs: 50.0,
            audioFrontier: 160,
            displayedFrontier: 156,
            committedFrontier: 152,
            lagMs: 250.0
        ),
        encoderProvenanceReport: sampleHelixPulseBundle().encoderProvenance,
        pulseBundleFields: sampleHelixPulseBundleFields(),
        pulseBundle: sampleHelixPulseBundle(),
        pulseAvailable: HelixPulseAvailable(pulseId: 102)
    )
}

func sameHelixStreamMetrics(_ lhs: HelixStreamMetrics, _ rhs: HelixStreamMetrics) -> Bool {
    lhs.pulseIds == rhs.pulseIds
        && lhs.pulseDurationUs == rhs.pulseDurationUs
        && lhs.decodedTokens == rhs.decodedTokens
        && lhs.committedTokens == rhs.committedTokens
        && lhs.retainedSpeculativeTokens == rhs.retainedSpeculativeTokens
        && lhs.evictedAudioTokens == rhs.evictedAudioTokens
        && lhs.evictedCommittedTokens == rhs.evictedCommittedTokens
        && lhs.rewindK == rhs.rewindK
        && lhs.arTokenCount == rhs.arTokenCount
        && lhs.rollingWer == rhs.rollingWer
        && lhs.s2dP50Ms == rhs.s2dP50Ms
}

func sameHelixAudioTokenRange(
    _ lhs: HelixAudioTokenRange,
    _ rhs: HelixAudioTokenRange
) -> Bool {
    lhs.start == rhs.start && lhs.end == rhs.end
}

func sameHelixVerifyDraftStatus(
    _ lhs: HelixVerifyDraftStatus,
    _ rhs: HelixVerifyDraftStatus
) -> Bool {
    switch (lhs, rhs) {
    case (.accepted, .accepted),
        (.divergent, .divergent),
        (.discardedAfterDivergence, .discardedAfterDivergence):
        return true
    default:
        return false
    }
}

func sameHelixVerifyDraftRow(
    _ lhs: HelixVerifyDraftRow,
    _ rhs: HelixVerifyDraftRow
) -> Bool {
    lhs.draftIndex == rhs.draftIndex
        && lhs.draftTokenId == rhs.draftTokenId
        && lhs.verifiedTextTokenId == rhs.verifiedTextTokenId
        && lhs.text == rhs.text
        && sameHelixVerifyDraftStatus(lhs.status, rhs.status)
        && sameHelixAudioTokenRange(lhs.expectedObservedAudio, rhs.expectedObservedAudio)
        && lhs.maxDominantAudioMass == rhs.maxDominantAudioMass
        && lhs.recordCount == rhs.recordCount
        && lhs.maxLogit == rhs.maxLogit
        && lhs.draftLogit == rhs.draftLogit
}

func sameHelixVerifySeedRow(
    _ lhs: HelixVerifySeedRow?,
    _ rhs: HelixVerifySeedRow?
) -> Bool {
    switch (lhs, rhs) {
    case (nil, nil):
        return true
    case (.some(let l), .some(let r)):
        return l.queryRow == r.queryRow
            && l.nextTokenSeed == r.nextTokenSeed
            && sameHelixAudioTokenRange(l.expectedObservedAudio, r.expectedObservedAudio)
            && l.maxDominantAudioMass == r.maxDominantAudioMass
            && l.recordCount == r.recordCount
            && l.maxLogit == r.maxLogit
    default:
        return false
    }
}

func sameHelixVerifyEvidenceDigest(
    _ lhs: HelixVerifyEvidenceDigest,
    _ rhs: HelixVerifyEvidenceDigest
) -> Bool {
    lhs.pulseId == rhs.pulseId
        && lhs.rewindK == rhs.rewindK
        && lhs.acceptedPrefixLen == rhs.acceptedPrefixLen
        && lhs.divergenceRow == rhs.divergenceRow
        && lhs.drafts.count == rhs.drafts.count
        && zip(lhs.drafts, rhs.drafts).allSatisfy { sameHelixVerifyDraftRow($0, $1) }
        && sameHelixVerifySeedRow(lhs.seed, rhs.seed)
}

func sameHelixPulses(_ lhs: [HelixPulseAvailable], _ rhs: [HelixPulseAvailable]) -> Bool {
    lhs.count == rhs.count && zip(lhs, rhs).allSatisfy { $0.pulseId == $1.pulseId }
}

func sameHelixPulseBundle(_ lhs: HelixPulseBundle, _ rhs: HelixPulseBundle) -> Bool {
    String(reflecting: lhs) == String(reflecting: rhs)
}

func sameHelixTraceServiceSurface(
    _ lhs: HelixTraceServiceSurface,
    _ rhs: HelixTraceServiceSurface
) -> Bool {
    sameReflecting(lhs, rhs)
}

func traceyRuleId(_ base: String, _ version: UInt32) -> TraceyRuleId {
    TraceyRuleId(base: base, version: version)
}

func sampleTraceyStatusResponse() -> TraceyStatusResponse {
    TraceyStatusResponse(impls: [
        TraceyImplStatus(
            spec: "vox",
            implName: "rust",
            totalRules: 59,
            coveredRules: 59,
            staleRules: 0,
            verifiedRules: 59
        ),
        TraceyImplStatus(
            spec: "vox",
            implName: "typescript",
            totalRules: 173,
            coveredRules: 173,
            staleRules: 0,
            verifiedRules: 100
        ),
    ])
}

func sampleTraceyQueryRequest() -> TraceyUncoveredRequest {
    TraceyUncoveredRequest(spec: "vox", implName: "rust", prefix: "rpc.channel")
}

func sampleTraceyUntestedRequest() -> TraceyUntestedRequest {
    TraceyUntestedRequest(spec: "vox", implName: "rust", prefix: "rpc.channel")
}

func sampleTraceyStaleRequest() -> TraceyStaleRequest {
    TraceyStaleRequest(spec: "vox", implName: "rust", prefix: "rpc.channel")
}

func sampleTraceyUnmappedRequest() -> TraceyUnmappedRequest {
    TraceyUnmappedRequest(spec: "vox", implName: "rust", path: "rust/vox-codegen/src")
}

func sampleTraceySectionRules() -> [TraceySectionRules] {
    [
        TraceySectionRules(
            section: "Channel Binding",
            rules: [
                TraceyRuleRef(
                    id: traceyRuleId("rpc.channel.direct-args", 1),
                    text: "Channels are direct service arguments."
                ),
                TraceyRuleRef(id: traceyRuleId("rpc.channel.no-collections", 1), text: nil),
            ]
        )
    ]
}

func sampleTraceyUncoveredResponse() -> TraceyUncoveredResponse {
    TraceyUncoveredResponse(
        spec: "vox",
        implName: "rust",
        totalRules: 175,
        uncoveredCount: 2,
        bySection: sampleTraceySectionRules()
    )
}

func sampleTraceyUntestedResponse() -> TraceyUntestedResponse {
    TraceyUntestedResponse(
        spec: "vox",
        implName: "rust",
        totalRules: 175,
        untestedCount: 3,
        bySection: sampleTraceySectionRules()
    )
}

func sampleTraceyStaleResponse() -> TraceyStaleResponse {
    TraceyStaleResponse(
        spec: "vox",
        implName: "rust",
        totalRules: 175,
        staleCount: 1,
        refs: [
            TraceyStaleEntry(
                currentId: traceyRuleId("rpc.channel.direct-args", 2),
                file: "rust/vox-codegen/src/targets/swift/mod.rs",
                line: 67,
                referenceId: traceyRuleId("rpc.channel.direct-args", 1)
            )
        ]
    )
}

func sampleTraceyUnmappedResponse() -> TraceyUnmappedResponse {
    TraceyUnmappedResponse(
        spec: "vox",
        implName: "rust",
        totalUnits: 9,
        unmappedCount: 2,
        entries: [
            TraceyUnmappedEntry(
                path: "rust/vox-codegen/src/targets",
                isDir: true,
                totalUnits: 5,
                unmappedUnits: 1,
                units: []
            ),
            TraceyUnmappedEntry(
                path: "rust/vox-codegen/src/targets/swift/mod.rs",
                isDir: false,
                totalUnits: 4,
                unmappedUnits: 1,
                units: [
                    TraceyUnmappedUnit(
                        kind: "function",
                        name: "emit_tracey_bridge",
                        startLine: 41,
                        endLine: 78
                    )
                ]
            ),
        ]
    )
}

func sampleTraceyApiConfig() -> TraceyApiConfig {
    TraceyApiConfig(
        projectRoot: "/workspace/vox",
        specs: [
            TraceyApiSpecInfo(
                name: "vox",
                prefix: "r",
                source: "docs/content/spec/*.md",
                sourceUrl: "https://vixen.rs/vox/spec",
                implementations: ["rust", "swift", "typescript"]
            )
        ]
    )
}

func sampleTraceyReloadResponse() -> TraceyReloadResponse {
    TraceyReloadResponse(version: 13, rebuildTimeMs: 42)
}

func sampleTraceyHealthResponse() -> TraceyHealthResponse {
    TraceyHealthResponse(
        version: 13,
        watcherActive: true,
        watcherError: nil,
        configError: "ignored include pattern failed to parse",
        watcherLastEventMs: 1_717_000_000_123,
        watcherEventCount: 7,
        watchedDirectories: ["docs/content/spec", "rust"],
        uptimeSecs: 3600
    )
}

func sampleTraceyRuleInfo() -> TraceyRuleInfo {
    TraceyRuleInfo(
        id: traceyRuleId("rpc.channel.direct-args", 1),
        raw: "Channels are direct service arguments.",
        html: "<p>Channels are direct service arguments.</p>",
        sourceFile: "docs/content/spec/vox.md",
        sourceLine: 42,
        coverage: [
            TraceyRuleCoverage(
                spec: "vox",
                implName: "rust",
                implRefs: [
                    TraceyCodeRef(file: "rust/vox-codegen/src/targets/swift/mod.rs", line: 67)
                ],
                verifyRefs: [
                    TraceyCodeRef(file: "spec/spec-tests/tests/cases/testbed.rs", line: 1450)
                ]
            )
        ],
        versionDiff: "Added direct argument wording."
    )
}

func sampleTraceyForwardResponse() -> TraceyApiSpecForward {
    let staleRef = TraceyApiStaleRef(
        file: "swift/subject/Sources/subject-swift/Subject.swift",
        line: 549,
        referenceId: traceyRuleId("rpc.channel.direct-args", 1)
    )
    let rule = TraceyApiRule(
        id: traceyRuleId("rpc.channel.direct-args", 2),
        raw: "Channels are direct service arguments.",
        html: "<p>Channels are direct service arguments.</p>",
        status: "stable",
        level: "must",
        sourceFile: "docs/content/spec/rpc.md",
        sourceLine: 42,
        sourceColumn: 3,
        section: "channel-binding",
        sectionTitle: "Channel Binding",
        implRefs: [
            TraceyCodeRef(file: "rust/vox-codegen/src/targets/typescript/mod.rs", line: 128)
        ],
        verifyRefs: [
            TraceyCodeRef(file: "spec/spec-tests/tests/cases/testbed.rs", line: 3662)
        ],
        dependsRefs: [
            TraceyCodeRef(file: "docs/content/guides/typescript.md", line: 18)
        ],
        isStale: true,
        staleRefs: [staleRef]
    )
    return TraceyApiSpecForward(name: "vox", rules: [rule])
}

func sampleTraceyReverseResponse() -> TraceyApiReverseData {
    TraceyApiReverseData(
        totalUnits: 7,
        coveredUnits: 5,
        files: [
            TraceyApiFileEntry(
                path: "rust/vox-codegen/src/targets/typescript/mod.rs",
                totalUnits: 4,
                coveredUnits: 3
            ),
            TraceyApiFileEntry(
                path: "swift/subject/Sources/subject-swift/Subject.swift",
                totalUnits: 3,
                coveredUnits: 2
            ),
        ]
    )
}

func sampleTraceyFileRequest() -> TraceyFileRequest {
    TraceyFileRequest(
        spec: "vox",
        implName: "rust",
        path: "rust/vox-codegen/src/targets/typescript/mod.rs"
    )
}

func sampleTraceyFileResponse() -> TraceyApiFileData {
    TraceyApiFileData(
        path: "rust/vox-codegen/src/targets/typescript/mod.rs",
        content: "fn emit_tracey_dashboard_bridge() {}\n",
        html: "<pre><span>fn emit_tracey_dashboard_bridge() {}</span></pre>",
        units: [
            TraceyApiCodeUnit(
                kind: "function",
                name: "emit_tracey_dashboard_bridge",
                startLine: 1,
                endLine: 1,
                ruleRefs: ["rpc.channel.direct-args", "encoding.struct"]
            )
        ]
    )
}

func sampleTraceySpecContentResponse() -> TraceyApiSpecData {
    let direct = TraceyOutlineCoverage(implCount: 1, verifyCount: 1, total: 2)
    let aggregate = TraceyOutlineCoverage(implCount: 3, verifyCount: 2, total: 4)
    return TraceyApiSpecData(
        name: "vox",
        sections: [
            TraceySpecSection(
                sourceFile: "docs/content/spec/rpc.md",
                html: "<h2 id=\"channel-binding\">Channel Binding</h2>",
                weight: 20
            )
        ],
        outline: [
            TraceyOutlineEntry(
                title: "Channel Binding",
                slug: "channel-binding",
                level: 2,
                coverage: direct,
                aggregated: aggregate
            )
        ],
        headInjections: ["<script type=\"module\">mermaid.initialize({});</script>"]
    )
}

func sampleTraceySearchResults() -> [TraceySearchResult] {
    [
        TraceySearchResult(
            kind: "rule",
            id: "rpc.channel.direct-args",
            line: 0,
            content: "Channels are direct service arguments.",
            highlighted: "<mark>channel</mark> direct args",
            score: 12.5
        ),
        TraceySearchResult(
            kind: "source",
            id: "rust/vox-codegen/src/targets/typescript/mod.rs",
            line: 128,
            content: "// r[impl rpc.channel.direct-args]",
            highlighted: nil,
            score: 7.25
        ),
    ]
}

func sampleTraceyUpdateFileRangeRequest() -> TraceyUpdateFileRangeRequest {
    TraceyUpdateFileRangeRequest(
        path: "docs/content/spec/rpc.md",
        start: 120,
        end: 144,
        content: "Channels are direct service arguments.",
        fileHash: "sha256:tracey-dashboard-ok"
    )
}

func sampleTraceyUpdateFileRangeConflictRequest() -> TraceyUpdateFileRangeRequest {
    TraceyUpdateFileRangeRequest(
        path: "docs/content/spec/rpc.md",
        start: 120,
        end: 144,
        content: "Channels are direct service arguments.",
        fileHash: "stale"
    )
}

func sampleTraceyUpdateError() -> TraceyUpdateError {
    TraceyUpdateError(message: "file changed on disk")
}

func sampleTraceyConfigPatternRequest() -> TraceyConfigPatternRequest {
    TraceyConfigPatternRequest(
        spec: "vox",
        implName: "typescript",
        pattern: "typescript/**/*.generated.ts"
    )
}

func sampleTraceyBadConfigPatternRequest() -> TraceyConfigPatternRequest {
    TraceyConfigPatternRequest(
        spec: "vox",
        implName: "typescript",
        pattern: "bad[glob"
    )
}

func sampleTraceyValidateRequest() -> TraceyValidateRequest {
    TraceyValidateRequest(spec: "vox", implName: "rust")
}

func sampleTraceyValidationResult() -> TraceyValidationResult {
    TraceyValidationResult(
        spec: "vox",
        implName: "rust",
        errors: [
            TraceyValidationError(
                code: .staleRequirement,
                message: "reference points to an older rule version",
                file: "rust/subject-rust/src/lib.rs",
                line: 12,
                column: 9,
                relatedRules: [traceyRuleId("rpc.channel.direct-args", 2)],
                referenceRuleId: traceyRuleId("rpc.channel.direct-args", 1),
                referenceText: "r[impl rpc.channel.direct-args]"
            ),
            TraceyValidationError(
                code: .unknownRequirement,
                message: "unknown requirement",
                file: nil,
                line: nil,
                column: nil,
                relatedRules: [],
                referenceRuleId: nil,
                referenceText: "r[verify typo.rule]"
            ),
        ],
        warningCount: 1,
        errorCount: 1
    )
}

func sampleTraceyLspContent() -> String {
    "// r[impl rpc.channel.direct-args]\nfn main() {}\n"
}

func sampleTraceyLspPositionRequest() -> TraceyLspPositionRequest {
    TraceyLspPositionRequest(
        path: "src/lib.rs",
        content: sampleTraceyLspContent(),
        line: 0,
        character: 8
    )
}

func sampleTraceyLspReferencesRequest() -> TraceyLspReferencesRequest {
    TraceyLspReferencesRequest(
        path: "src/lib.rs",
        content: sampleTraceyLspContent(),
        line: 0,
        character: 8,
        includeDeclaration: true
    )
}

func sampleTraceyLspDocumentRequest() -> TraceyLspDocumentRequest {
    TraceyLspDocumentRequest(path: "src/lib.rs", content: sampleTraceyLspContent())
}

func sampleTraceyLspInlayHintsRequest() -> TraceyLspInlayHintsRequest {
    TraceyLspInlayHintsRequest(
        path: "src/lib.rs",
        content: sampleTraceyLspContent(),
        startLine: 0,
        endLine: 2
    )
}

func sampleTraceyLspRenameRequest() -> TraceyLspRenameRequest {
    TraceyLspRenameRequest(
        path: "src/lib.rs",
        content: sampleTraceyLspContent(),
        line: 0,
        character: 8,
        newName: "rpc.channel.direct-args-renamed"
    )
}

func sampleTraceyLspLocations() -> [TraceyLspLocation] {
    [
        TraceyLspLocation(path: "docs/content/spec/rpc.md", line: 211, character: 3),
        TraceyLspLocation(path: "spec/spec-tests/tests/cases/testbed.rs", line: 1450, character: 6),
    ]
}

func sampleTraceyHoverInfo() -> TraceyHoverInfo {
    TraceyHoverInfo(
        ruleId: traceyRuleId("rpc.channel.direct-args", 1),
        raw: "Channels are direct service arguments.",
        specName: "vox",
        specUrl: "https://vixen.rs/vox/spec/rpc",
        sourceFile: "docs/content/spec/rpc.md",
        implCount: 1,
        verifyCount: 1,
        implRefs: [
            TraceyCodeRef(file: "rust/vox-codegen/src/targets/swift/mod.rs", line: 67)
        ],
        verifyRefs: [
            TraceyCodeRef(file: "spec/spec-tests/tests/cases/testbed.rs", line: 1450)
        ],
        rangeStartLine: 0,
        rangeStartChar: 3,
        rangeEndLine: 0,
        rangeEndChar: 36,
        versionDiff: "Added direct argument wording."
    )
}

func sampleTraceyLspCompletions() -> [TraceyLspCompletionItem] {
    [
        TraceyLspCompletionItem(
            label: "impl",
            kind: "verb",
            detail: "implementation reference",
            documentation: nil,
            insertText: "impl "
        ),
        TraceyLspCompletionItem(
            label: "rpc.channel.direct-args",
            kind: "rule",
            detail: "vox",
            documentation: "Channels are direct service arguments.",
            insertText: nil
        ),
    ]
}

func sampleTraceyLspWorkspaceDiagnostics() -> [TraceyLspFileDiagnostics] {
    [
        TraceyLspFileDiagnostics(
            path: "src/lib.rs",
            diagnostics: [
                TraceyLspDiagnostic(
                    severity: "warning",
                    code: "stale_requirement",
                    message: "reference points to an older rule version",
                    startLine: 7,
                    startChar: 4,
                    endLine: 7,
                    endChar: 41
                )
            ]
        )
    ]
}

func sampleTraceyLspSymbols() -> [TraceyLspSymbol] {
    [
        TraceyLspSymbol(
            name: "rpc.channel.direct-args",
            kind: "impl",
            path: "src/lib.rs",
            startLine: 0,
            startChar: 3,
            endLine: 0,
            endChar: 36
        ),
        TraceyLspSymbol(
            name: "rpc.channel.no-collections",
            kind: "verify",
            path: "spec/spec-tests/tests/cases/testbed.rs",
            startLine: 1450,
            startChar: 6,
            endLine: 1450,
            endChar: 41
        ),
    ]
}

func sampleTraceyLspSemanticTokens() -> [TraceyLspSemanticToken] {
    [
        TraceyLspSemanticToken(line: 0, startChar: 3, length: 4, tokenType: 0, modifiers: 0),
        TraceyLspSemanticToken(line: 0, startChar: 8, length: 23, tokenType: 1, modifiers: 2),
    ]
}

func sampleTraceyLspCodeLens() -> [TraceyLspCodeLens] {
    [
        TraceyLspCodeLens(
            line: 0,
            startChar: 3,
            endChar: 36,
            title: "1 impl, 1 verify",
            command: "tracey.showRule",
            arguments: ["rpc.channel.direct-args"]
        )
    ]
}

func sampleTraceyLspInlayHints() -> [TraceyLspInlayHint] {
    [
        TraceyLspInlayHint(line: 0, character: 36, label: "covered")
    ]
}

func sampleTraceyPrepareRenameResult() -> TraceyPrepareRenameResult {
    TraceyPrepareRenameResult(
        startLine: 0,
        startChar: 8,
        endLine: 0,
        endChar: 31,
        placeholder: "rpc.channel.direct-args"
    )
}

func sampleTraceyLspTextEdits() -> [TraceyLspTextEdit] {
    [
        TraceyLspTextEdit(
            path: "src/lib.rs",
            startLine: 0,
            startChar: 8,
            endLine: 0,
            endChar: 31,
            newText: "rpc.channel.direct-args-renamed"
        ),
        TraceyLspTextEdit(
            path: "docs/content/spec/rpc.md",
            startLine: 211,
            startChar: 3,
            endLine: 211,
            endChar: 26,
            newText: "rpc.channel.direct-args-renamed"
        ),
    ]
}

func sampleTraceyLspCodeActions() -> [TraceyLspCodeAction] {
    [
        TraceyLspCodeAction(
            title: "Open requirement",
            kind: "quickfix",
            command: "tracey.openRule",
            arguments: ["rpc.channel.direct-args"],
            isPreferred: true
        )
    ]
}

func sampleTraceyUpdates() -> [TraceyDataUpdate] {
    [
        TraceyDataUpdate(version: 11, delta: nil),
        TraceyDataUpdate(
            version: 12,
            delta: TraceyDeltaSummary(
                newlyCovered: [
                    TraceyCoverageChange(
                        ruleId: traceyRuleId("rpc.channel.direct-args", 1),
                        file: "rust/vox-codegen/src/targets/swift/mod.rs",
                        line: 67
                    )
                ],
                newlyUncovered: [traceyRuleId("rpc.channel.no-collections", 1)]
            )
        ),
    ]
}

func sameTraceyRuleId(_ lhs: TraceyRuleId, _ rhs: TraceyRuleId) -> Bool {
    lhs.base == rhs.base && lhs.version == rhs.version
}

func sameOptionalTraceyRuleId(_ lhs: TraceyRuleId?, _ rhs: TraceyRuleId?) -> Bool {
    switch (lhs, rhs) {
    case (nil, nil):
        return true
    case (.some(let l), .some(let r)):
        return sameTraceyRuleId(l, r)
    default:
        return false
    }
}

func sameTraceyCodeRef(_ lhs: TraceyCodeRef, _ rhs: TraceyCodeRef) -> Bool {
    lhs.file == rhs.file && lhs.line == rhs.line
}

func sameTraceyStatusResponse(_ lhs: TraceyStatusResponse, _ rhs: TraceyStatusResponse) -> Bool {
    lhs.impls.count == rhs.impls.count
        && zip(lhs.impls, rhs.impls).allSatisfy {
            $0.spec == $1.spec && $0.implName == $1.implName
                && $0.totalRules == $1.totalRules && $0.coveredRules == $1.coveredRules
                && $0.staleRules == $1.staleRules && $0.verifiedRules == $1.verifiedRules
        }
}

func sameTraceyUncoveredRequest(
    _ lhs: TraceyUncoveredRequest,
    _ rhs: TraceyUncoveredRequest
) -> Bool {
    lhs.spec == rhs.spec && lhs.implName == rhs.implName && lhs.prefix == rhs.prefix
}

func sameTraceyUntestedRequest(
    _ lhs: TraceyUntestedRequest,
    _ rhs: TraceyUntestedRequest
) -> Bool {
    lhs.spec == rhs.spec && lhs.implName == rhs.implName && lhs.prefix == rhs.prefix
}

func sameTraceyStaleRequest(_ lhs: TraceyStaleRequest, _ rhs: TraceyStaleRequest) -> Bool {
    lhs.spec == rhs.spec && lhs.implName == rhs.implName && lhs.prefix == rhs.prefix
}

func sameTraceyUnmappedRequest(
    _ lhs: TraceyUnmappedRequest,
    _ rhs: TraceyUnmappedRequest
) -> Bool {
    lhs.spec == rhs.spec && lhs.implName == rhs.implName && lhs.path == rhs.path
}

func sameTraceyRuleRef(_ lhs: TraceyRuleRef, _ rhs: TraceyRuleRef) -> Bool {
    sameTraceyRuleId(lhs.id, rhs.id) && lhs.text == rhs.text
}

func sameTraceySectionRules(_ lhs: TraceySectionRules, _ rhs: TraceySectionRules) -> Bool {
    lhs.section == rhs.section
        && lhs.rules.count == rhs.rules.count
        && zip(lhs.rules, rhs.rules).allSatisfy { sameTraceyRuleRef($0, $1) }
}

func sameTraceyUncoveredResponse(
    _ lhs: TraceyUncoveredResponse,
    _ rhs: TraceyUncoveredResponse
) -> Bool {
    lhs.spec == rhs.spec
        && lhs.implName == rhs.implName
        && lhs.totalRules == rhs.totalRules
        && lhs.uncoveredCount == rhs.uncoveredCount
        && lhs.bySection.count == rhs.bySection.count
        && zip(lhs.bySection, rhs.bySection).allSatisfy { sameTraceySectionRules($0, $1) }
}

func sameTraceyUntestedResponse(
    _ lhs: TraceyUntestedResponse,
    _ rhs: TraceyUntestedResponse
) -> Bool {
    lhs.spec == rhs.spec
        && lhs.implName == rhs.implName
        && lhs.totalRules == rhs.totalRules
        && lhs.untestedCount == rhs.untestedCount
        && lhs.bySection.count == rhs.bySection.count
        && zip(lhs.bySection, rhs.bySection).allSatisfy { sameTraceySectionRules($0, $1) }
}

func sameTraceyStaleEntry(_ lhs: TraceyStaleEntry, _ rhs: TraceyStaleEntry) -> Bool {
    sameTraceyRuleId(lhs.currentId, rhs.currentId)
        && lhs.file == rhs.file
        && lhs.line == rhs.line
        && sameTraceyRuleId(lhs.referenceId, rhs.referenceId)
}

func sameTraceyStaleResponse(_ lhs: TraceyStaleResponse, _ rhs: TraceyStaleResponse) -> Bool {
    lhs.spec == rhs.spec
        && lhs.implName == rhs.implName
        && lhs.totalRules == rhs.totalRules
        && lhs.staleCount == rhs.staleCount
        && lhs.refs.count == rhs.refs.count
        && zip(lhs.refs, rhs.refs).allSatisfy { sameTraceyStaleEntry($0, $1) }
}

func sameTraceyUnmappedUnit(_ lhs: TraceyUnmappedUnit, _ rhs: TraceyUnmappedUnit) -> Bool {
    lhs.kind == rhs.kind
        && lhs.name == rhs.name
        && lhs.startLine == rhs.startLine
        && lhs.endLine == rhs.endLine
}

func sameTraceyUnmappedEntry(_ lhs: TraceyUnmappedEntry, _ rhs: TraceyUnmappedEntry) -> Bool {
    lhs.path == rhs.path
        && lhs.isDir == rhs.isDir
        && lhs.totalUnits == rhs.totalUnits
        && lhs.unmappedUnits == rhs.unmappedUnits
        && lhs.units.count == rhs.units.count
        && zip(lhs.units, rhs.units).allSatisfy { sameTraceyUnmappedUnit($0, $1) }
}

func sameTraceyUnmappedResponse(
    _ lhs: TraceyUnmappedResponse,
    _ rhs: TraceyUnmappedResponse
) -> Bool {
    lhs.spec == rhs.spec
        && lhs.implName == rhs.implName
        && lhs.totalUnits == rhs.totalUnits
        && lhs.unmappedCount == rhs.unmappedCount
        && lhs.entries.count == rhs.entries.count
        && zip(lhs.entries, rhs.entries).allSatisfy { sameTraceyUnmappedEntry($0, $1) }
}

func sameTraceyApiSpecInfo(_ lhs: TraceyApiSpecInfo, _ rhs: TraceyApiSpecInfo) -> Bool {
    lhs.name == rhs.name
        && lhs.prefix == rhs.prefix
        && lhs.source == rhs.source
        && lhs.sourceUrl == rhs.sourceUrl
        && lhs.implementations == rhs.implementations
}

func sameTraceyApiConfig(_ lhs: TraceyApiConfig, _ rhs: TraceyApiConfig) -> Bool {
    lhs.projectRoot == rhs.projectRoot
        && lhs.specs.count == rhs.specs.count
        && zip(lhs.specs, rhs.specs).allSatisfy { sameTraceyApiSpecInfo($0, $1) }
}

func sameTraceyReloadResponse(
    _ lhs: TraceyReloadResponse,
    _ rhs: TraceyReloadResponse
) -> Bool {
    lhs.version == rhs.version && lhs.rebuildTimeMs == rhs.rebuildTimeMs
}

func sameTraceyHealthResponse(
    _ lhs: TraceyHealthResponse,
    _ rhs: TraceyHealthResponse
) -> Bool {
    lhs.version == rhs.version
        && lhs.watcherActive == rhs.watcherActive
        && lhs.watcherError == rhs.watcherError
        && lhs.configError == rhs.configError
        && lhs.watcherLastEventMs == rhs.watcherLastEventMs
        && lhs.watcherEventCount == rhs.watcherEventCount
        && lhs.watchedDirectories == rhs.watchedDirectories
        && lhs.uptimeSecs == rhs.uptimeSecs
}

func sameTraceyRuleCoverage(_ lhs: TraceyRuleCoverage, _ rhs: TraceyRuleCoverage) -> Bool {
    lhs.spec == rhs.spec && lhs.implName == rhs.implName
        && lhs.implRefs.count == rhs.implRefs.count
        && lhs.verifyRefs.count == rhs.verifyRefs.count
        && zip(lhs.implRefs, rhs.implRefs).allSatisfy { sameTraceyCodeRef($0, $1) }
        && zip(lhs.verifyRefs, rhs.verifyRefs).allSatisfy { sameTraceyCodeRef($0, $1) }
}

func sameTraceyRuleInfo(_ lhs: TraceyRuleInfo?, _ rhs: TraceyRuleInfo?) -> Bool {
    switch (lhs, rhs) {
    case (nil, nil):
        return true
    case (.some(let l), .some(let r)):
        return sameTraceyRuleId(l.id, r.id)
            && l.raw == r.raw
            && l.html == r.html
            && l.sourceFile == r.sourceFile
            && l.sourceLine == r.sourceLine
            && l.coverage.count == r.coverage.count
            && zip(l.coverage, r.coverage).allSatisfy { sameTraceyRuleCoverage($0, $1) }
            && l.versionDiff == r.versionDiff
    default:
        return false
    }
}

func sameTraceyDashboardValue<T>(_ lhs: T, _ rhs: T) -> Bool {
    sameReflecting(lhs, rhs)
}

func sameTraceyValidationErrorCode(
    _ lhs: TraceyValidationErrorCode,
    _ rhs: TraceyValidationErrorCode
) -> Bool {
    switch (lhs, rhs) {
    case (.circularDependency, .circularDependency),
        (.invalidNaming, .invalidNaming),
        (.unknownRequirement, .unknownRequirement),
        (.staleRequirement, .staleRequirement),
        (.duplicateRequirement, .duplicateRequirement),
        (.unknownPrefix, .unknownPrefix),
        (.implInTestFile, .implInTestFile),
        (.includeUnparseableFile, .includeUnparseableFile):
        return true
    default:
        return false
    }
}

func sameTraceyValidationError(
    _ lhs: TraceyValidationError,
    _ rhs: TraceyValidationError
) -> Bool {
    sameTraceyValidationErrorCode(lhs.code, rhs.code)
        && lhs.message == rhs.message
        && lhs.file == rhs.file
        && lhs.line == rhs.line
        && lhs.column == rhs.column
        && lhs.relatedRules.count == rhs.relatedRules.count
        && zip(lhs.relatedRules, rhs.relatedRules).allSatisfy { sameTraceyRuleId($0, $1) }
        && sameOptionalTraceyRuleId(lhs.referenceRuleId, rhs.referenceRuleId)
        && lhs.referenceText == rhs.referenceText
}

func sameTraceyValidationResult(
    _ lhs: TraceyValidationResult,
    _ rhs: TraceyValidationResult
) -> Bool {
    lhs.spec == rhs.spec
        && lhs.implName == rhs.implName
        && lhs.warningCount == rhs.warningCount
        && lhs.errorCount == rhs.errorCount
        && lhs.errors.count == rhs.errors.count
        && zip(lhs.errors, rhs.errors).allSatisfy { sameTraceyValidationError($0, $1) }
}

func sameTraceyLspPositionRequest(
    _ lhs: TraceyLspPositionRequest,
    _ rhs: TraceyLspPositionRequest
) -> Bool {
    lhs.path == rhs.path && lhs.content == rhs.content && lhs.line == rhs.line
        && lhs.character == rhs.character
}

func sameTraceyLspReferencesRequest(
    _ lhs: TraceyLspReferencesRequest,
    _ rhs: TraceyLspReferencesRequest
) -> Bool {
    lhs.path == rhs.path && lhs.content == rhs.content && lhs.line == rhs.line
        && lhs.character == rhs.character && lhs.includeDeclaration == rhs.includeDeclaration
}

func sameTraceyLspDocumentRequest(
    _ lhs: TraceyLspDocumentRequest,
    _ rhs: TraceyLspDocumentRequest
) -> Bool {
    lhs.path == rhs.path && lhs.content == rhs.content
}

func sameTraceyLspInlayHintsRequest(
    _ lhs: TraceyLspInlayHintsRequest,
    _ rhs: TraceyLspInlayHintsRequest
) -> Bool {
    lhs.path == rhs.path && lhs.content == rhs.content && lhs.startLine == rhs.startLine
        && lhs.endLine == rhs.endLine
}

func sameTraceyLspRenameRequest(
    _ lhs: TraceyLspRenameRequest,
    _ rhs: TraceyLspRenameRequest
) -> Bool {
    lhs.path == rhs.path && lhs.content == rhs.content && lhs.line == rhs.line
        && lhs.character == rhs.character && lhs.newName == rhs.newName
}

func sameTraceyLspLocations(
    _ lhs: [TraceyLspLocation],
    _ rhs: [TraceyLspLocation]
) -> Bool {
    lhs.count == rhs.count
        && zip(lhs, rhs).allSatisfy {
            $0.path == $1.path && $0.line == $1.line && $0.character == $1.character
        }
}

func sameTraceyHoverInfo(_ lhs: TraceyHoverInfo?, _ rhs: TraceyHoverInfo?) -> Bool {
    switch (lhs, rhs) {
    case (nil, nil):
        return true
    case (.some(let l), .some(let r)):
        return sameTraceyRuleId(l.ruleId, r.ruleId)
            && l.raw == r.raw
            && l.specName == r.specName
            && l.specUrl == r.specUrl
            && l.sourceFile == r.sourceFile
            && l.implCount == r.implCount
            && l.verifyCount == r.verifyCount
            && l.implRefs.count == r.implRefs.count
            && l.verifyRefs.count == r.verifyRefs.count
            && zip(l.implRefs, r.implRefs).allSatisfy { sameTraceyCodeRef($0, $1) }
            && zip(l.verifyRefs, r.verifyRefs).allSatisfy { sameTraceyCodeRef($0, $1) }
            && l.rangeStartLine == r.rangeStartLine
            && l.rangeStartChar == r.rangeStartChar
            && l.rangeEndLine == r.rangeEndLine
            && l.rangeEndChar == r.rangeEndChar
            && l.versionDiff == r.versionDiff
    default:
        return false
    }
}

func sameTraceyLspCompletions(
    _ lhs: [TraceyLspCompletionItem],
    _ rhs: [TraceyLspCompletionItem]
) -> Bool {
    lhs.count == rhs.count
        && zip(lhs, rhs).allSatisfy {
            $0.label == $1.label && $0.kind == $1.kind && $0.detail == $1.detail
                && $0.documentation == $1.documentation && $0.insertText == $1.insertText
        }
}

func sameTraceyLspWorkspaceDiagnostics(
    _ lhs: [TraceyLspFileDiagnostics],
    _ rhs: [TraceyLspFileDiagnostics]
) -> Bool {
    lhs.count == rhs.count
        && zip(lhs, rhs).allSatisfy { left, right in
            left.path == right.path
                && left.diagnostics.count == right.diagnostics.count
                && zip(left.diagnostics, right.diagnostics).allSatisfy {
                    $0.severity == $1.severity && $0.code == $1.code && $0.message == $1.message
                        && $0.startLine == $1.startLine && $0.startChar == $1.startChar
                        && $0.endLine == $1.endLine && $0.endChar == $1.endChar
                }
        }
}

func sameTraceyLspSymbols(_ lhs: [TraceyLspSymbol], _ rhs: [TraceyLspSymbol]) -> Bool {
    lhs.count == rhs.count
        && zip(lhs, rhs).allSatisfy {
            $0.name == $1.name && $0.kind == $1.kind && $0.path == $1.path
                && $0.startLine == $1.startLine && $0.startChar == $1.startChar
                && $0.endLine == $1.endLine && $0.endChar == $1.endChar
        }
}

func sameTraceyLspSemanticTokens(
    _ lhs: [TraceyLspSemanticToken],
    _ rhs: [TraceyLspSemanticToken]
) -> Bool {
    lhs.count == rhs.count
        && zip(lhs, rhs).allSatisfy {
            $0.line == $1.line && $0.startChar == $1.startChar && $0.length == $1.length
                && $0.tokenType == $1.tokenType && $0.modifiers == $1.modifiers
        }
}

func sameTraceyLspCodeLens(_ lhs: [TraceyLspCodeLens], _ rhs: [TraceyLspCodeLens]) -> Bool {
    lhs.count == rhs.count
        && zip(lhs, rhs).allSatisfy {
            $0.line == $1.line && $0.startChar == $1.startChar && $0.endChar == $1.endChar
                && $0.title == $1.title && $0.command == $1.command
                && $0.arguments == $1.arguments
        }
}

func sameTraceyLspInlayHints(
    _ lhs: [TraceyLspInlayHint],
    _ rhs: [TraceyLspInlayHint]
) -> Bool {
    lhs.count == rhs.count
        && zip(lhs, rhs).allSatisfy {
            $0.line == $1.line && $0.character == $1.character && $0.label == $1.label
        }
}

func sameTraceyPrepareRenameResult(
    _ lhs: TraceyPrepareRenameResult?,
    _ rhs: TraceyPrepareRenameResult?
) -> Bool {
    switch (lhs, rhs) {
    case (nil, nil):
        return true
    case (.some(let l), .some(let r)):
        return l.startLine == r.startLine && l.startChar == r.startChar
            && l.endLine == r.endLine && l.endChar == r.endChar
            && l.placeholder == r.placeholder
    default:
        return false
    }
}

func sameTraceyLspTextEdits(_ lhs: [TraceyLspTextEdit], _ rhs: [TraceyLspTextEdit]) -> Bool {
    lhs.count == rhs.count
        && zip(lhs, rhs).allSatisfy {
            $0.path == $1.path && $0.startLine == $1.startLine
                && $0.startChar == $1.startChar && $0.endLine == $1.endLine
                && $0.endChar == $1.endChar && $0.newText == $1.newText
        }
}

func sameTraceyLspCodeActions(
    _ lhs: [TraceyLspCodeAction],
    _ rhs: [TraceyLspCodeAction]
) -> Bool {
    lhs.count == rhs.count
        && zip(lhs, rhs).allSatisfy {
            $0.title == $1.title && $0.kind == $1.kind && $0.command == $1.command
                && $0.arguments == $1.arguments && $0.isPreferred == $1.isPreferred
        }
}

func sameTraceyDeltaSummary(_ lhs: TraceyDeltaSummary?, _ rhs: TraceyDeltaSummary?) -> Bool {
    switch (lhs, rhs) {
    case (nil, nil):
        return true
    case (.some(let l), .some(let r)):
        return l.newlyCovered.count == r.newlyCovered.count
            && l.newlyUncovered.count == r.newlyUncovered.count
            && zip(l.newlyCovered, r.newlyCovered).allSatisfy {
                sameTraceyRuleId($0.ruleId, $1.ruleId) && $0.file == $1.file && $0.line == $1.line
            }
            && zip(l.newlyUncovered, r.newlyUncovered).allSatisfy { sameTraceyRuleId($0, $1) }
    default:
        return false
    }
}

func sameTraceyUpdates(_ lhs: [TraceyDataUpdate], _ rhs: [TraceyDataUpdate]) -> Bool {
    lhs.count == rhs.count
        && zip(lhs, rhs).allSatisfy { $0.version == $1.version && sameTraceyDeltaSummary($0.delta, $1.delta) }
}

func sampleDibsListRequest() -> DibsListRequest {
    DibsListRequest(
        table: "products",
        filters: [
            DibsFilter(field: "active", op: .eq, value: .bool(true), values: []),
            DibsFilter(field: "id", op: .`in`, value: .null, values: [.i64(1), .i64(2)]),
            DibsFilter(field: "metadata", op: .jsonGetText, value: .string("sku"), values: []),
        ],
        sort: [DibsSort(field: "created_at", dir: .desc)],
        limit: 2,
        offset: 0,
        select: ["id", "name", "active", "payload"]
    )
}

func sampleDibsListResponse() -> DibsListResponse {
    DibsListResponse(
        rows: [sampleDibsRowOne(), sampleDibsRowTwo()],
        total: 2
    )
}

func sampleDibsRowOne() -> DibsRow {
    DibsRow(fields: [
        DibsRowField(name: "id", value: .i64(1)),
        DibsRowField(name: "name", value: .string("phon adapter")),
        DibsRowField(name: "active", value: .bool(true)),
        DibsRowField(name: "score", value: .f64(9.5)),
        DibsRowField(name: "payload", value: .bytes(Data([0, 1, 2, 255]))),
    ])
}

func sampleDibsRowTwo() -> DibsRow {
    DibsRow(fields: [
        DibsRowField(name: "id", value: .i64(2)),
        DibsRowField(name: "name", value: .string("vox bridge")),
        DibsRowField(name: "active", value: .bool(false)),
        DibsRowField(name: "small", value: .i16(7)),
        DibsRowField(name: "count", value: .i32(42)),
        DibsRowField(name: "ratio", value: .f32(0.5)),
        DibsRowField(name: "deleted_at", value: .null),
        DibsRowField(name: "payload", value: .bytes(Data())),
    ])
}

func sampleDibsSchema() -> DibsSchemaInfo {
    DibsSchemaInfo(tables: [
        DibsTableInfo(
            name: "products",
            columns: [
                DibsColumnInfo(
                    name: "id",
                    sqlType: "BIGINT",
                    rustType: "i64",
                    nullable: false,
                    default: "generated by default as identity",
                    primaryKey: true,
                    unique: true,
                    autoGenerated: true,
                    long: false,
                    label: false,
                    enumVariants: [],
                    doc: "Product primary key",
                    lang: nil,
                    icon: "hash",
                    subtype: nil
                ),
                DibsColumnInfo(
                    name: "name",
                    sqlType: "TEXT",
                    rustType: "String",
                    nullable: false,
                    default: nil,
                    primaryKey: false,
                    unique: false,
                    autoGenerated: false,
                    long: false,
                    label: true,
                    enumVariants: [],
                    doc: "Display name",
                    lang: nil,
                    icon: "text",
                    subtype: nil
                ),
                DibsColumnInfo(
                    name: "status",
                    sqlType: "TEXT",
                    rustType: "ProductStatus",
                    nullable: false,
                    default: "'draft'",
                    primaryKey: false,
                    unique: false,
                    autoGenerated: false,
                    long: false,
                    label: false,
                    enumVariants: ["draft", "active"],
                    doc: nil,
                    lang: nil,
                    icon: "badge",
                    subtype: nil
                ),
                DibsColumnInfo(
                    name: "metadata",
                    sqlType: "JSONB",
                    rustType: "Jsonb<facet_value::Value>",
                    nullable: true,
                    default: nil,
                    primaryKey: false,
                    unique: false,
                    autoGenerated: false,
                    long: true,
                    label: false,
                    enumVariants: [],
                    doc: "Structured product metadata",
                    lang: "json",
                    icon: "braces",
                    subtype: nil
                ),
                DibsColumnInfo(
                    name: "category_id",
                    sqlType: "BIGINT",
                    rustType: "Option<i64>",
                    nullable: true,
                    default: nil,
                    primaryKey: false,
                    unique: false,
                    autoGenerated: false,
                    long: false,
                    label: false,
                    enumVariants: [],
                    doc: nil,
                    lang: nil,
                    icon: "link",
                    subtype: nil
                ),
            ],
            foreignKeys: [
                DibsForeignKeyInfo(
                    columns: ["category_id"],
                    referencesTable: "categories",
                    referencesColumns: ["id"]
                )
            ],
            indices: [
                DibsIndexInfo(
                    name: "products_active_created_at_idx",
                    columns: [
                        DibsIndexColumnInfo(name: "active", order: "asc", nulls: "default"),
                        DibsIndexColumnInfo(name: "created_at", order: "desc", nulls: "last"),
                    ],
                    unique: false,
                    whereClause: "deleted_at IS NULL"
                )
            ],
            sourceFile: "examples/my-app-workspace/my-app-db/src/lib.rs",
            sourceLine: 42,
            doc: "Products shown in the dynamic Dibs admin UI",
            icon: "package"
        )
    ])
}

func sampleDibsGetRequest() -> DibsGetRequest {
    DibsGetRequest(table: "products", pk: .i64(1))
}

func sampleDibsCreateRequest() -> DibsCreateRequest {
    DibsCreateRequest(
        table: "products",
        data: DibsRow(fields: [
            DibsRowField(name: "name", value: .string("new adapter")),
            DibsRowField(name: "active", value: .bool(true)),
        ])
    )
}

func sampleDibsCreateResponse() -> DibsRow {
    DibsRow(fields: [
        DibsRowField(name: "id", value: .i64(3)),
        DibsRowField(name: "name", value: .string("new adapter")),
        DibsRowField(name: "active", value: .bool(true)),
    ])
}

func sampleDibsUpdateRequest() -> DibsUpdateRequest {
    DibsUpdateRequest(
        table: "products",
        pk: .i64(1),
        data: DibsRow(fields: [
            DibsRowField(name: "active", value: .bool(false)),
            DibsRowField(name: "score", value: .f64(10.0)),
        ])
    )
}

func sampleDibsUpdateResponse() -> DibsRow {
    DibsRow(fields: [
        DibsRowField(name: "id", value: .i64(1)),
        DibsRowField(name: "name", value: .string("phon adapter")),
        DibsRowField(name: "active", value: .bool(false)),
        DibsRowField(name: "score", value: .f64(10.0)),
    ])
}

func sampleDibsDeleteRequest() -> DibsDeleteRequest {
    DibsDeleteRequest(table: "products", pk: .i64(2))
}

func sampleDibsMigrationStatusRequest() -> DibsMigrationStatusRequest {
    DibsMigrationStatusRequest(databaseUrl: "postgres://localhost/dibs_fixture")
}

func sampleDibsMigrationStatus() -> [DibsMigrationInfo] {
    [
        DibsMigrationInfo(
            version: "20240501000000",
            name: "create_users",
            applied: true,
            appliedAt: "2024-05-01T00:00:00Z",
            sourceFile: "migrations/20240501000000_create_users.rs",
            source: "CREATE TABLE users (...)"
        ),
        DibsMigrationInfo(
            version: "20240601000000",
            name: "create_products",
            applied: false,
            appliedAt: nil,
            sourceFile: "migrations/20240601000000_create_products.rs",
            source: "CREATE TABLE products (...)"
        ),
    ]
}

func sampleDibsMigrateRequest() -> DibsMigrateRequest {
    DibsMigrateRequest(
        databaseUrl: "postgres://localhost/dibs_fixture",
        migration: "20240601000000_create_products"
    )
}

func sampleDibsLogs() -> [DibsMigrationLog] {
    let migration = "20240601000000_create_products"
    return [
        DibsMigrationLog(level: .info, message: "checking migrations", migration: nil),
        DibsMigrationLog(level: .debug, message: "running migration", migration: migration),
        DibsMigrationLog(level: .warn, message: "sample warning", migration: migration),
        DibsMigrationLog(level: .info, message: "migration complete", migration: migration),
    ]
}

func sampleDibsMigrateResult() -> DibsMigrateResult {
    DibsMigrateResult(
        totalDefined: 3,
        alreadyApplied: [
            DibsAppliedMigration(
                version: "20240501000000_create_users",
                appliedAt: "2024-05-01T00:00:00Z"
            )
        ],
        applied: [
            DibsRanMigration(version: "20240601000000_create_products", durationMs: 37)
        ],
        setupMs: 5,
        totalTimeMs: 42
    )
}

func sameDibsValue(_ lhs: DibsValue, _ rhs: DibsValue) -> Bool {
    switch (lhs, rhs) {
    case (.null, .null):
        true
    case (.bool(let l), .bool(let r)):
        l == r
    case (.i16(let l), .i16(let r)):
        l == r
    case (.i32(let l), .i32(let r)):
        l == r
    case (.i64(let l), .i64(let r)):
        l == r
    case (.f32(let l), .f32(let r)):
        l == r
    case (.f64(let l), .f64(let r)):
        l == r
    case (.string(let l), .string(let r)):
        l == r
    case (.bytes(let l), .bytes(let r)):
        l == r
    default:
        false
    }
}

func sameDibsRowField(_ lhs: DibsRowField, _ rhs: DibsRowField) -> Bool {
    lhs.name == rhs.name && sameDibsValue(lhs.value, rhs.value)
}

func sameDibsRow(_ lhs: DibsRow, _ rhs: DibsRow) -> Bool {
    lhs.fields.count == rhs.fields.count
        && zip(lhs.fields, rhs.fields).allSatisfy { sameDibsRowField($0, $1) }
}

func sameDibsListResponse(_ lhs: DibsListResponse, _ rhs: DibsListResponse) -> Bool {
    lhs.total == rhs.total && lhs.rows.count == rhs.rows.count
        && zip(lhs.rows, rhs.rows).allSatisfy { sameDibsRow($0, $1) }
}

func sameDibsLogLevel(_ lhs: DibsLogLevel, _ rhs: DibsLogLevel) -> Bool {
    switch (lhs, rhs) {
    case (.debug, .debug), (.info, .info), (.warn, .warn), (.error, .error):
        true
    default:
        false
    }
}

func sameDibsMigrationLog(_ lhs: DibsMigrationLog, _ rhs: DibsMigrationLog) -> Bool {
    sameDibsLogLevel(lhs.level, rhs.level) && lhs.message == rhs.message
        && lhs.migration == rhs.migration
}

func sameDibsLogs(_ lhs: [DibsMigrationLog], _ rhs: [DibsMigrationLog]) -> Bool {
    lhs.count == rhs.count && zip(lhs, rhs).allSatisfy { sameDibsMigrationLog($0, $1) }
}

func sameDibsMigrateResult(_ lhs: DibsMigrateResult, _ rhs: DibsMigrateResult) -> Bool {
    lhs.totalDefined == rhs.totalDefined
        && lhs.setupMs == rhs.setupMs
        && lhs.totalTimeMs == rhs.totalTimeMs
        && lhs.alreadyApplied.count == rhs.alreadyApplied.count
        && zip(lhs.alreadyApplied, rhs.alreadyApplied).allSatisfy { left, right in
            left.version == right.version && left.appliedAt == right.appliedAt
        }
        && lhs.applied.count == rhs.applied.count
        && zip(lhs.applied, rhs.applied).allSatisfy { left, right in
            left.version == right.version && left.durationMs == right.durationMs
        }
}

// MARK: - Server Mode

private func serviceLaneMetadata(_ service: String) -> Metadata {
    var metadata: Metadata = .null
    metadata.metaSet("vox-service", .string(service))
    return metadata
}

private func defaultLaneSettings() -> ConnectionSettings {
    ConnectionSettings(parity: .odd, maxConcurrentRequests: 64, initialChannelCredit: 16)
}

/// In "server" mode, the subject acts as the RPC server (handler).
/// But it CONNECTS TO the test harness (specified by PEER_ADDR).
func runServer() async throws {
    let handler = TestbedService()
    let dispatcher = TestbedDispatcher(handler: handler)
    guard let addr = ProcessInfo.processInfo.environment["PEER_ADDR"] else {
        log("PEER_ADDR not set")
        throw SubjectError.missingEnv
    }

    let acceptConnections = ProcessInfo.processInfo.environment["ACCEPT_CONNECTIONS"] != "0"
    log("server mode: connecting to \(addr), acceptConnections=\(acceptConnections)")

    let connection: Connection
    if addr.hasPrefix("local://") {
        let path = String(addr.dropFirst("local://".count))
        guard !path.isEmpty else {
            log("invalid PEER_ADDR format")
            throw SubjectError.invalidAddr
        }
        let connector = UnixConnector(path: path)
        connection = try await Connection.connect(
            connector,
            onLane: acceptConnections
                ? DefaultLaneAcceptor(dispatcher: dispatcher) : nil
        )
    } else {
        let parts = addr.split(separator: ":")
        guard parts.count == 2, let port = Int(parts[1]) else {
            log("invalid PEER_ADDR format")
            throw SubjectError.invalidAddr
        }
        let host = String(parts[0])
        let connector = TcpConnector(host: host, port: port)
        connection = try await Connection.connect(
            connector,
            onLane: acceptConnections
                ? DefaultLaneAcceptor(dispatcher: dispatcher) : nil
        )
    }

    do {
        // r[impl hosted.subject.lifecycle]
        try await connection.run()
    } catch {
        // r[impl hosted.subject.lifecycle]
        connection.handle.shutdown()
        throw error
    }
    // r[impl hosted.subject.lifecycle]
    connection.handle.shutdown()
}

// MARK: - Client Mode

func runClientScenario(client: TestbedClient, scenario: String) async throws {
    log("running client scenario: \(scenario)")

    switch scenario {
    case "echo":
        let result = try await client.echo(message: "hello from swift")
        log("echo result: \(result)")
    case "reverse":
        let result = try await client.reverse(message: "hello")
        guard result == "olleh" else {
            log("reverse expected olleh, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("reverse OK")
    case "divide_success":
        let result = try await client.divide(dividend: 10, divisor: 3)
        guard case .success(3) = result else {
            log("divide_success expected success(3), got \(result)")
            throw SubjectError.invalidResponse
        }
        log("divide_success OK")
    case "divide_zero":
        let result = try await client.divide(dividend: 10, divisor: 0)
        guard case .failure(.divisionByZero) = result else {
            log("divide_zero expected divisionByZero, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("divide_zero OK")
    case "divide_overflow":
        let result = try await client.divide(dividend: .min, divisor: -1)
        guard case .failure(.overflow) = result else {
            log("divide_overflow expected overflow, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("divide_overflow OK")
    case "sum":
        let (tx, rx): (UnboundTx<Int32>, UnboundRx<Int32>) = channel()

        let sender = Task {
            try await Task.sleep(nanoseconds: 50_000_000)
            try await tx.send(1)
            try await tx.send(2)
            try await tx.send(3)
            try await tx.send(4)
            try await tx.send(5)
            tx.close()
        }

        let result = try await client.sum(numbers: rx)
        _ = try await sender.value
        guard result == 15 else {
            log("sum expected 15, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("sum result: \(result)")
    case "sum_client_to_server":
        let (tx, rx): (UnboundTx<Int32>, UnboundRx<Int32>) = channel()

        let callTask = Task {
            try await client.sum(numbers: rx)
        }
        for n in [1, 2, 3, 4, 5] {
            try await tx.send(Int32(n))
        }
        tx.close()
        let result = try await callTask.value
        guard result == 15 else {
            log("sum_client_to_server expected 15, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("sum_client_to_server OK")
    case "sum_large":
        let (tx, rx): (UnboundTx<Int32>, UnboundRx<Int32>) = channel()

        let n = 100
        let callTask = Task {
            try await client.sumLarge(numbers: rx)
        }
        for i in 0..<n {
            try await tx.send(Int32(i))
        }
        tx.close()
        let result = try await callTask.value
        let expected = Int64(n * (n - 1) / 2)
        guard result == expected else {
            log("sum_large expected \(expected), got \(result)")
            throw SubjectError.invalidResponse
        }
        log("sum_large OK")
    case "generate":
        let (tx, rx): (UnboundTx<Int32>, UnboundRx<Int32>) = channel()

        try await client.generate(count: 5, output: tx)

        var received: [Int32] = []
        for try await n in rx {
            received.append(n)
        }
        guard received == [0, 1, 2, 3, 4] else {
            log("generate expected [0, 1, 2, 3, 4], got \(received)")
            throw SubjectError.invalidResponse
        }
        log("generate result OK: \(received)")
    case "generate_large":
        let (tx, rx): (UnboundTx<Int32>, UnboundRx<Int32>) = channel()

        let count: UInt32 = 100
        async let call: Void = client.generateLarge(count: count, output: tx)
        async let received: [Int32] = {
            var values: [Int32] = []
            for try await n in rx {
                values.append(n)
            }
            return values
        }()
        let (_, receivedValues) = try await (call, received)
        let expected = (0..<Int32(count)).map { $0 }
        guard receivedValues == expected else {
            log("generate_large expected \(expected.count) ordered items, got \(receivedValues)")
            throw SubjectError.invalidResponse
        }
        log("generate_large OK")
    case "divide_error":
        let result = try await client.divide(dividend: 10, divisor: 0)
        guard case .failure(.divisionByZero) = result else {
            log("divide_error expected division_by_zero")
            throw SubjectError.invalidResponse
        }
        log("divide_error result OK")
    case "lookup_found":
        let result = try await client.lookup(id: 1)
        guard case .success(let person) = result, person.name == "Alice" else {
            log("lookup_found expected Alice, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("lookup_found OK")
    case "lookup_found_no_email":
        let result = try await client.lookup(id: 2)
        guard case .success(let person) = result, person.name == "Bob", person.email == nil else {
            log("lookup_found_no_email expected Bob with nil email, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("lookup_found_no_email OK")
    case "lookup_not_found":
        let result = try await client.lookup(id: 999)
        guard case .failure(.notFound) = result else {
            log("lookup_not_found expected notFound, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("lookup_not_found OK")
    case "lookup_access_denied":
        let result = try await client.lookup(id: 100)
        guard case .failure(.accessDenied) = result else {
            log("lookup_access_denied expected accessDenied, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("lookup_access_denied OK")
    case "echo_point":
        let point = Point(x: 42, y: -7)
        let result = try await client.echoPoint(point: point)
        guard result.x == point.x, result.y == point.y else {
            log("echo_point expected \(point), got \(result)")
            throw SubjectError.invalidResponse
        }
        log("echo_point OK")
    case "create_person":
        let dave = try await client.createPerson(name: "Dave", age: 40, email: "dave@example.com")
        guard dave.name == "Dave", dave.age == 40, dave.email == "dave@example.com" else {
            log("create_person expected Dave, got \(dave)")
            throw SubjectError.invalidResponse
        }
        let eve = try await client.createPerson(name: "Eve", age: 25, email: nil)
        guard eve.name == "Eve", eve.age == 25, eve.email == nil else {
            log("create_person expected Eve with nil email, got \(eve)")
            throw SubjectError.invalidResponse
        }
        log("create_person OK")
    case "rectangle_area":
        let rect = Rectangle(
            topLeft: Point(x: 0, y: 10),
            bottomRight: Point(x: 5, y: 0),
            label: nil
        )
        let result = try await client.rectangleArea(rect: rect)
        guard abs(result - 50.0) < 1e-9 else {
            log("rectangle_area expected 50.0, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("rectangle_area OK")
    case "parse_color":
        guard try await client.parseColor(name: "red") == .red else {
            log("parse_color red failed")
            throw SubjectError.invalidResponse
        }
        guard try await client.parseColor(name: "green") == .green else {
            log("parse_color green failed")
            throw SubjectError.invalidResponse
        }
        guard try await client.parseColor(name: "blue") == .blue else {
            log("parse_color blue failed")
            throw SubjectError.invalidResponse
        }
        guard try await client.parseColor(name: "purple") == nil else {
            log("parse_color purple expected nil")
            throw SubjectError.invalidResponse
        }
        log("parse_color OK")
    case "get_points":
        let result = try await client.getPoints(count: 5)
        guard result.count == 5, result.first?.x == 0, result.last?.x == 4 else {
            log("get_points expected 5 points from 0..4, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("get_points OK")
    case "swap_pair":
        let result = try await client.swapPair(pair: (99, "hello"))
        guard result.0 == "hello", result.1 == 99 else {
            log("swap_pair expected (hello, 99), got \(result)")
            throw SubjectError.invalidResponse
        }
        log("swap_pair OK")
    case "echo_bytes":
        let data = Data([1, 2, 3, 255, 0, 128])
        let result = try await client.echoBytes(data: data)
        guard result == data else {
            log("echo_bytes mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_bytes OK")
    case "echo_bool":
        guard try await client.echoBool(b: true) == true else {
            log("echo_bool true failed")
            throw SubjectError.invalidResponse
        }
        guard try await client.echoBool(b: false) == false else {
            log("echo_bool false failed")
            throw SubjectError.invalidResponse
        }
        log("echo_bool OK")
    case "echo_u64":
        for n: UInt64 in [0, 1, 1_000_000_000_000, .max] {
            let result = try await client.echoU64(n: n)
            guard result == n else {
                log("echo_u64 expected \(n), got \(result)")
                throw SubjectError.invalidResponse
            }
        }
        log("echo_u64 OK")
    case "echo_option_string":
        let stringResult = try await client.echoOptionString(s: "hello")
        guard stringResult == "hello" else {
            log("echo_option_string Some failed: \(String(describing: stringResult))")
            throw SubjectError.invalidResponse
        }
        let nilResult = try await client.echoOptionString(s: nil)
        guard nilResult == nil else {
            log("echo_option_string None failed: \(String(describing: nilResult))")
            throw SubjectError.invalidResponse
        }
        log("echo_option_string OK")
    case "describe_point":
        let first = try await client.describePoint(label: "origin", x: 0, y: 0, active: true)
        guard first.label == "origin", first.x == 0, first.y == 0, first.active else {
            log("describe_point origin failed: \(first)")
            throw SubjectError.invalidResponse
        }
        let second = try await client.describePoint(label: "far", x: -100, y: 200, active: false)
        guard second.label == "far", second.x == -100, second.y == 200, second.active == false
        else {
            log("describe_point far failed: \(second)")
            throw SubjectError.invalidResponse
        }
        log("describe_point OK")
    case "all_colors":
        let result = try await client.allColors()
        guard result == [.red, .green, .blue] else {
            log("all_colors expected [.red, .green, .blue], got \(result)")
            throw SubjectError.invalidResponse
        }
        log("all_colors OK")
    case "shape_area":
        let result = try await client.shapeArea(shape: .rectangle(width: 3.0, height: 4.0))
        guard result == 12.0 else {
            log("shape_area expected 12.0, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("shape_area result: \(result)")
    case "echo_shape":
        let shapes: [Shape] = [
            .point,
            .circle(radius: 3.14),
            .rectangle(width: 2.0, height: 5.0),
        ]
        for shape in shapes {
            let result = try await client.echoShape(shape: shape)
            guard sameShape(result, shape) else {
                log("echo_shape expected \(shape), got \(result)")
                throw SubjectError.invalidResponse
            }
        }
        log("echo_shape OK")
    case "echo_tree":
        let tree = Tree(
            value: 1,
            children: [
                Tree(value: 2, children: []),
                Tree(value: 3, children: [Tree(value: 4, children: [])]),
            ])
        let result = try await client.echoTree(tree: tree)
        guard sameTree(result, tree) else {
            log("echo_tree expected \(tree), got \(result)")
            throw SubjectError.invalidResponse
        }
        log("echo_tree OK")
    case "echo_ecosystem_bridge":
        let payload = sampleEcosystemBridgePayload()
        let result = try await client.echoEcosystemBridge(payload: payload)
        guard sameEcosystemBridgePayload(result, payload) else {
            log("echo_ecosystem_bridge payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_ecosystem_bridge OK")
    case "echo_dodeca_template_call":
        let payload = sampleDodecaTemplateCall()
        let result = try await client.echoDodecaTemplateCall(call: payload)
        guard sameDodecaTemplateCall(result, payload) else {
            log("echo_dodeca_template_call payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_dodeca_template_call OK")
    case "dodeca_html_process":
        let expected = sampleDodecaHtmlProcessResult()
        let result = try await client.dodecaHtmlProcess(input: sampleDodecaHtmlProcessInput())
        guard sameDodecaHtmlProcessResult(result, expected) else {
            log("dodeca_html_process payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("dodeca_html_process OK")
    case "dodeca_execute_code_samples":
        let expected = sampleDodecaCodeExecutionResult()
        let result = try await client.dodecaExecuteCodeSamples(
            input: sampleDodecaExecuteSamplesInput()
        )
        guard sameDodecaCodeExecutionResult(result, expected) else {
            log("dodeca_execute_code_samples payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("dodeca_execute_code_samples OK")
    case "dodeca_load_data":
        let expected = sampleDodecaLoadDataResult()
        let result = try await client.dodecaLoadData(
            content: sampleDodecaDataContent(),
            format: sampleDodecaDataFormat()
        )
        guard sameDodecaLoadDataResult(result, expected) else {
            log("dodeca_load_data payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("dodeca_load_data OK")
    case "dodeca_parse_and_render":
        let expected = sampleDodecaParseResult()
        let result = try await client.dodecaParseAndRender(
            sourcePath: sampleDodecaMarkdownSourcePath(),
            content: sampleDodecaMarkdownContent(),
            sourceMap: true
        )
        guard sameDodecaParseResult(result, expected) else {
            log("dodeca_parse_and_render payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("dodeca_parse_and_render OK")
    case "echo_dodeca_image_processor_fixture":
        let payload = sampleDodecaImageProcessorFixture()
        let result = try await client.echoDodecaImageProcessorFixture(fixture: payload)
        guard sameReflecting(result, payload) else {
            log("echo_dodeca_image_processor_fixture payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_dodeca_image_processor_fixture OK")
    case "echo_dodeca_search_indexer_fixture":
        let payload = sampleDodecaSearchIndexerFixture()
        let result = try await client.echoDodecaSearchIndexerFixture(fixture: payload)
        guard sameReflecting(result, payload) else {
            log("echo_dodeca_search_indexer_fixture payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_dodeca_search_indexer_fixture OK")
    case "echo_dodeca_asset_processing_fixture":
        let payload = sampleDodecaAssetProcessingFixture()
        let result = try await client.echoDodecaAssetProcessingFixture(fixture: payload)
        guard sameDodecaAssetProcessingFixture(result, payload) else {
            log("echo_dodeca_asset_processing_fixture payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_dodeca_asset_processing_fixture OK")

    case "echo_dodeca_small_cell_services_fixture":
        let payload = sampleDodecaSmallCellServicesFixture()
        let result = try await client.echoDodecaSmallCellServicesFixture(fixture: payload)
        guard sameDodecaSmallCellServicesFixture(result, payload) else {
            log("echo_dodeca_small_cell_services_fixture payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_dodeca_small_cell_services_fixture OK")
    case "echo_dodeca_devtools_event":
        let payload = sampleDodecaDevtoolsEvent()
        let result = try await client.echoDodecaDevtoolsEvent(event: payload)
        guard sameReflecting(result, payload) else {
            log("echo_dodeca_devtools_event payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_dodeca_devtools_event OK")
    case "dodeca_devtools_get_scope":
        let expected = sampleDodecaScopeEntries()
        let result = try await client.dodecaDevtoolsGetScope(path: ["page"])
        guard sameReflecting(result, expected) else {
            log("dodeca_devtools_get_scope payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("dodeca_devtools_get_scope OK")
    case "dodeca_devtools_eval":
        let expected = sampleDodecaEvalResult()
        let result = try await client.dodecaDevtoolsEval(
            snapshotId: "snap-devtools-42",
            expression: "page.title"
        )
        guard sameReflecting(result, expected) else {
            log("dodeca_devtools_eval payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("dodeca_devtools_eval OK")
    case "dodeca_devtools_open_dead_link":
        let expected = sampleDodecaOpenSourceResult()
        let result = try await client.dodecaDevtoolsOpenDeadLink(
            route: "/guide/",
            target: sampleDodecaDeadLinkTarget()
        )
        guard sameReflecting(result, expected) else {
            log("dodeca_devtools_open_dead_link payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("dodeca_devtools_open_dead_link OK")
    case "dodeca_devtools_edit_load":
        let expected = sampleDodecaEditLoad()
        let result = try await client.dodecaDevtoolsEditLoad(
            token: "editor-token",
            route: "/guide/"
        )
        guard sameReflecting(result, expected) else {
            log("dodeca_devtools_edit_load payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("dodeca_devtools_edit_load OK")
    case "dodeca_devtools_edit_preview":
        let expected = sampleDodecaEditPreview()
        let result = try await client.dodecaDevtoolsEditPreview(
            token: "editor-token",
            sourceKey: "content/guide.md",
            buffer: "# Guide\n\nUpdated from browser."
        )
        guard sameReflecting(result, expected) else {
            log("dodeca_devtools_edit_preview payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("dodeca_devtools_edit_preview OK")
    case "dodeca_devtools_edit_save":
        let expected = sampleDodecaEditSave()
        let result = try await client.dodecaDevtoolsEditSave(
            token: "editor-token",
            req: sampleDodecaEditSaveReq()
        )
        guard sameReflecting(result, expected) else {
            log("dodeca_devtools_edit_save payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("dodeca_devtools_edit_save OK")
    case "dodeca_devtools_edit_upload":
        let expected = sampleDodecaEditUpload()
        let result = try await client.dodecaDevtoolsEditUpload(
            token: "editor-token",
            req: sampleDodecaEditUploadReq()
        )
        guard sameReflecting(result, expected) else {
            log("dodeca_devtools_edit_upload payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("dodeca_devtools_edit_upload OK")
    case "dodeca_devtools_edit_read":
        let expected = sampleDodecaEditRead()
        let result = try await client.dodecaDevtoolsEditRead(
            token: "editor-token",
            uri: "file:///workspace/content/guide.md"
        )
        guard sameReflecting(result, expected) else {
            log("dodeca_devtools_edit_read payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("dodeca_devtools_edit_read OK")
    case "dodeca_devtools_edit_list":
        let expected = sampleDodecaEditList()
        let result = try await client.dodecaDevtoolsEditList(token: "editor-token")
        guard sameReflecting(result, expected) else {
            log("dodeca_devtools_edit_list payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("dodeca_devtools_edit_list OK")
    case "echo_styx_value":
        let value = sampleStyxValue()
        let result = try await client.echoStyxValue(value: value)
        guard sameStyxValue(result, value) else {
            log("echo_styx_value payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_styx_value OK")
    case "styx_lsp_initialize":
        let expected = sampleStyxLspInitializeResult()
        let result = try await client.styxLspInitialize(params: sampleStyxLspInitializeParams())
        guard sameReflecting(result, expected) else {
            log("styx_lsp_initialize payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("styx_lsp_initialize OK")
    case "styx_lsp_completions":
        let expected = sampleStyxLspCompletions()
        let result = try await client.styxLspCompletions(params: sampleStyxLspCompletionParams())
        guard sameReflecting(result, expected) else {
            log("styx_lsp_completions payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("styx_lsp_completions OK")
    case "styx_lsp_hover":
        let expected = sampleStyxLspHoverResult()
        let result = try await client.styxLspHover(params: sampleStyxLspHoverParams())
        guard let result = result, sameReflecting(result, expected) else {
            log("styx_lsp_hover payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("styx_lsp_hover OK")
    case "styx_lsp_inlay_hints":
        let expected = sampleStyxLspInlayHints()
        let result = try await client.styxLspInlayHints(params: sampleStyxLspInlayHintParams())
        guard sameReflecting(result, expected) else {
            log("styx_lsp_inlay_hints payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("styx_lsp_inlay_hints OK")
    case "styx_lsp_diagnostics":
        let expected = sampleStyxLspDiagnostics()
        let result = try await client.styxLspDiagnostics(params: sampleStyxLspDiagnosticParams())
        guard sameReflecting(result, expected) else {
            log("styx_lsp_diagnostics payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("styx_lsp_diagnostics OK")
    case "styx_lsp_code_actions":
        let expected = sampleStyxLspCodeActions()
        let result = try await client.styxLspCodeActions(params: sampleStyxLspCodeActionParams())
        guard sameReflecting(result, expected) else {
            log("styx_lsp_code_actions payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("styx_lsp_code_actions OK")
    case "styx_lsp_definition":
        let expected = sampleStyxLspLocations()
        let result = try await client.styxLspDefinition(params: sampleStyxLspDefinitionParams())
        guard sameReflecting(result, expected) else {
            log("styx_lsp_definition payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("styx_lsp_definition OK")
    case "styx_lsp_shutdown":
        try await client.styxLspShutdown()
        log("styx_lsp_shutdown OK")
    case "styx_host_get_subtree":
        let result = try await client.styxHostGetSubtree(params: sampleStyxLspGetSubtreeParams())
        guard let result = result, sameStyxValue(result, sampleStyxValue()) else {
            log("styx_host_get_subtree payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("styx_host_get_subtree OK")
    case "styx_host_get_document":
        let result = try await client.styxHostGetDocument(params: sampleStyxLspGetDocumentParams())
        guard let result = result, sameStyxValue(result, sampleStyxValue()) else {
            log("styx_host_get_document payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("styx_host_get_document OK")
    case "styx_host_get_source":
        let result = try await client.styxHostGetSource(params: sampleStyxLspGetSourceParams())
        guard result == sampleStyxLspSource() else {
            log("styx_host_get_source payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("styx_host_get_source OK")
    case "styx_host_get_schema":
        let expected = sampleStyxLspSchemaInfo()
        let result = try await client.styxHostGetSchema(params: sampleStyxLspGetSchemaParams())
        guard let result = result, sameReflecting(result, expected) else {
            log("styx_host_get_schema payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("styx_host_get_schema OK")
    case "styx_host_offset_to_position":
        let expected = sampleStyxLspPosition()
        let result = try await client.styxHostOffsetToPosition(
            params: sampleStyxLspOffsetToPositionParams()
        )
        guard let result = result, sameReflecting(result, expected) else {
            log("styx_host_offset_to_position payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("styx_host_offset_to_position OK")
    case "styx_host_position_to_offset":
        let result = try await client.styxHostPositionToOffset(
            params: sampleStyxLspPositionToOffsetParams()
        )
        guard result == 16 else {
            log("styx_host_position_to_offset payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("styx_host_position_to_offset OK")
    case "stax_flamegraph":
        let params = sampleStaxViewParams()
        let expected = sampleStaxFlamegraphUpdate(params)
        let result = try await client.staxFlamegraph(params: params)
        guard sameStaxFlamegraphUpdate(result, expected) else {
            log("stax_flamegraph payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("stax_flamegraph OK")
    case "echo_stax_flamegraph_update":
        let params = sampleStaxViewParams()
        let update = sampleStaxFlamegraphUpdate(params)
        let result = try await client.echoStaxFlamegraphUpdate(update: update)
        guard sameStaxFlamegraphUpdate(result, update) else {
            log("echo_stax_flamegraph_update payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_stax_flamegraph_update OK")
    case "stax_subscribe_flamegraph_updates":
        let (updateTx, updateRx): (UnboundTx<StaxFlamegraphUpdate>, UnboundRx<StaxFlamegraphUpdate>) =
            channel()
        async let result: Void = client.staxSubscribeFlamegraphUpdates(output: updateTx)
        async let updates: [StaxFlamegraphUpdate] = {
            var values: [StaxFlamegraphUpdate] = []
            for try await update in updateRx {
                values.append(update)
            }
            return values
        }()
        let (_, receivedUpdates) = try await (result, updates)
        let expectedUpdates = sampleStaxFlamegraphUpdates()
        guard receivedUpdates.count == expectedUpdates.count
            && zip(receivedUpdates, expectedUpdates).allSatisfy({ sameStaxFlamegraphUpdate($0, $1) })
        else {
            log("stax_subscribe_flamegraph_updates payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("stax_subscribe_flamegraph_updates OK")
    case "echo_stax_linux_broker_control":
        let fixture = sampleStaxLinuxBrokerControlFixture()
        let result = try await client.echoStaxLinuxBrokerControl(fixture: fixture)
        guard sameReflecting(result, fixture) else {
            log("echo_stax_linux_broker_control payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_stax_linux_broker_control OK")
    case "stax_macos_record":
        let (batchTx, batchRx): (UnboundTx<StaxMacKdBufBatch>, UnboundRx<StaxMacKdBufBatch>) =
            channel()
        async let result = client.staxMacosRecord(
            config: sampleStaxMacosConfig(),
            records: batchTx
        )
        async let batches: [StaxMacKdBufBatch] = {
            var values: [StaxMacKdBufBatch] = []
            for try await batch in batchRx {
                values.append(batch)
            }
            return values
        }()
        let (recordResult, receivedBatches) = try await (result, batches)
        guard case .success(let summary) = recordResult,
            sameReflecting(summary, sampleStaxMacosRecordSummary())
        else {
            log("stax_macos_record summary mismatch")
            throw SubjectError.invalidResponse
        }
        guard sameStaxMacBatches(receivedBatches, sampleStaxMacosBatches()) else {
            log("stax_macos_record batches mismatch")
            throw SubjectError.invalidResponse
        }
        log("stax_macos_record OK")
    case "echo_hotmeal_live_reload_event":
        for event in sampleHotmealLiveReloadEvents() {
            let result = try await client.echoHotmealLiveReloadEvent(event: event)
            guard sameHotmealLiveReloadEvent(result, event) else {
                log("echo_hotmeal_live_reload_event payload mismatch")
                throw SubjectError.invalidResponse
            }
        }
        log("echo_hotmeal_live_reload_event OK")
    case "echo_hotmeal_apply_patches_result":
        let payload = sampleHotmealApplyPatchesResult()
        let result = try await client.echoHotmealApplyPatchesResult(result: payload)
        guard sameHotmealApplyPatchesResult(result, payload) else {
            log("echo_hotmeal_apply_patches_result payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_hotmeal_apply_patches_result OK")
    case "hotmeal_live_reload_subscribe":
        try await client.hotmealLiveReloadSubscribe(route: sampleHotmealRoute())
        log("hotmeal_live_reload_subscribe OK")
    case "hotmeal_live_reload_on_event":
        for event in sampleHotmealLiveReloadEvents() {
            try await client.hotmealLiveReloadOnEvent(event: event)
        }
        log("hotmeal_live_reload_on_event OK")
    case "echo_helix_stream_metrics":
        let metrics = sampleHelixStreamMetrics()
        let result = try await client.echoHelixStreamMetrics(metrics: metrics)
        guard sameHelixStreamMetrics(result, metrics) else {
            log("echo_helix_stream_metrics payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_helix_stream_metrics OK")
    case "echo_helix_verify_evidence":
        let digest = sampleHelixVerifyEvidence()
        let result = try await client.echoHelixVerifyEvidence(digest: digest)
        guard sameHelixVerifyEvidenceDigest(result, digest) else {
            log("echo_helix_verify_evidence payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_helix_verify_evidence OK")
    case "helix_subscribe_pulses":
        let (pulseTx, pulseRx): (UnboundTx<HelixPulseAvailable>, UnboundRx<HelixPulseAvailable>) =
            channel()
        async let result: Void = client.helixSubscribePulses(output: pulseTx)
        async let pulses: [HelixPulseAvailable] = {
            var values: [HelixPulseAvailable] = []
            for try await pulse in pulseRx {
                values.append(pulse)
            }
            return values
        }()
        let (_, receivedPulses) = try await (result, pulses)
        guard sameHelixPulses(receivedPulses, sampleHelixPulses()) else {
            log("helix_subscribe_pulses mismatch: \(receivedPulses)")
            throw SubjectError.invalidResponse
        }
        log("helix_subscribe_pulses OK")
    case "helix_pulse_bundle":
        let expected = sampleHelixPulseBundle()
        let result = try await client.helixPulseBundle(
            pulseId: 102,
            fields: sampleHelixPulseBundleFields()
        )
        guard sameHelixPulseBundle(result, expected) else {
            log("helix_pulse_bundle payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("helix_pulse_bundle OK")
    case "helix_trace_service_surface":
        let expected = sampleHelixTraceServiceSurface()
        let result = try await client.helixTraceServiceSurface()
        guard sameHelixTraceServiceSurface(result, expected) else {
            log("helix_trace_service_surface payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("helix_trace_service_surface OK")
    case "tracey_status":
        let status = try await client.traceyStatus()
        guard sameTraceyStatusResponse(status, sampleTraceyStatusResponse()) else {
            log("tracey_status payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("tracey_status OK")
    case "tracey_core_control":
        let uncovered = try await client.traceyUncovered(req: sampleTraceyQueryRequest())
        guard sameTraceyUncoveredResponse(uncovered, sampleTraceyUncoveredResponse()) else {
            log("tracey_uncovered payload mismatch")
            throw SubjectError.invalidResponse
        }
        let untested = try await client.traceyUntested(req: sampleTraceyUntestedRequest())
        guard sameTraceyUntestedResponse(untested, sampleTraceyUntestedResponse()) else {
            log("tracey_untested payload mismatch")
            throw SubjectError.invalidResponse
        }
        let stale = try await client.traceyStale(req: sampleTraceyStaleRequest())
        guard sameTraceyStaleResponse(stale, sampleTraceyStaleResponse()) else {
            log("tracey_stale payload mismatch")
            throw SubjectError.invalidResponse
        }
        let unmapped = try await client.traceyUnmapped(req: sampleTraceyUnmappedRequest())
        guard sameTraceyUnmappedResponse(unmapped, sampleTraceyUnmappedResponse()) else {
            log("tracey_unmapped payload mismatch")
            throw SubjectError.invalidResponse
        }
        let config = try await client.traceyConfig()
        guard sameTraceyApiConfig(config, sampleTraceyApiConfig()) else {
            log("tracey_config payload mismatch")
            throw SubjectError.invalidResponse
        }
        try await client.traceyVfsOpen(path: "src/lib.rs", content: sampleTraceyLspContent())
        try await client.traceyVfsChange(
            path: "src/lib.rs",
            content: "// r[verify rpc.channel.direct-args]\n"
        )
        try await client.traceyVfsClose(path: "src/lib.rs")
        let reload = try await client.traceyReload()
        guard sameTraceyReloadResponse(reload, sampleTraceyReloadResponse()) else {
            log("tracey_reload payload mismatch")
            throw SubjectError.invalidResponse
        }
        let version = try await client.traceyVersion()
        guard version == 13 else {
            log("tracey_version payload mismatch")
            throw SubjectError.invalidResponse
        }
        let health = try await client.traceyHealth()
        guard sameTraceyHealthResponse(health, sampleTraceyHealthResponse()) else {
            log("tracey_health payload mismatch")
            throw SubjectError.invalidResponse
        }
        try await client.traceyShutdown()
        log("tracey_core_control OK")
    case "tracey_rule":
        let rule = try await client.traceyRule(ruleId: traceyRuleId("rpc.channel.direct-args", 1))
        guard sameTraceyRuleInfo(rule, sampleTraceyRuleInfo()) else {
            log("tracey_rule known payload mismatch")
            throw SubjectError.invalidResponse
        }
        let missing = try await client.traceyRule(ruleId: traceyRuleId("missing.rule", 1))
        guard missing == nil else {
            log("tracey_rule missing expected nil")
            throw SubjectError.invalidResponse
        }
        log("tracey_rule OK")
    case "tracey_dashboard":
        let forward = try await client.traceyForward(spec: "vox", implName: "rust")
        guard sameTraceyDashboardValue(forward, Optional(sampleTraceyForwardResponse())) else {
            log("tracey_forward payload mismatch")
            throw SubjectError.invalidResponse
        }
        let missingForward = try await client.traceyForward(spec: "missing", implName: "rust")
        guard missingForward == nil else {
            log("tracey_forward missing expected nil")
            throw SubjectError.invalidResponse
        }

        let reverse = try await client.traceyReverse(spec: "vox", implName: "rust")
        guard sameTraceyDashboardValue(reverse, Optional(sampleTraceyReverseResponse())) else {
            log("tracey_reverse payload mismatch")
            throw SubjectError.invalidResponse
        }

        let file = try await client.traceyFile(req: sampleTraceyFileRequest())
        guard sameTraceyDashboardValue(file, Optional(sampleTraceyFileResponse())) else {
            log("tracey_file payload mismatch")
            throw SubjectError.invalidResponse
        }

        let specContent = try await client.traceySpecContent(spec: "vox", implName: "rust")
        guard sameTraceyDashboardValue(specContent, Optional(sampleTraceySpecContentResponse())) else {
            log("tracey_spec_content payload mismatch")
            throw SubjectError.invalidResponse
        }

        let search = try await client.traceySearch(query: "channel", limit: 10)
        guard sameTraceyDashboardValue(search, sampleTraceySearchResults()) else {
            log("tracey_search payload mismatch")
            throw SubjectError.invalidResponse
        }

        let updateOk = try await client.traceyUpdateFileRange(
            req: sampleTraceyUpdateFileRangeRequest())
        guard case .success = updateOk else {
            log("tracey_update_file_range expected success")
            throw SubjectError.invalidResponse
        }
        let updateConflict = try await client.traceyUpdateFileRange(
            req: sampleTraceyUpdateFileRangeConflictRequest())
        switch updateConflict {
        case .failure(let error) where sameTraceyDashboardValue(error, sampleTraceyUpdateError()):
            break
        default:
            log("tracey_update_file_range expected user error")
            throw SubjectError.invalidResponse
        }

        let excludeOk = try await client.traceyConfigAddExclude(
            req: sampleTraceyConfigPatternRequest())
        guard case .success = excludeOk else {
            log("tracey_config_add_exclude expected success")
            throw SubjectError.invalidResponse
        }
        let excludeBad = try await client.traceyConfigAddExclude(
            req: sampleTraceyBadConfigPatternRequest())
        switch excludeBad {
        case .failure(let error) where error == "invalid pattern":
            break
        default:
            log("tracey_config_add_exclude expected user error")
            throw SubjectError.invalidResponse
        }
        let includeOk = try await client.traceyConfigAddInclude(
            req: sampleTraceyConfigPatternRequest())
        guard case .success = includeOk else {
            log("tracey_config_add_include expected success")
            throw SubjectError.invalidResponse
        }
        log("tracey_dashboard OK")
    case "tracey_validate":
        let result = try await client.traceyValidate(req: sampleTraceyValidateRequest())
        guard sameTraceyValidationResult(result, sampleTraceyValidationResult()) else {
            log("tracey_validate payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("tracey_validate OK")
    case "tracey_lsp_surface":
        let testFile = try await client.traceyIsTestFile(
            path: "spec/spec-tests/tests/cases/testbed.rs")
        guard testFile else {
            log("tracey_is_test_file expected true for tests path")
            throw SubjectError.invalidResponse
        }
        let sourceFile = try await client.traceyIsTestFile(path: "src/lib.rs")
        guard !sourceFile else {
            log("tracey_is_test_file expected false for source path")
            throw SubjectError.invalidResponse
        }

        let hover = try await client.traceyLspHover(req: sampleTraceyLspPositionRequest())
        guard sameTraceyHoverInfo(hover, sampleTraceyHoverInfo()) else {
            log("tracey_lsp_hover payload mismatch")
            throw SubjectError.invalidResponse
        }

        let definition = try await client.traceyLspDefinition(req: sampleTraceyLspPositionRequest())
        guard sameTraceyLspLocations(definition, sampleTraceyLspLocations()) else {
            log("tracey_lsp_definition payload mismatch")
            throw SubjectError.invalidResponse
        }

        let implementation = try await client.traceyLspImplementation(
            req: sampleTraceyLspPositionRequest())
        guard sameTraceyLspLocations(implementation, sampleTraceyLspLocations()) else {
            log("tracey_lsp_implementation payload mismatch")
            throw SubjectError.invalidResponse
        }

        let references = try await client.traceyLspReferences(
            req: sampleTraceyLspReferencesRequest())
        guard sameTraceyLspLocations(references, sampleTraceyLspLocations()) else {
            log("tracey_lsp_references payload mismatch")
            throw SubjectError.invalidResponse
        }

        let completions = try await client.traceyLspCompletions(
            req: sampleTraceyLspPositionRequest())
        guard sameTraceyLspCompletions(completions, sampleTraceyLspCompletions()) else {
            log("tracey_lsp_completions payload mismatch")
            throw SubjectError.invalidResponse
        }

        let documentSymbols = try await client.traceyLspDocumentSymbols(
            req: sampleTraceyLspDocumentRequest())
        guard sameTraceyLspSymbols(documentSymbols, sampleTraceyLspSymbols()) else {
            log("tracey_lsp_document_symbols payload mismatch")
            throw SubjectError.invalidResponse
        }

        let workspaceSymbols = try await client.traceyLspWorkspaceSymbols(query: "rpc.channel")
        guard sameTraceyLspSymbols(workspaceSymbols, sampleTraceyLspSymbols()) else {
            log("tracey_lsp_workspace_symbols payload mismatch")
            throw SubjectError.invalidResponse
        }

        let semanticTokens = try await client.traceyLspSemanticTokens(
            req: sampleTraceyLspDocumentRequest())
        guard sameTraceyLspSemanticTokens(semanticTokens, sampleTraceyLspSemanticTokens()) else {
            log("tracey_lsp_semantic_tokens payload mismatch")
            throw SubjectError.invalidResponse
        }

        let codeLens = try await client.traceyLspCodeLens(req: sampleTraceyLspDocumentRequest())
        guard sameTraceyLspCodeLens(codeLens, sampleTraceyLspCodeLens()) else {
            log("tracey_lsp_code_lens payload mismatch")
            throw SubjectError.invalidResponse
        }

        let inlayHints = try await client.traceyLspInlayHints(
            req: sampleTraceyLspInlayHintsRequest())
        guard sameTraceyLspInlayHints(inlayHints, sampleTraceyLspInlayHints()) else {
            log("tracey_lsp_inlay_hints payload mismatch")
            throw SubjectError.invalidResponse
        }

        let prepareRename = try await client.traceyLspPrepareRename(
            req: sampleTraceyLspPositionRequest())
        guard sameTraceyPrepareRenameResult(prepareRename, sampleTraceyPrepareRenameResult()) else {
            log("tracey_lsp_prepare_rename payload mismatch")
            throw SubjectError.invalidResponse
        }

        let textEdits = try await client.traceyLspRename(req: sampleTraceyLspRenameRequest())
        guard sameTraceyLspTextEdits(textEdits, sampleTraceyLspTextEdits()) else {
            log("tracey_lsp_rename payload mismatch")
            throw SubjectError.invalidResponse
        }

        let codeActions = try await client.traceyLspCodeActions(
            req: sampleTraceyLspPositionRequest())
        guard sameTraceyLspCodeActions(codeActions, sampleTraceyLspCodeActions()) else {
            log("tracey_lsp_code_actions payload mismatch")
            throw SubjectError.invalidResponse
        }

        let highlights = try await client.traceyLspDocumentHighlight(
            req: sampleTraceyLspPositionRequest())
        guard sameTraceyLspLocations(highlights, sampleTraceyLspLocations()) else {
            log("tracey_lsp_document_highlight payload mismatch")
            throw SubjectError.invalidResponse
        }

        log("tracey_lsp_surface OK")
    case "tracey_lsp_workspace_diagnostics":
        let result = try await client.traceyLspWorkspaceDiagnostics()
        guard sameTraceyLspWorkspaceDiagnostics(result, sampleTraceyLspWorkspaceDiagnostics()) else {
            log("tracey_lsp_workspace_diagnostics payload mismatch")
            throw SubjectError.invalidResponse
        }
        log("tracey_lsp_workspace_diagnostics OK")
    case "tracey_subscribe_updates":
        let (updateTx, updateRx): (UnboundTx<TraceyDataUpdate>, UnboundRx<TraceyDataUpdate>) =
            channel()
        async let result: Void = client.traceySubscribeUpdates(updates: updateTx)
        async let updates: [TraceyDataUpdate] = {
            var values: [TraceyDataUpdate] = []
            for try await update in updateRx {
                values.append(update)
            }
            return values
        }()
        let (_, receivedUpdates) = try await (result, updates)
        guard sameTraceyUpdates(receivedUpdates, sampleTraceyUpdates()) else {
            log("tracey_subscribe_updates mismatch: \(receivedUpdates)")
            throw SubjectError.invalidResponse
        }
        log("tracey_subscribe_updates OK")
    case "dibs_list":
        let result = try await client.dibsList(request: sampleDibsListRequest())
        guard case .success(let response) = result,
            sameDibsListResponse(response, sampleDibsListResponse())
        else {
            log("dibs_list response mismatch: \(result)")
            throw SubjectError.invalidResponse
        }
        log("dibs_list OK")
    case "dibs_schema":
        let result = try await client.dibsSchema()
        guard sameReflecting(result, sampleDibsSchema()) else {
            log("dibs_schema response mismatch: \(result)")
            throw SubjectError.invalidResponse
        }
        log("dibs_schema OK")
    case "dibs_get":
        let result = try await client.dibsGet(request: sampleDibsGetRequest())
        guard case .success(let response?) = result,
            sameDibsRow(response, sampleDibsRowOne())
        else {
            log("dibs_get response mismatch: \(result)")
            throw SubjectError.invalidResponse
        }
        log("dibs_get OK")
    case "dibs_create":
        let result = try await client.dibsCreate(request: sampleDibsCreateRequest())
        guard case .success(let response) = result,
            sameDibsRow(response, sampleDibsCreateResponse())
        else {
            log("dibs_create response mismatch: \(result)")
            throw SubjectError.invalidResponse
        }
        log("dibs_create OK")
    case "dibs_update":
        let result = try await client.dibsUpdate(request: sampleDibsUpdateRequest())
        guard case .success(let response) = result,
            sameDibsRow(response, sampleDibsUpdateResponse())
        else {
            log("dibs_update response mismatch: \(result)")
            throw SubjectError.invalidResponse
        }
        log("dibs_update OK")
    case "dibs_delete":
        let result = try await client.dibsDelete(request: sampleDibsDeleteRequest())
        guard case .success(1) = result else {
            log("dibs_delete response mismatch: \(result)")
            throw SubjectError.invalidResponse
        }
        log("dibs_delete OK")
    case "dibs_migration_status":
        let result = try await client.dibsMigrationStatus(
            request: sampleDibsMigrationStatusRequest()
        )
        guard case .success(let response) = result,
            sameReflecting(response, sampleDibsMigrationStatus())
        else {
            log("dibs_migration_status response mismatch: \(result)")
            throw SubjectError.invalidResponse
        }
        log("dibs_migration_status OK")
    case "dibs_migrate":
        let (logTx, logRx): (UnboundTx<DibsMigrationLog>, UnboundRx<DibsMigrationLog>) = channel()
        async let result = client.dibsMigrate(request: sampleDibsMigrateRequest(), logs: logTx)
        async let logs: [DibsMigrationLog] = {
            var values: [DibsMigrationLog] = []
            for try await logEntry in logRx {
                values.append(logEntry)
            }
            return values
        }()
        let (migrationResult, receivedLogs) = try await (result, logs)
        guard case .success(let value) = migrationResult,
            sameDibsMigrateResult(value, sampleDibsMigrateResult())
        else {
            log("dibs_migrate result mismatch: \(migrationResult)")
            throw SubjectError.invalidResponse
        }
        guard sameDibsLogs(receivedLogs, sampleDibsLogs()) else {
            log("dibs_migrate logs mismatch: \(receivedLogs)")
            throw SubjectError.invalidResponse
        }
        log("dibs_migrate OK")
    case "create_canvas":
        let result = try await client.createCanvas(
            name: "enum-canvas",
            shapes: [.point, .circle(radius: 2.5)],
            background: .green
        )
        guard result.name == "enum-canvas" else {
            log("create_canvas expected name enum-canvas, got \(result.name)")
            throw SubjectError.invalidResponse
        }
        guard case .green = result.background else {
            log("create_canvas expected green background")
            throw SubjectError.invalidResponse
        }
        guard result.shapes.count == 2 else {
            log("create_canvas expected 2 shapes, got \(result.shapes.count)")
            throw SubjectError.invalidResponse
        }
        guard case .point = result.shapes[0] else {
            log("create_canvas expected first shape to be point")
            throw SubjectError.invalidResponse
        }
        guard case .circle(let radius) = result.shapes[1], radius == 2.5 else {
            log("create_canvas expected second shape to be circle(radius: 2.5)")
            throw SubjectError.invalidResponse
        }
        log("create_canvas result OK")
    case "pipelining":
        try await withThrowingTaskGroup(of: Void.self) { group in
            for i in 0..<10 {
                group.addTask {
                    let expected = "msg\(i)"
                    let result = try await client.echo(message: expected)
                    guard result == expected else {
                        throw SubjectError.invalidResponse
                    }
                }
            }
            try await group.waitForAll()
        }
        log("pipelining OK")
    case "process_message":
        let result = try await client.processMessage(msg: .data(Data([1, 2, 3, 4])))
        guard case .data(let payload) = result, payload == Data([4, 3, 2, 1]) else {
            log("process_message returned unexpected payload")
            throw SubjectError.invalidResponse
        }
        log("process_message result OK")
    case "post_reply_generate":
        let (tx, rx): (UnboundTx<Int32>, UnboundRx<Int32>) = channel()
        try await client.postReplyGenerate(output: tx)
        var received: [Int32] = []
        for try await n in rx {
            received.append(n)
        }
        let expected: [Int32] = [0, 1, 2, 3, 4]
        guard received == expected else {
            log("post_reply_generate expected \(expected), got \(received)")
            throw SubjectError.invalidResponse
        }
        log("post_reply_generate OK")
    case "post_reply_sum":
        let (inputTx, inputRx): (UnboundTx<Int32>, UnboundRx<Int32>) = channel()
        let (resultTx, resultRx): (UnboundTx<Int64>, UnboundRx<Int64>) = channel()
        try await client.postReplySum(input: inputRx, result: resultTx)
        for n in [1, 2, 3, 4, 5] {
            try await inputTx.send(Int32(n))
        }
        inputTx.close()
        var resultIter = resultRx.makeAsyncIterator()
        guard let total = try await resultIter.next() else {
            log("post_reply_sum result channel closed without a value")
            throw SubjectError.invalidResponse
        }
        guard total == 15 else {
            log("post_reply_sum expected 15, got \(total)")
            throw SubjectError.invalidResponse
        }
        if let extra = try await resultIter.next() {
            log("post_reply_sum result channel yielded extra value \(extra)")
            throw SubjectError.invalidResponse
        }
        log("post_reply_sum OK")
    case "transform_bidi":
        let (inputTx, inputRx): (UnboundTx<String>, UnboundRx<String>) = channel()
        let (outputTx, outputRx): (UnboundTx<String>, UnboundRx<String>) = channel()
        let messages = ["alpha", "beta", "gamma"]
        async let call: Void = client.transform(input: inputRx, output: outputTx)
        try await Task.sleep(nanoseconds: 50_000_000)
        async let received: [String] = {
            var values: [String] = []
            for try await s in outputRx {
                values.append(s)
            }
            return values
        }()
        for message in messages {
            try await inputTx.send(message)
        }
        inputTx.close()
        let (_, receivedValues) = try await (call, received)
        guard receivedValues == messages else {
            log("transform_bidi expected \(messages), got \(receivedValues)")
            throw SubjectError.invalidResponse
        }
        log("transform_bidi OK")
    case "dodeca_byte_tunnel":
        let (inboundTx, inboundRx): (UnboundTx<Data>, UnboundRx<Data>) = channel()
        let (outboundTx, outboundRx): (UnboundTx<Data>, UnboundRx<Data>) = channel()
        let chunks = [
            Data([0, 1, 2, 3]),
            Data(),
            Data([255, 254, 253]),
        ]
        async let call: Void = client.dodecaByteTunnel(inbound: inboundRx, outbound: outboundTx)
        try await Task.sleep(nanoseconds: 50_000_000)
        async let received: [Data] = {
            var values: [Data] = []
            for try await chunk in outboundRx {
                values.append(chunk)
            }
            return values
        }()
        for chunk in chunks {
            try await inboundTx.send(chunk)
        }
        inboundTx.close()
        let (_, receivedValues) = try await (call, received)
        guard receivedValues == chunks else {
            log("dodeca_byte_tunnel expected \(chunks), got \(receivedValues)")
            throw SubjectError.invalidResponse
        }
        log("dodeca_byte_tunnel OK")
    case "dodeca_devtools_lsp":
        let (clientTx, clientRx): (UnboundTx<String>, UnboundRx<String>) = channel()
        let (serverTx, serverRx): (UnboundTx<String>, UnboundRx<String>) = channel()
        let chunks = [
            "Content-Length: 37\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":1}",
            "{\"method\":\"textDocument/didOpen\"}",
        ]
        let expected = chunks.map { "lsp:\($0)" }
        async let call: Void = client.dodecaDevtoolsLsp(
            token: "editor-token",
            clientToServer: clientRx,
            serverToClient: serverTx
        )
        try await Task.sleep(nanoseconds: 50_000_000)
        async let received: [String] = {
            var values: [String] = []
            for try await chunk in serverRx {
                values.append(chunk)
            }
            return values
        }()
        for chunk in chunks {
            try await clientTx.send(chunk)
        }
        clientTx.close()
        let (_, receivedValues) = try await (call, received)
        guard receivedValues == expected else {
            log("dodeca_devtools_lsp expected \(expected), got \(receivedValues)")
            throw SubjectError.invalidResponse
        }
        log("dodeca_devtools_lsp OK")

    default:
        log("unknown CLIENT_SCENARIO: \(scenario)")
        throw SubjectError.unknownScenario
    }
}

func runClient() async throws {
    guard let addr = ProcessInfo.processInfo.environment["PEER_ADDR"] else {
        log("PEER_ADDR not set")
        throw SubjectError.missingEnv
    }

    log("connecting to \(addr)")

    // Parse host:port
    let parts = addr.split(separator: ":")
    guard parts.count == 2, let port = Int(parts[1]) else {
        log("invalid PEER_ADDR format")
        throw SubjectError.invalidAddr
    }
    let host = String(parts[0])

    let connector = TcpConnector(host: host, port: port)

    let handler = TestbedService()
    let dispatcher = TestbedDispatcher(handler: handler)

    let connection = try await Connection.connect(
        connector,
        onLane: DefaultLaneAcceptor(dispatcher: dispatcher)
    )

    log("handshake complete")

    // Spawn driver
    let driverTask = Task {
        do {
            // r[impl hosted.subject.lifecycle]
            try await connection.run()
        } catch {
            log("driver error: \(error)")
        }
    }

    // Create client
    let lane = try await connection.openLane(
        settings: defaultLaneSettings(),
        metadata: serviceLaneMetadata("Testbed")
    )
    let client = TestbedClient(connection: lane)
    let scenario = ProcessInfo.processInfo.environment["CLIENT_SCENARIO"] ?? "echo"
    do {
        try await runClientScenario(client: client, scenario: scenario)
    } catch {
        // r[impl hosted.subject.lifecycle]
        connection.handle.shutdown()
        await driverTask.value
        throw error
    }
    // r[impl hosted.subject.lifecycle]
    connection.handle.shutdown()
    await driverTask.value
}

func runServerListen() async throws {
    let listenPort = ProcessInfo.processInfo.environment["LISTEN_PORT"].flatMap(Int.init) ?? 0
    let acceptor = TcpAcceptor(host: "127.0.0.1", port: listenPort)
    let acceptConnections = ProcessInfo.processInfo.environment["ACCEPT_CONNECTIONS"] == "1"
    let handler = TestbedService()
    let dispatcher = TestbedDispatcher(handler: handler)
    let connection = try await Connection.accept(
        acceptor,
        onLane: acceptConnections
            ? DefaultLaneAcceptor(dispatcher: dispatcher) : nil
    )
    do {
        // r[impl hosted.subject.lifecycle]
        try await connection.run()
    } catch {
        // r[impl hosted.subject.lifecycle]
        connection.handle.shutdown()
        throw error
    }
    // r[impl hosted.subject.lifecycle]
    connection.handle.shutdown()
}

// MARK: - Errors

enum SubjectError: Error {
    case missingEnv
    case invalidAddr
    case invalidResponse
    case unknownScenario
}

// MARK: - Main Entry Point

@main
struct SubjectMain {
    static func main() async {
        let mode = ProcessInfo.processInfo.environment["SUBJECT_MODE"] ?? "server"
        log("subject-swift starting in \(mode) mode")

        do {
            try await runWithSubjectTimeout(mode: mode) {
                switch mode {
                case "server":
                    try await runServer()
                case "server-listen":
                    try await runServerListen()
                case "client":
                    try await runClient()
                default:
                    log("unknown SUBJECT_MODE: \(mode)")
                    exit(1)
                }
            }
        } catch {
            log("error: \(error)")
            exit(1)
        }
    }
}

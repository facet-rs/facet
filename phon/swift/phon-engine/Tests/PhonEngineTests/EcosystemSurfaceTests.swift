import Testing

import PhonDibsEcosystemFixtures
import func PhonEcosystemFixtures.helixTraceServiceSurfaceDescriptor
import func PhonEcosystemFixtures.sampleHelixTraceServiceSurface
import PhonEngineTestSupport
import PhonIR
import PhonSchema

@testable import PhonEngine

private struct OffCpuBreakdown: Equatable {
    var sleepNs: UInt64
    var ioNs: UInt64
    var mutexNs: UInt64
}

private struct FlameNode: Equatable {
    var address: UInt64
    var functionName: UInt32?
    var binary: UInt32?
    var onCpuNs: UInt64
    var offCpu: OffCpuBreakdown
    var children: [FlameNode]
}

private struct StaxFlamegraphUpdate: Equatable {
    var totalOnCpuNs: UInt64
    var strings: [String]
    var root: FlameNode
}

private struct StaxLinuxPerfSessionConfig: Equatable {
    var targetPid: UInt32
    var frequencyHz: UInt32
    var kernelStacks: Bool
    var requestWaking: Bool
    var requestPmu: Bool
    var requestDwarfUnwind: Bool
}

private struct StaxLinuxWakingFieldOffsets: Equatable {
    var wakeePidOffset: UInt32
    var wakeePidSize: UInt32
}

private enum StaxLinuxPerfSessionError: Equatable {
    case notPrivileged(detail: String)
    case perfEventOpen(cpu: UInt32, errno: Int32, detail: String)
    case noSuchTarget(UInt32)
    case notAuthorized(callerUid: UInt32, targetUid: UInt32)
}

private struct StaxLinuxDaemonStatus: Equatable {
    var version: String
    var hostArch: String
    var privileged: Bool
    var perfEventParanoid: Int32
}

private struct StaxLinuxBrokerControlFixture: Equatable {
    var config: StaxLinuxPerfSessionConfig
    var status: StaxLinuxDaemonStatus
    var errors: [StaxLinuxPerfSessionError]
    var wakingFieldOffsets: StaxLinuxWakingFieldOffsets?
}

private struct StaxMacKdBuf: Equatable {
    var timestamp: UInt64
    var arg1: UInt64
    var arg2: UInt64
    var arg3: UInt64
    var arg4: UInt64
    var arg5: UInt64
    var debugid: UInt32
    var cpuid: UInt32
    var unused: UInt64
}

private struct StaxMacKdBufBatch: Equatable {
    var records: [StaxMacKdBuf]
    var readStartedMachTicks: UInt64
    var drainedMachTicks: UInt64
    var queuedForSendMachTicks: UInt64
    var sendStartedMachTicks: UInt64
    var drainedAtUnixNs: UInt64
}

private struct StaxNotPrivilegedPayload {
    var detail: String
}

private struct StaxPerfEventOpenPayload {
    var cpu: UInt32
    var errno: Int32
    var detail: String
}

private struct StaxNotAuthorizedPayload {
    var callerUid: UInt32
    var targetUid: UInt32
}

private enum StaxSchema {
    static let update = SchemaId(1)
    static let optionU32 = SchemaId(2)
    static let offCpu = SchemaId(3)
    static let flameNode = SchemaId(4)
    static let flameNodeList = SchemaId(5)
    static let stringList = SchemaId(6)
    static let linuxPerfSessionConfig = SchemaId(7)
    static let linuxWakingFieldOffsets = SchemaId(8)
    static let optionLinuxWakingFieldOffsets = SchemaId(9)
    static let linuxPerfSessionError = SchemaId(10)
    static let linuxPerfSessionErrorList = SchemaId(11)
    static let linuxDaemonStatus = SchemaId(12)
    static let linuxBrokerControlFixture = SchemaId(13)
    static let macKdBuf = SchemaId(14)
    static let macKdBufList = SchemaId(15)
    static let macKdBufBatch = SchemaId(16)
}

private struct DodecaRoutes: Equatable {
    var routes: Set<String>
}

private struct DodecaResolvedDependency: Equatable {
    var name: String
    var version: String?
}

private struct DodecaCodeExecutionMetadata: Equatable {
    var language: String
    var dependencies: [DodecaResolvedDependency]
    var durationMs: UInt64
}

private enum DodecaInjectionLocation: Equatable {
    case head
    case body
}

private struct DodecaInjection: Equatable {
    var location: DodecaInjectionLocation
    var content: String
}

private struct DodecaStringU32: Equatable {
    var string: String
    var value: UInt32
}

private struct DodecaResponsiveImageInfo: Equatable {
    var jxlSrcset: [DodecaStringU32]
    var webpSrcset: [DodecaStringU32]
}

private struct DodecaMountLocalization: Equatable {
    var segment: String
    var routes: Set<String>
}

private struct DodecaHtmlProcessInput: Equatable {
    var html: String
    var pathMap: [String: String]?
    var knownRoutes: Set<String>?
    var codeMetadata: [String: DodecaCodeExecutionMetadata]?
    var injections: [DodecaInjection]
    var imageVariants: [String: DodecaResponsiveImageInfo]?
    var viteCssMap: [String: [String]]?
    var mount: DodecaMountLocalization?
}

private struct DodecaStringValue: Equatable {
    var key: String
    var value: Value
}

private struct DodecaTemplateCall: Equatable {
    var contextId: String
    var name: String
    var args: [Value]
    var kwargs: [DodecaStringValue]
}

private enum DodecaLoadDataResult: Equatable {
    case success(value: Value)
    case error(message: String)
}

private struct DodecaMarkdownHeading: Equatable {
    var title: String
    var id: String
    var level: UInt8
}

private struct DodecaReqDefinition: Equatable {
    var id: String
    var anchorId: String
}

private enum DodecaSourceKind: Equatable {
    case heading
    case paragraph
    case blockQuote
    case list
    case listItem
    case definitionList
    case definitionListTitle
    case definitionListDefinition
    case thematicBreak
    case table
    case tableHead
    case tableRow
    case tableCell
    case image
}

private struct DodecaSourceMapEntry: Equatable {
    var id: String
    var kind: DodecaSourceKind
    var lineStart: UInt32
    var lineEnd: UInt32
    var byteStart: UInt64
    var byteEnd: UInt64
}

private struct DodecaSourceMap: Equatable {
    var sourcePath: String?
    var entries: [DodecaSourceMapEntry]
}

private struct DodecaFrontmatter: Equatable {
    var title: String
    var weight: Int32
    var description: String?
    var template: String?
    var extra: Value
}

private struct DodecaParseSuccessPayload: Equatable {
    var frontmatter: DodecaFrontmatter
    var html: String
    var headings: [DodecaMarkdownHeading]
    var reqs: [DodecaReqDefinition]
    var headInjections: [String]
    var sourceMap: DodecaSourceMap
}

private struct DodecaParseErrorPayload: Equatable {
    var message: String
}

private enum DodecaParseResult: Equatable {
    case success(DodecaParseSuccessPayload)
    case error(DodecaParseErrorPayload)
}

private struct DodecaDecodedImage: Equatable {
    var pixels: [UInt8]
    var width: UInt32
    var height: UInt32
    var channels: UInt8
}

private struct DodecaImageSuccessPayload: Equatable {
    var image: DodecaDecodedImage
}

private struct DodecaThumbhashSuccessPayload: Equatable {
    var dataUrl: String
}

private struct DodecaImageErrorPayload: Equatable {
    var message: String
}

private enum DodecaImageResult: Equatable {
    case success(DodecaImageSuccessPayload)
    case thumbhashSuccess(DodecaThumbhashSuccessPayload)
    case error(DodecaImageErrorPayload)
}

private struct DodecaResizeInput: Equatable {
    var pixels: [UInt8]
    var width: UInt32
    var height: UInt32
    var channels: UInt8
    var targetWidth: UInt32
}

private struct DodecaThumbhashInput: Equatable {
    var pixels: [UInt8]
    var width: UInt32
    var height: UInt32
}

private struct DodecaImageProcessorFixture: Equatable {
    var pngData: [UInt8]
    var decodedResult: DodecaImageResult
    var resizeInput: DodecaResizeInput
    var resizeResult: DodecaImageResult
    var thumbhashInput: DodecaThumbhashInput
    var thumbhashResult: DodecaImageResult
    var errorResult: DodecaImageResult
}

private struct DodecaSearchPage: Equatable {
    var url: String
    var source: String
    var html: String
}

private struct DodecaSearchFile: Equatable {
    var path: String
    var contents: [UInt8]
}

private struct DodecaSearchSuccessPayload: Equatable {
    var files: [DodecaSearchFile]
}

private struct DodecaSearchErrorPayload: Equatable {
    var message: String
}

private enum DodecaSearchIndexResult: Equatable {
    case success(DodecaSearchSuccessPayload)
    case error(DodecaSearchErrorPayload)
}

private struct DodecaSearchIndexerFixture: Equatable {
    var pages: [DodecaSearchPage]
    var result: DodecaSearchIndexResult
    var errorResult: DodecaSearchIndexResult
}

private enum DodecaSchema {
    static let routes = SchemaId(101)
    static let routeSet = SchemaId(102)
    static let optionString = SchemaId(103)
    static let resolvedDependency = SchemaId(104)
    static let resolvedDependencyList = SchemaId(105)
    static let codeExecutionMetadata = SchemaId(106)
    static let injectionLocation = SchemaId(107)
    static let injection = SchemaId(108)
    static let injectionList = SchemaId(109)
    static let stringU32Tuple = SchemaId(110)
    static let stringU32TupleList = SchemaId(111)
    static let responsiveImageInfo = SchemaId(112)
    static let mountLocalization = SchemaId(113)
    static let mapStringString = SchemaId(114)
    static let optionMapStringString = SchemaId(115)
    static let optionStringSet = SchemaId(116)
    static let mapStringCodeMetadata = SchemaId(117)
    static let optionMapStringCodeMetadata = SchemaId(118)
    static let mapStringResponsiveImageInfo = SchemaId(119)
    static let optionMapStringResponsiveImageInfo = SchemaId(120)
    static let stringList = SchemaId(121)
    static let mapStringStringList = SchemaId(122)
    static let optionMapStringStringList = SchemaId(123)
    static let optionMountLocalization = SchemaId(124)
    static let htmlProcessInput = SchemaId(125)
    static let dynamic = SchemaId(126)
    static let dynamicList = SchemaId(127)
    static let stringValueTuple = SchemaId(128)
    static let stringValueTupleList = SchemaId(129)
    static let templateCall = SchemaId(130)
    static let markdownHeading = SchemaId(131)
    static let markdownHeadingList = SchemaId(132)
    static let reqDefinition = SchemaId(133)
    static let reqDefinitionList = SchemaId(134)
    static let sourceKind = SchemaId(135)
    static let sourceMapEntry = SchemaId(136)
    static let sourceMapEntryList = SchemaId(137)
    static let sourceMap = SchemaId(138)
    static let frontmatter = SchemaId(139)
    static let parseResult = SchemaId(140)
    static let loadDataResult = SchemaId(141)
    static let decodedImage = SchemaId(142)
    static let imageResult = SchemaId(143)
    static let resizeInput = SchemaId(144)
    static let thumbhashInput = SchemaId(145)
    static let imageProcessorFixture = SchemaId(146)
    static let searchPage = SchemaId(147)
    static let searchPageList = SchemaId(148)
    static let searchFile = SchemaId(149)
    static let searchFileList = SchemaId(150)
    static let searchIndexResult = SchemaId(151)
    static let searchIndexerFixture = SchemaId(152)
}

private enum DibsSqlValue: Equatable {
    case null
    case bool(Bool)
    case i16(Int16)
    case i32(Int32)
    case i64(Int64)
    case f32(Float)
    case f64(Double)
    case string(String)
    case bytes([UInt8])
}

private struct DibsRowField: Equatable {
    var name: String
    var value: DibsSqlValue
}

private struct DibsListResponse: Equatable {
    var rows: [[DibsRowField]]
    var total: UInt64?
}

private enum DibsSchema {
    static let sqlValue = SchemaId(301)
    static let rowField = SchemaId(302)
    static let rowFieldList = SchemaId(303)
    static let rowFieldListList = SchemaId(304)
    static let optionU64 = SchemaId(305)
    static let listResponse = SchemaId(306)
}

private enum HotmealLiveReloadEvent: Equatable {
    case reload
    case patches(route: String, patchesBlob: [UInt8])
    case headChanged(route: String)
}

private struct HotmealSubscribeRequest: Equatable {
    var route: String
}

private struct HotmealLiveReloadFixture: Equatable {
    var subscribe: HotmealSubscribeRequest
    var events: [HotmealLiveReloadEvent]
}

private enum HotmealSchema {
    static let liveReloadEvent = SchemaId(401)
    static let liveReloadEventList = SchemaId(402)
    static let subscribeRequest = SchemaId(403)
    static let liveReloadFixture = SchemaId(404)
}

private struct TraceyRuleId: Equatable {
    var base: String
    var version: UInt32
}

private struct TraceyRuleRef: Equatable {
    var id: TraceyRuleId
    var text: String?
}

private struct TraceySectionRules: Equatable {
    var section: String
    var rules: [TraceyRuleRef]
}

private struct TraceyUncoveredRequest: Equatable {
    var spec: String?
    var implName: String?
    var prefix: String?
}

private struct TraceyUncoveredResponse: Equatable {
    var spec: String
    var implName: String
    var totalRules: UInt64
    var uncoveredCount: UInt64
    var bySection: [TraceySectionRules]
}

private struct TraceyImplStatus: Equatable {
    var spec: String
    var implName: String
    var totalRules: UInt64
    var coveredRules: UInt64
    var staleRules: UInt64
    var verifiedRules: UInt64
}

private struct TraceyStatusResponse: Equatable {
    var impls: [TraceyImplStatus]
}

private struct TraceyCoverageChange: Equatable {
    var ruleId: TraceyRuleId
    var file: String
    var line: UInt64
}

private struct TraceyDeltaSummary: Equatable {
    var newlyCovered: [TraceyCoverageChange]
    var newlyUncovered: [TraceyRuleId]
}

private struct TraceyDataUpdate: Equatable {
    var version: UInt64
    var delta: TraceyDeltaSummary?
}

private struct TraceyLspDiagnostic: Equatable {
    var severity: String
    var code: String
    var message: String
    var startLine: UInt32
    var startChar: UInt32
    var endLine: UInt32
    var endChar: UInt32
}

private struct TraceyLspFileDiagnostics: Equatable {
    var path: String
    var diagnostics: [TraceyLspDiagnostic]
}

private struct TraceyLspSymbol: Equatable {
    var name: String
    var kind: String
    var path: String?
    var startLine: UInt32
    var startChar: UInt32
    var endLine: UInt32
    var endChar: UInt32
}

private struct TraceyMigrationFixture: Equatable {
    var status: TraceyStatusResponse
    var uncoveredRequest: TraceyUncoveredRequest
    var uncoveredResponse: TraceyUncoveredResponse
    var dataUpdateItem: TraceyDataUpdate
    var workspaceDiagnostics: [TraceyLspFileDiagnostics]
    var workspaceSymbols: [TraceyLspSymbol]
}

private enum TraceySchema {
    static let ruleId = SchemaId(501)
    static let optionString = SchemaId(502)
    static let ruleRef = SchemaId(503)
    static let ruleRefList = SchemaId(504)
    static let sectionRules = SchemaId(505)
    static let sectionRulesList = SchemaId(506)
    static let uncoveredRequest = SchemaId(507)
    static let uncoveredResponse = SchemaId(508)
    static let implStatus = SchemaId(509)
    static let implStatusList = SchemaId(510)
    static let statusResponse = SchemaId(511)
    static let coverageChange = SchemaId(512)
    static let coverageChangeList = SchemaId(513)
    static let ruleIdList = SchemaId(514)
    static let deltaSummary = SchemaId(515)
    static let optionDeltaSummary = SchemaId(516)
    static let dataUpdate = SchemaId(517)
    static let lspDiagnostic = SchemaId(518)
    static let lspDiagnosticList = SchemaId(519)
    static let lspFileDiagnostics = SchemaId(520)
    static let lspFileDiagnosticsList = SchemaId(521)
    static let lspSymbol = SchemaId(522)
    static let lspSymbolList = SchemaId(523)
    static let migrationFixture = SchemaId(524)
}

private struct HelixAudioTokenRange: Equatable {
    var start: UInt32
    var end: UInt32
}

private struct HelixAudioRepresentationSpan: Equatable {
    var audio: HelixAudioTokenRange
    var audioRepresentationVersion: UInt32
}

private struct HelixStreamMeta: Equatable {
    var schemaVersion: UInt32
    var pulseIds: [UInt64]
    var timelineEventCount: UInt64
    var attentionBatchCount: UInt64
}

private struct HelixVerifyOutcome: Equatable {
    var rewindK: UInt64
    var acceptedPrefixLen: UInt64?
    var divergenceRow: UInt64?
    var discardedSpeculativeTokens: UInt64?
}

private struct HelixPulseRollup: Equatable {
    var pulseId: UInt64
    var pulseStartUs: UInt64?
    var pulseDurationUs: UInt64?
    var encoderDurationUs: UInt64?
    var refreshDurationUs: UInt64?
    var verifyDurationUs: UInt64?
    var decodeDurationUs: UInt64?
    var commitDurationUs: UInt64?
    var pulseMelFrames: UInt64
    var committedTokens: UInt64
    var retainedSpeculativeTokens: UInt64
    var residentCommittedTokens: UInt64
    var evictedAudioTokens: UInt64
    var evictedCommittedTokens: UInt64
    var decodedTokens: UInt64
    var hitEos: Bool
    var verify: HelixVerifyOutcome?
    var hasAttentionBatch: Bool
    var arTokenCount: UInt64
}

private struct HelixTextTokenSnapshot: Equatable {
    var textTokenId: UInt32
    var text: String?
    var textBefore: String?
    var inVerifyBatch: Bool
    var decodedThisPulse: Bool
}

private struct HelixPromptLayout: Equatable {
    var pulseId: UInt64
    var firstAudioTokenId: UInt32
    var residentAudioFrames: UInt64
    var changedAudioSpans: [HelixAudioRepresentationSpan]
    var textTokenStart: UInt32
    var textTokenEnd: UInt32
    var textTokens: [HelixTextTokenSnapshot]
}

private struct HelixPulseAttentionHeatmap: Equatable {
    var pulseId: UInt64
    var firstAudioTokenId: UInt32
    var audioTokenCount: UInt32
    var textTokenStart: UInt32
    var textTokenCount: UInt32
    var recordCount: UInt32
    var maxValue: Float
    var meanAudioMass: [Float]
    var textTokenGlyphs: [String]
}

private struct HelixStreamMetrics: Equatable {
    var pulseIds: [UInt64]
    var pulseDurationUs: [UInt64]
    var decodedTokens: [UInt64]
    var committedTokens: [UInt64]
    var retainedSpeculativeTokens: [UInt64]
    var evictedAudioTokens: [UInt64]
    var evictedCommittedTokens: [UInt64]
    var rewindK: [UInt64]
    var arTokenCount: [UInt64]
    var rollingWer: [Double]
    var s2dP50Ms: [Double]
}

private struct HelixPulseAvailable: Equatable {
    var pulseId: UInt64
}

private struct HelixRunInfo: Equatable {
    var backend: String
    var modelDir: String
    var input: String
    var piece: String?
    var pulseMs: UInt32
    var audioRingCapacity: UInt32
    var textRingCapacity: UInt32
    var commitRevisableTailTextTokens: UInt32
    var reviseLogitMargin: Float
    var sampleRate: UInt32
    var melHopSamples: UInt32
    var numMelBins: UInt32
    var numMelFrames: UInt32
    var audioTokensPerChunk: UInt32
    var nativeWindowTokens: UInt32
    var realtimePacing: Bool
    var profilePhases: Bool
    var attentionTraceSchemaVersion: UInt32
    var traceServerSchemaVersion: UInt32
}

private struct HelixTraceSnapshot: Equatable {
    var meta: HelixStreamMeta
    var runInfo: HelixRunInfo
    var rollup: HelixPulseRollup?
    var promptLayout: HelixPromptLayout?
    var attentionHeatmap: HelixPulseAttentionHeatmap?
    var streamMetrics: HelixStreamMetrics
    var pulseAvailable: HelixPulseAvailable
}

private enum HelixSchema {
    static let optionU64 = SchemaId(601)
    static let optionString = SchemaId(602)
    static let u64List = SchemaId(603)
    static let f32List = SchemaId(604)
    static let f64List = SchemaId(605)
    static let stringList = SchemaId(606)
    static let audioTokenRange = SchemaId(607)
    static let audioRepresentationSpan = SchemaId(608)
    static let audioRepresentationSpanList = SchemaId(609)
    static let streamMeta = SchemaId(610)
    static let verifyOutcome = SchemaId(611)
    static let optionVerifyOutcome = SchemaId(612)
    static let pulseRollup = SchemaId(613)
    static let optionPulseRollup = SchemaId(614)
    static let textTokenSnapshot = SchemaId(615)
    static let textTokenSnapshotList = SchemaId(616)
    static let promptLayout = SchemaId(617)
    static let optionPromptLayout = SchemaId(618)
    static let attentionHeatmap = SchemaId(619)
    static let optionAttentionHeatmap = SchemaId(620)
    static let streamMetrics = SchemaId(621)
    static let pulseAvailable = SchemaId(622)
    static let runInfo = SchemaId(623)
    static let traceSnapshot = SchemaId(624)
}

private struct StyxValue: Equatable {
    var tag: StyxTag?
    var payload: StyxPayload?
    var span: StyxSpan?
}

private struct StyxTag: Equatable {
    var name: String
    var span: StyxSpan?
}

private indirect enum StyxPayload: Equatable {
    case scalar(StyxScalar)
    case sequence(StyxSequence)
    case object(StyxObject)
}

private struct StyxScalar: Equatable {
    var text: String
    var kind: StyxScalarKind
    var span: StyxSpan?
}

private enum StyxScalarKind: Equatable {
    case bare
    case quoted
    case raw
    case heredoc
}

private struct StyxSequence: Equatable {
    var items: [StyxValue]
    var span: StyxSpan?
}

private struct StyxEntry: Equatable {
    var key: StyxValue
    var value: StyxValue
    var docComment: String?
}

private struct StyxObject: Equatable {
    var entries: [StyxEntry]
    var span: StyxSpan?
}

private struct StyxSpan: Equatable {
    var start: UInt32
    var end: UInt32
}

private struct StyxLspPosition: Equatable {
    var line: UInt32
    var character: UInt32
}

private struct StyxLspRange: Equatable {
    var start: StyxLspPosition
    var end: StyxLspPosition
}

private struct StyxLspCursor: Equatable {
    var line: UInt32
    var character: UInt32
    var offset: UInt32
}

private enum StyxLspCapability: Equatable {
    case completions
    case hover
    case diagnostics
    case codeActions
    case definition
}

private struct StyxLspInitializeParams: Equatable {
    var styxVersion: String
    var documentUri: String
    var schemaId: String
}

private struct StyxLspInitializeResult: Equatable {
    var name: String
    var version: String
    var capabilities: [StyxLspCapability]
}

private struct StyxLspCompletionParams: Equatable {
    var documentUri: String
    var cursor: StyxLspCursor
    var path: [String]
    var prefix: String
    var context: StyxValue?
    var taggedContext: StyxValue?
}

private enum StyxLspCompletionKind: Equatable {
    case field
    case type
    case function
    case keyword
}

private struct StyxLspCompletionItem: Equatable {
    var label: String
    var detail: String?
    var documentation: String?
    var kind: StyxLspCompletionKind?
    var sortText: String?
    var insertText: String?
}

private struct StyxLspHoverParams: Equatable {
    var documentUri: String
    var cursor: StyxLspCursor
    var path: [String]
    var context: StyxValue?
    var taggedContext: StyxValue?
}

private struct StyxLspHoverResult: Equatable {
    var contents: String
    var range: StyxLspRange?
}

private struct StyxLspInlayHintParams: Equatable {
    var documentUri: String
    var range: StyxLspRange
    var context: StyxValue?
}

private enum StyxLspInlayHintKind: Equatable {
    case type
    case parameter
}

private struct StyxLspInlayHint: Equatable {
    var position: StyxLspPosition
    var label: String
    var kind: StyxLspInlayHintKind?
    var paddingLeft: Bool
    var paddingRight: Bool
}

private enum StyxLspDiagnosticSeverity: Equatable {
    case error
    case warning
    case information
    case hint
}

private struct StyxLspDiagnostic: Equatable {
    var span: StyxSpan
    var severity: StyxLspDiagnosticSeverity
    var message: String
    var source: String?
    var code: String?
    var data: StyxValue?
}

private struct StyxLspDiagnosticParams: Equatable {
    var documentUri: String
    var tree: StyxValue
    var content: String
}

private struct StyxLspCodeActionParams: Equatable {
    var documentUri: String
    var span: StyxSpan
    var diagnostics: [StyxLspDiagnostic]
}

private enum StyxLspCodeActionKind: Equatable {
    case quickFix
    case refactor
}

private struct StyxLspWorkspaceEdit: Equatable {
    var changes: [StyxLspDocumentEdit]
}

private struct StyxLspDocumentEdit: Equatable {
    var uri: String
    var edits: [StyxLspTextEdit]
}

private struct StyxLspTextEdit: Equatable {
    var span: StyxSpan
    var newText: String
}

private struct StyxLspCodeAction: Equatable {
    var title: String
    var kind: StyxLspCodeActionKind?
    var edit: StyxLspWorkspaceEdit?
    var isPreferred: Bool
}

private struct StyxLspDefinitionParams: Equatable {
    var documentUri: String
    var cursor: StyxLspCursor
    var path: [String]
    var context: StyxValue?
    var taggedContext: StyxValue?
}

private struct StyxLspLocation: Equatable {
    var uri: String
    var span: StyxSpan
}

private struct StyxLspSchemaInfo: Equatable {
    var source: String
    var uri: String
}

private struct StyxLspGetSubtreeParams: Equatable {
    var documentUri: String
    var path: [String]
}

private struct StyxLspGetDocumentParams: Equatable {
    var documentUri: String
}

private struct StyxLspGetSourceParams: Equatable {
    var documentUri: String
}

private struct StyxLspGetSchemaParams: Equatable {
    var documentUri: String
}

private struct StyxLspOffsetToPositionParams: Equatable {
    var documentUri: String
    var offset: UInt32
}

private struct StyxLspPositionToOffsetParams: Equatable {
    var documentUri: String
    var position: StyxLspPosition
}

private struct StyxLspSurfaceFixture: Equatable {
    var initializeParams: StyxLspInitializeParams
    var initializeResult: StyxLspInitializeResult
    var completionParams: StyxLspCompletionParams
    var completions: [StyxLspCompletionItem]
    var hoverParams: StyxLspHoverParams
    var hoverResult: StyxLspHoverResult?
    var inlayHintParams: StyxLspInlayHintParams
    var inlayHints: [StyxLspInlayHint]
    var diagnosticParams: StyxLspDiagnosticParams
    var diagnostics: [StyxLspDiagnostic]
    var codeActionParams: StyxLspCodeActionParams
    var codeActions: [StyxLspCodeAction]
    var definitionParams: StyxLspDefinitionParams
    var locations: [StyxLspLocation]
    var getSubtreeParams: StyxLspGetSubtreeParams
    var subtree: StyxValue?
    var getDocumentParams: StyxLspGetDocumentParams
    var document: StyxValue?
    var getSourceParams: StyxLspGetSourceParams
    var source: String?
    var getSchemaParams: StyxLspGetSchemaParams
    var schema: StyxLspSchemaInfo?
    var offsetToPositionParams: StyxLspOffsetToPositionParams
    var position: StyxLspPosition?
    var positionToOffsetParams: StyxLspPositionToOffsetParams
    var offset: UInt32?
}

private enum StyxSchema {
    static let value = SchemaId(201)
    static let optionTag = SchemaId(202)
    static let tag = SchemaId(203)
    static let optionPayload = SchemaId(204)
    static let payload = SchemaId(205)
    static let scalar = SchemaId(206)
    static let scalarKind = SchemaId(207)
    static let sequence = SchemaId(208)
    static let valueList = SchemaId(209)
    static let entry = SchemaId(210)
    static let entryList = SchemaId(211)
    static let object = SchemaId(212)
    static let span = SchemaId(213)
    static let optionSpan = SchemaId(214)
    static let optionString = SchemaId(215)
    static let optionValue = SchemaId(216)
    static let stringList = SchemaId(217)
    static let optionU32 = SchemaId(218)
    static let lspPosition = SchemaId(219)
    static let lspRange = SchemaId(220)
    static let lspCursor = SchemaId(221)
    static let lspCapability = SchemaId(222)
    static let lspCapabilityList = SchemaId(223)
    static let lspInitializeParams = SchemaId(224)
    static let lspInitializeResult = SchemaId(225)
    static let lspCompletionKind = SchemaId(226)
    static let optionLspCompletionKind = SchemaId(227)
    static let lspCompletionParams = SchemaId(228)
    static let lspCompletionItem = SchemaId(229)
    static let lspCompletionItemList = SchemaId(230)
    static let optionLspRange = SchemaId(231)
    static let lspHoverParams = SchemaId(232)
    static let lspHoverResult = SchemaId(233)
    static let optionLspHoverResult = SchemaId(234)
    static let lspInlayHintKind = SchemaId(235)
    static let optionLspInlayHintKind = SchemaId(236)
    static let lspInlayHintParams = SchemaId(237)
    static let lspInlayHint = SchemaId(238)
    static let lspInlayHintList = SchemaId(239)
    static let lspDiagnosticSeverity = SchemaId(240)
    static let lspDiagnostic = SchemaId(241)
    static let lspDiagnosticList = SchemaId(242)
    static let lspDiagnosticParams = SchemaId(243)
    static let lspCodeActionKind = SchemaId(244)
    static let optionLspCodeActionKind = SchemaId(245)
    static let lspTextEdit = SchemaId(246)
    static let lspTextEditList = SchemaId(247)
    static let lspDocumentEdit = SchemaId(248)
    static let lspDocumentEditList = SchemaId(249)
    static let lspWorkspaceEdit = SchemaId(250)
    static let optionLspWorkspaceEdit = SchemaId(251)
    static let lspCodeActionParams = SchemaId(252)
    static let lspCodeAction = SchemaId(253)
    static let lspCodeActionList = SchemaId(254)
    static let lspDefinitionParams = SchemaId(255)
    static let lspLocation = SchemaId(256)
    static let lspLocationList = SchemaId(257)
    static let lspSchemaInfo = SchemaId(258)
    static let optionLspSchemaInfo = SchemaId(259)
    static let lspGetSubtreeParams = SchemaId(260)
    static let lspGetDocumentParams = SchemaId(261)
    static let lspGetSourceParams = SchemaId(262)
    static let lspGetSchemaParams = SchemaId(263)
    static let lspOffsetToPositionParams = SchemaId(264)
    static let optionLspPosition = SchemaId(265)
    static let lspPositionToOffsetParams = SchemaId(266)
    static let lspSurfaceFixture = SchemaId(267)
}

private func scalarDesc(_ p: Primitive) -> Descriptor {
    let size = fixedSize(p)!
    return Descriptor(
        schema: .concrete(primitiveId(p)),
        layout: Layout(size: size, align: alignment(p)),
        access: .scalar
    )
}

private func stringDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(primitiveId(.string)),
        layout: MemoryLayout<String>.phonLayout,
        access: .bytes(BytesAccess(stride: 1, elemAlign: 1, witness: .string))
    )
}

private func bytesDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(primitiveId(.bytes)),
        layout: MemoryLayout<[UInt8]>.phonLayout,
        access: .bytes(BytesAccess(stride: 1, elemAlign: 1, witness: .byteArray))
    )
}

private func optionU32Desc() -> Descriptor {
    Descriptor(
        schema: .concrete(StaxSchema.optionU32),
        layout: MemoryLayout<UInt32?>.phonLayout,
        access: .option(OptionAccess(witness: .of(UInt32.self), some: scalarDesc(.u32)))
    )
}

private func stringListDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(StaxSchema.stringList),
        layout: MemoryLayout<[String]>.phonLayout,
        access: .sequence(SequenceAccess(
            element: stringDesc(),
            stride: MemoryLayout<String>.stride,
            elemAlign: MemoryLayout<String>.alignment,
            witness: .of(String.self)
        ))
    )
}

private func dodecaSchemas() -> [Schema] {
    [
        Schema(id: DodecaSchema.optionString, kind: .option(element: .concrete(primitiveId(.string)))),
        Schema(id: DodecaSchema.resolvedDependency, kind: .structure(name: "ResolvedDependency", fields: [
            Field(name: "name", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "version", schema: .concrete(DodecaSchema.optionString), required: true),
        ])),
        Schema(id: DodecaSchema.resolvedDependencyList, kind: .list(element: .concrete(DodecaSchema.resolvedDependency))),
        Schema(id: DodecaSchema.codeExecutionMetadata, kind: .structure(name: "CodeExecutionMetadata", fields: [
            Field(name: "language", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "dependencies", schema: .concrete(DodecaSchema.resolvedDependencyList), required: true),
            Field(name: "duration_ms", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
        Schema(id: DodecaSchema.injectionLocation, kind: .enumeration(name: "InjectionLocation", variants: [
            Variant(name: "Head", index: 0, payload: .unit),
            Variant(name: "Body", index: 1, payload: .unit),
        ])),
        Schema(id: DodecaSchema.injection, kind: .structure(name: "Injection", fields: [
            Field(name: "location", schema: .concrete(DodecaSchema.injectionLocation), required: true),
            Field(name: "content", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: DodecaSchema.injectionList, kind: .list(element: .concrete(DodecaSchema.injection))),
        Schema(id: DodecaSchema.stringU32Tuple, kind: .tuple(elements: [
            .concrete(primitiveId(.string)),
            .concrete(primitiveId(.u32)),
        ])),
        Schema(id: DodecaSchema.stringU32TupleList, kind: .list(element: .concrete(DodecaSchema.stringU32Tuple))),
        Schema(id: DodecaSchema.responsiveImageInfo, kind: .structure(name: "ResponsiveImageInfo", fields: [
            Field(name: "jxl_srcset", schema: .concrete(DodecaSchema.stringU32TupleList), required: true),
            Field(name: "webp_srcset", schema: .concrete(DodecaSchema.stringU32TupleList), required: true),
        ])),
        Schema(id: DodecaSchema.routeSet, kind: .set(element: .concrete(primitiveId(.string)))),
        Schema(id: DodecaSchema.mountLocalization, kind: .structure(name: "MountLocalization", fields: [
            Field(name: "segment", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "routes", schema: .concrete(DodecaSchema.routeSet), required: true),
        ])),
        Schema(id: DodecaSchema.mapStringString, kind: .map(key: .concrete(primitiveId(.string)), value: .concrete(primitiveId(.string)))),
        Schema(id: DodecaSchema.optionMapStringString, kind: .option(element: .concrete(DodecaSchema.mapStringString))),
        Schema(id: DodecaSchema.optionStringSet, kind: .option(element: .concrete(DodecaSchema.routeSet))),
        Schema(id: DodecaSchema.mapStringCodeMetadata, kind: .map(key: .concrete(primitiveId(.string)), value: .concrete(DodecaSchema.codeExecutionMetadata))),
        Schema(id: DodecaSchema.optionMapStringCodeMetadata, kind: .option(element: .concrete(DodecaSchema.mapStringCodeMetadata))),
        Schema(id: DodecaSchema.mapStringResponsiveImageInfo, kind: .map(key: .concrete(primitiveId(.string)), value: .concrete(DodecaSchema.responsiveImageInfo))),
        Schema(id: DodecaSchema.optionMapStringResponsiveImageInfo, kind: .option(element: .concrete(DodecaSchema.mapStringResponsiveImageInfo))),
        Schema(id: DodecaSchema.stringList, kind: .list(element: .concrete(primitiveId(.string)))),
        Schema(id: DodecaSchema.mapStringStringList, kind: .map(key: .concrete(primitiveId(.string)), value: .concrete(DodecaSchema.stringList))),
        Schema(id: DodecaSchema.optionMapStringStringList, kind: .option(element: .concrete(DodecaSchema.mapStringStringList))),
        Schema(id: DodecaSchema.optionMountLocalization, kind: .option(element: .concrete(DodecaSchema.mountLocalization))),
        Schema(id: DodecaSchema.htmlProcessInput, kind: .structure(name: "DodecaHtmlProcessInput", fields: [
            Field(name: "html", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "path_map", schema: .concrete(DodecaSchema.optionMapStringString), required: true),
            Field(name: "known_routes", schema: .concrete(DodecaSchema.optionStringSet), required: true),
            Field(name: "code_metadata", schema: .concrete(DodecaSchema.optionMapStringCodeMetadata), required: true),
            Field(name: "injections", schema: .concrete(DodecaSchema.injectionList), required: true),
            Field(name: "image_variants", schema: .concrete(DodecaSchema.optionMapStringResponsiveImageInfo), required: true),
            Field(name: "vite_css_map", schema: .concrete(DodecaSchema.optionMapStringStringList), required: true),
            Field(name: "mount", schema: .concrete(DodecaSchema.optionMountLocalization), required: true),
        ])),
        Schema(id: DodecaSchema.dynamic, kind: .dynamic),
        Schema(id: DodecaSchema.dynamicList, kind: .list(element: .concrete(DodecaSchema.dynamic))),
        Schema(id: DodecaSchema.stringValueTuple, kind: .tuple(elements: [
            .concrete(primitiveId(.string)),
            .concrete(DodecaSchema.dynamic),
        ])),
        Schema(id: DodecaSchema.stringValueTupleList, kind: .list(element: .concrete(DodecaSchema.stringValueTuple))),
        Schema(id: DodecaSchema.templateCall, kind: .structure(name: "DodecaTemplateCall", fields: [
            Field(name: "context_id", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "name", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "args", schema: .concrete(DodecaSchema.dynamicList), required: true),
            Field(name: "kwargs", schema: .concrete(DodecaSchema.stringValueTupleList), required: true),
        ])),
        Schema(id: DodecaSchema.loadDataResult, kind: .enumeration(name: "DodecaLoadDataResult", variants: [
            Variant(name: "Success", index: 0, payload: .structure([
                Field(name: "value", schema: .concrete(DodecaSchema.dynamic), required: true),
            ])),
            Variant(name: "Error", index: 1, payload: .structure([
                Field(name: "message", schema: .concrete(primitiveId(.string)), required: true),
            ])),
        ])),
        Schema(id: DodecaSchema.markdownHeading, kind: .structure(name: "DodecaMarkdownHeading", fields: [
            Field(name: "title", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "id", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "level", schema: .concrete(primitiveId(.u8)), required: true),
        ])),
        Schema(id: DodecaSchema.markdownHeadingList, kind: .list(element: .concrete(DodecaSchema.markdownHeading))),
        Schema(id: DodecaSchema.reqDefinition, kind: .structure(name: "DodecaReqDefinition", fields: [
            Field(name: "id", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "anchor_id", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: DodecaSchema.reqDefinitionList, kind: .list(element: .concrete(DodecaSchema.reqDefinition))),
        Schema(id: DodecaSchema.sourceKind, kind: .enumeration(name: "DodecaSourceKind", variants: [
            Variant(name: "Heading", index: 0, payload: .unit),
            Variant(name: "Paragraph", index: 1, payload: .unit),
            Variant(name: "BlockQuote", index: 2, payload: .unit),
            Variant(name: "List", index: 3, payload: .unit),
            Variant(name: "ListItem", index: 4, payload: .unit),
            Variant(name: "DefinitionList", index: 5, payload: .unit),
            Variant(name: "DefinitionListTitle", index: 6, payload: .unit),
            Variant(name: "DefinitionListDefinition", index: 7, payload: .unit),
            Variant(name: "ThematicBreak", index: 8, payload: .unit),
            Variant(name: "Table", index: 9, payload: .unit),
            Variant(name: "TableHead", index: 10, payload: .unit),
            Variant(name: "TableRow", index: 11, payload: .unit),
            Variant(name: "TableCell", index: 12, payload: .unit),
            Variant(name: "Image", index: 13, payload: .unit),
        ])),
        Schema(id: DodecaSchema.sourceMapEntry, kind: .structure(name: "DodecaSourceMapEntry", fields: [
            Field(name: "id", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "kind", schema: .concrete(DodecaSchema.sourceKind), required: true),
            Field(name: "line_start", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "line_end", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "byte_start", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "byte_end", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
        Schema(id: DodecaSchema.sourceMapEntryList, kind: .list(element: .concrete(DodecaSchema.sourceMapEntry))),
        Schema(id: DodecaSchema.sourceMap, kind: .structure(name: "DodecaSourceMap", fields: [
            Field(name: "source_path", schema: .concrete(DodecaSchema.optionString), required: true),
            Field(name: "entries", schema: .concrete(DodecaSchema.sourceMapEntryList), required: true),
        ])),
        Schema(id: DodecaSchema.frontmatter, kind: .structure(name: "DodecaFrontmatter", fields: [
            Field(name: "title", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "weight", schema: .concrete(primitiveId(.i32)), required: true),
            Field(name: "description", schema: .concrete(DodecaSchema.optionString), required: true),
            Field(name: "template", schema: .concrete(DodecaSchema.optionString), required: true),
            Field(name: "extra", schema: .concrete(DodecaSchema.dynamic), required: true),
        ])),
        Schema(id: DodecaSchema.parseResult, kind: .enumeration(name: "DodecaParseResult", variants: [
            Variant(name: "Success", index: 0, payload: .structure([
                Field(name: "frontmatter", schema: .concrete(DodecaSchema.frontmatter), required: true),
                Field(name: "html", schema: .concrete(primitiveId(.string)), required: true),
                Field(name: "headings", schema: .concrete(DodecaSchema.markdownHeadingList), required: true),
                Field(name: "reqs", schema: .concrete(DodecaSchema.reqDefinitionList), required: true),
                Field(name: "head_injections", schema: .concrete(DodecaSchema.stringList), required: true),
                Field(name: "source_map", schema: .concrete(DodecaSchema.sourceMap), required: true),
            ])),
            Variant(name: "Error", index: 1, payload: .structure([
                Field(name: "message", schema: .concrete(primitiveId(.string)), required: true),
            ])),
        ])),
        Schema(id: DodecaSchema.decodedImage, kind: .structure(name: "DecodedImage", fields: [
            Field(name: "pixels", schema: .concrete(primitiveId(.bytes)), required: true),
            Field(name: "width", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "height", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "channels", schema: .concrete(primitiveId(.u8)), required: true),
        ])),
        Schema(id: DodecaSchema.imageResult, kind: .enumeration(name: "ImageResult", variants: [
            Variant(name: "Success", index: 0, payload: .structure([
                Field(name: "image", schema: .concrete(DodecaSchema.decodedImage), required: true),
            ])),
            Variant(name: "ThumbhashSuccess", index: 1, payload: .structure([
                Field(name: "data_url", schema: .concrete(primitiveId(.string)), required: true),
            ])),
            Variant(name: "Error", index: 2, payload: .structure([
                Field(name: "message", schema: .concrete(primitiveId(.string)), required: true),
            ])),
        ])),
        Schema(id: DodecaSchema.resizeInput, kind: .structure(name: "ResizeInput", fields: [
            Field(name: "pixels", schema: .concrete(primitiveId(.bytes)), required: true),
            Field(name: "width", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "height", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "channels", schema: .concrete(primitiveId(.u8)), required: true),
            Field(name: "target_width", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: DodecaSchema.thumbhashInput, kind: .structure(name: "ThumbhashInput", fields: [
            Field(name: "pixels", schema: .concrete(primitiveId(.bytes)), required: true),
            Field(name: "width", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "height", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: DodecaSchema.imageProcessorFixture, kind: .structure(name: "DodecaImageProcessorFixture", fields: [
            Field(name: "png_data", schema: .concrete(primitiveId(.bytes)), required: true),
            Field(name: "decoded_result", schema: .concrete(DodecaSchema.imageResult), required: true),
            Field(name: "resize_input", schema: .concrete(DodecaSchema.resizeInput), required: true),
            Field(name: "resize_result", schema: .concrete(DodecaSchema.imageResult), required: true),
            Field(name: "thumbhash_input", schema: .concrete(DodecaSchema.thumbhashInput), required: true),
            Field(name: "thumbhash_result", schema: .concrete(DodecaSchema.imageResult), required: true),
            Field(name: "error_result", schema: .concrete(DodecaSchema.imageResult), required: true),
        ])),
        Schema(id: DodecaSchema.searchPage, kind: .structure(name: "SearchPage", fields: [
            Field(name: "url", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "source", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "html", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: DodecaSchema.searchPageList, kind: .list(element: .concrete(DodecaSchema.searchPage))),
        Schema(id: DodecaSchema.searchFile, kind: .structure(name: "SearchFile", fields: [
            Field(name: "path", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "contents", schema: .concrete(primitiveId(.bytes)), required: true),
        ])),
        Schema(id: DodecaSchema.searchFileList, kind: .list(element: .concrete(DodecaSchema.searchFile))),
        Schema(id: DodecaSchema.searchIndexResult, kind: .enumeration(name: "SearchIndexResult", variants: [
            Variant(name: "Success", index: 0, payload: .structure([
                Field(name: "files", schema: .concrete(DodecaSchema.searchFileList), required: true),
            ])),
            Variant(name: "Error", index: 1, payload: .structure([
                Field(name: "message", schema: .concrete(primitiveId(.string)), required: true),
            ])),
        ])),
        Schema(id: DodecaSchema.searchIndexerFixture, kind: .structure(name: "DodecaSearchIndexerFixture", fields: [
            Field(name: "pages", schema: .concrete(DodecaSchema.searchPageList), required: true),
            Field(name: "result", schema: .concrete(DodecaSchema.searchIndexResult), required: true),
            Field(name: "error_result", schema: .concrete(DodecaSchema.searchIndexResult), required: true),
        ])),
        Schema(id: DodecaSchema.routes, kind: .structure(name: "DodecaRoutes", fields: [
            Field(name: "routes", schema: .concrete(DodecaSchema.routeSet), required: true),
        ])),
    ]
}

private func dodecaRouteSetDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(DodecaSchema.routeSet),
        layout: MemoryLayout<Set<String>>.phonLayout,
        access: .sequence(SequenceAccess(
            element: stringDesc(),
            stride: MemoryLayout<String>.stride,
            elemAlign: MemoryLayout<String>.alignment,
            witness: .setOf(String.self)
        ))
    )
}

private func dodecaRoutesDescriptor() -> (root: Descriptor, registry: Registry) {
    let root = recordDesc(DodecaSchema.routes, DodecaRoutes.self, fields: [
        fieldAccess(\DodecaRoutes.routes, dodecaRouteSetDesc()),
    ])
    return (root, Registry(dodecaSchemas()))
}

private func dodecaOptionStringDesc() -> Descriptor {
    optionDesc(DodecaSchema.optionString, String.self, some: stringDesc())
}

private func dodecaResolvedDependencyDesc() -> Descriptor {
    recordDesc(DodecaSchema.resolvedDependency, DodecaResolvedDependency.self, fields: [
        fieldAccess(\DodecaResolvedDependency.name, stringDesc()),
        fieldAccess(\DodecaResolvedDependency.version, dodecaOptionStringDesc()),
    ])
}

private func dodecaResolvedDependencyListDesc() -> Descriptor {
    listDesc(DodecaSchema.resolvedDependencyList, DodecaResolvedDependency.self, element: dodecaResolvedDependencyDesc())
}

private func dodecaCodeExecutionMetadataDesc() -> Descriptor {
    recordDesc(DodecaSchema.codeExecutionMetadata, DodecaCodeExecutionMetadata.self, fields: [
        fieldAccess(\DodecaCodeExecutionMetadata.language, stringDesc()),
        fieldAccess(\DodecaCodeExecutionMetadata.dependencies, dodecaResolvedDependencyListDesc()),
        fieldAccess(\DodecaCodeExecutionMetadata.durationMs, scalarDesc(.u64)),
    ])
}

private func dodecaInjectionLocationDesc() -> Descriptor {
    unitEnumDesc(
        DodecaSchema.injectionLocation,
        DodecaInjectionLocation.self,
        variantCount: 2,
        tag: { ptr in
            switch ptr.assumingMemoryBound(to: DodecaInjectionLocation.self).pointee {
            case .head: return 0
            case .body: return 1
            }
        },
        make: { index in
            switch index {
            case 0: return .head
            case 1: return .body
            default: fatalError("bad DodecaInjectionLocation variant index")
            }
        }
    )
}

private func dodecaInjectionDesc() -> Descriptor {
    recordDesc(DodecaSchema.injection, DodecaInjection.self, fields: [
        fieldAccess(\DodecaInjection.location, dodecaInjectionLocationDesc()),
        fieldAccess(\DodecaInjection.content, stringDesc()),
    ])
}

private func dodecaInjectionListDesc() -> Descriptor {
    listDesc(DodecaSchema.injectionList, DodecaInjection.self, element: dodecaInjectionDesc())
}

private func dodecaStringU32TupleDesc() -> Descriptor {
    recordDesc(DodecaSchema.stringU32Tuple, DodecaStringU32.self, fields: [
        fieldAccess(\DodecaStringU32.string, stringDesc()),
        fieldAccess(\DodecaStringU32.value, scalarDesc(.u32)),
    ])
}

private func dodecaStringU32TupleListDesc() -> Descriptor {
    listDesc(DodecaSchema.stringU32TupleList, DodecaStringU32.self, element: dodecaStringU32TupleDesc())
}

private func dodecaResponsiveImageInfoDesc() -> Descriptor {
    recordDesc(DodecaSchema.responsiveImageInfo, DodecaResponsiveImageInfo.self, fields: [
        fieldAccess(\DodecaResponsiveImageInfo.jxlSrcset, dodecaStringU32TupleListDesc()),
        fieldAccess(\DodecaResponsiveImageInfo.webpSrcset, dodecaStringU32TupleListDesc()),
    ])
}

private func dodecaMountLocalizationDesc() -> Descriptor {
    recordDesc(DodecaSchema.mountLocalization, DodecaMountLocalization.self, fields: [
        fieldAccess(\DodecaMountLocalization.segment, stringDesc()),
        fieldAccess(\DodecaMountLocalization.routes, dodecaRouteSetDesc()),
    ])
}

private func dodecaStringListDesc() -> Descriptor {
    listDesc(DodecaSchema.stringList, String.self, element: stringDesc())
}

private func dodecaStringMapDesc<T>(
    _ schema: SchemaId,
    _ valueType: T.Type,
    value: Descriptor
) -> Descriptor {
    Descriptor(
        schema: .concrete(schema),
        layout: MemoryLayout<[String: T]>.phonLayout,
        access: .map(MapAccess(
            key: stringDesc(),
            value: value,
            keyStride: MemoryLayout<String>.stride,
            keyAlign: MemoryLayout<String>.alignment,
            valueStride: MemoryLayout<T>.stride,
            valueAlign: MemoryLayout<T>.alignment,
            witness: .stringKeyed(T.self)
        ))
    )
}

private func dodecaMapStringStringDesc() -> Descriptor {
    dodecaStringMapDesc(DodecaSchema.mapStringString, String.self, value: stringDesc())
}

private func dodecaMapStringCodeMetadataDesc() -> Descriptor {
    dodecaStringMapDesc(
        DodecaSchema.mapStringCodeMetadata,
        DodecaCodeExecutionMetadata.self,
        value: dodecaCodeExecutionMetadataDesc()
    )
}

private func dodecaMapStringResponsiveImageInfoDesc() -> Descriptor {
    dodecaStringMapDesc(
        DodecaSchema.mapStringResponsiveImageInfo,
        DodecaResponsiveImageInfo.self,
        value: dodecaResponsiveImageInfoDesc()
    )
}

private func dodecaMapStringStringListDesc() -> Descriptor {
    dodecaStringMapDesc(DodecaSchema.mapStringStringList, [String].self, value: dodecaStringListDesc())
}

private func dodecaHtmlProcessInputDescriptor() -> (root: Descriptor, registry: Registry) {
    let root = recordDesc(DodecaSchema.htmlProcessInput, DodecaHtmlProcessInput.self, fields: [
        fieldAccess(\DodecaHtmlProcessInput.html, stringDesc()),
        fieldAccess(\DodecaHtmlProcessInput.pathMap, optionDesc(DodecaSchema.optionMapStringString, [String: String].self, some: dodecaMapStringStringDesc())),
        fieldAccess(\DodecaHtmlProcessInput.knownRoutes, optionDesc(DodecaSchema.optionStringSet, Set<String>.self, some: dodecaRouteSetDesc())),
        fieldAccess(\DodecaHtmlProcessInput.codeMetadata, optionDesc(DodecaSchema.optionMapStringCodeMetadata, [String: DodecaCodeExecutionMetadata].self, some: dodecaMapStringCodeMetadataDesc())),
        fieldAccess(\DodecaHtmlProcessInput.injections, dodecaInjectionListDesc()),
        fieldAccess(\DodecaHtmlProcessInput.imageVariants, optionDesc(DodecaSchema.optionMapStringResponsiveImageInfo, [String: DodecaResponsiveImageInfo].self, some: dodecaMapStringResponsiveImageInfoDesc())),
        fieldAccess(\DodecaHtmlProcessInput.viteCssMap, optionDesc(DodecaSchema.optionMapStringStringList, [String: [String]].self, some: dodecaMapStringStringListDesc())),
        fieldAccess(\DodecaHtmlProcessInput.mount, optionDesc(DodecaSchema.optionMountLocalization, DodecaMountLocalization.self, some: dodecaMountLocalizationDesc())),
    ])
    return (root, Registry(dodecaSchemas()))
}

private func dodecaDynamicDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(DodecaSchema.dynamic),
        layout: MemoryLayout<Value>.phonLayout,
        access: .dynamic
    )
}

private func dodecaDynamicListDesc() -> Descriptor {
    listDesc(DodecaSchema.dynamicList, Value.self, element: dodecaDynamicDesc())
}

private func dodecaStringValueTupleDesc() -> Descriptor {
    recordDesc(DodecaSchema.stringValueTuple, DodecaStringValue.self, fields: [
        fieldAccess(\DodecaStringValue.key, stringDesc()),
        fieldAccess(\DodecaStringValue.value, dodecaDynamicDesc()),
    ])
}

private func dodecaStringValueTupleListDesc() -> Descriptor {
    listDesc(DodecaSchema.stringValueTupleList, DodecaStringValue.self, element: dodecaStringValueTupleDesc())
}

private func dodecaTemplateCallDescriptor() -> (root: Descriptor, registry: Registry) {
    let root = recordDesc(DodecaSchema.templateCall, DodecaTemplateCall.self, fields: [
        fieldAccess(\DodecaTemplateCall.contextId, stringDesc()),
        fieldAccess(\DodecaTemplateCall.name, stringDesc()),
        fieldAccess(\DodecaTemplateCall.args, dodecaDynamicListDesc()),
        fieldAccess(\DodecaTemplateCall.kwargs, dodecaStringValueTupleListDesc()),
    ])
    return (root, Registry(dodecaSchemas()))
}

private func dodecaLoadDataResultDesc() -> Descriptor {
    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: DodecaLoadDataResult.self).pointee {
        case .success: return 0
        case .error: return 1
        }
    }
    let projectPayload: (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void = { value, _, scratch in
        switch value.assumingMemoryBound(to: DodecaLoadDataResult.self).pointee {
        case .success(let payload):
            scratch.assumingMemoryBound(to: Value.self).initialize(to: payload)
        case .error(let message):
            scratch.assumingMemoryBound(to: String.self).initialize(to: message)
        }
    }
    let destroyPayload: (UnsafeMutableRawPointer, Int) -> Void = { scratch, localIndex in
        switch localIndex {
        case 0:
            scratch.assumingMemoryBound(to: Value.self).deinitialize(count: 1)
        case 1:
            scratch.assumingMemoryBound(to: String.self).deinitialize(count: 1)
        default:
            break
        }
    }
    let inject: (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void = { slot, localIndex, scratch in
        let result: DodecaLoadDataResult
        switch localIndex {
        case 0:
            result = .success(value: scratch.assumingMemoryBound(to: Value.self).move())
        case 1:
            result = .error(message: scratch.assumingMemoryBound(to: String.self).move())
        default:
            fatalError("bad DodecaLoadDataResult variant index")
        }
        slot.assumingMemoryBound(to: DodecaLoadDataResult.self).initialize(to: result)
    }

    return Descriptor(
        schema: .concrete(DodecaSchema.loadDataResult),
        layout: MemoryLayout<DodecaLoadDataResult>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: projectPayload,
            destroyPayload: destroyPayload,
            inject: inject,
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [FieldAccess(offset: 0, descriptor: dodecaDynamicDesc())],
                    payloadLayout: MemoryLayout<Value>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [FieldAccess(offset: 0, descriptor: stringDesc())],
                    payloadLayout: MemoryLayout<String>.phonLayout
                ),
            ]
        ))
    )
}

private func dodecaLoadDataResultDescriptor() -> (root: Descriptor, registry: Registry) {
    (dodecaLoadDataResultDesc(), Registry(dodecaSchemas()))
}

private func dodecaMarkdownHeadingDesc() -> Descriptor {
    recordDesc(DodecaSchema.markdownHeading, DodecaMarkdownHeading.self, fields: [
        fieldAccess(\DodecaMarkdownHeading.title, stringDesc()),
        fieldAccess(\DodecaMarkdownHeading.id, stringDesc()),
        fieldAccess(\DodecaMarkdownHeading.level, scalarDesc(.u8)),
    ])
}

private func dodecaMarkdownHeadingListDesc() -> Descriptor {
    listDesc(DodecaSchema.markdownHeadingList, DodecaMarkdownHeading.self, element: dodecaMarkdownHeadingDesc())
}

private func dodecaReqDefinitionDesc() -> Descriptor {
    recordDesc(DodecaSchema.reqDefinition, DodecaReqDefinition.self, fields: [
        fieldAccess(\DodecaReqDefinition.id, stringDesc()),
        fieldAccess(\DodecaReqDefinition.anchorId, stringDesc()),
    ])
}

private func dodecaReqDefinitionListDesc() -> Descriptor {
    listDesc(DodecaSchema.reqDefinitionList, DodecaReqDefinition.self, element: dodecaReqDefinitionDesc())
}

private func dodecaSourceKindDesc() -> Descriptor {
    unitEnumDesc(
        DodecaSchema.sourceKind,
        DodecaSourceKind.self,
        variantCount: 14,
        tag: { ptr in
            switch ptr.assumingMemoryBound(to: DodecaSourceKind.self).pointee {
            case .heading: return 0
            case .paragraph: return 1
            case .blockQuote: return 2
            case .list: return 3
            case .listItem: return 4
            case .definitionList: return 5
            case .definitionListTitle: return 6
            case .definitionListDefinition: return 7
            case .thematicBreak: return 8
            case .table: return 9
            case .tableHead: return 10
            case .tableRow: return 11
            case .tableCell: return 12
            case .image: return 13
            }
        },
        make: { index in
            switch index {
            case 0: return .heading
            case 1: return .paragraph
            case 2: return .blockQuote
            case 3: return .list
            case 4: return .listItem
            case 5: return .definitionList
            case 6: return .definitionListTitle
            case 7: return .definitionListDefinition
            case 8: return .thematicBreak
            case 9: return .table
            case 10: return .tableHead
            case 11: return .tableRow
            case 12: return .tableCell
            case 13: return .image
            default: fatalError("bad DodecaSourceKind variant index")
            }
        }
    )
}

private func dodecaSourceMapEntryDesc() -> Descriptor {
    recordDesc(DodecaSchema.sourceMapEntry, DodecaSourceMapEntry.self, fields: [
        fieldAccess(\DodecaSourceMapEntry.id, stringDesc()),
        fieldAccess(\DodecaSourceMapEntry.kind, dodecaSourceKindDesc()),
        fieldAccess(\DodecaSourceMapEntry.lineStart, scalarDesc(.u32)),
        fieldAccess(\DodecaSourceMapEntry.lineEnd, scalarDesc(.u32)),
        fieldAccess(\DodecaSourceMapEntry.byteStart, scalarDesc(.u64)),
        fieldAccess(\DodecaSourceMapEntry.byteEnd, scalarDesc(.u64)),
    ])
}

private func dodecaSourceMapEntryListDesc() -> Descriptor {
    listDesc(DodecaSchema.sourceMapEntryList, DodecaSourceMapEntry.self, element: dodecaSourceMapEntryDesc())
}

private func dodecaSourceMapDesc() -> Descriptor {
    recordDesc(DodecaSchema.sourceMap, DodecaSourceMap.self, fields: [
        fieldAccess(\DodecaSourceMap.sourcePath, dodecaOptionStringDesc()),
        fieldAccess(\DodecaSourceMap.entries, dodecaSourceMapEntryListDesc()),
    ])
}

private func dodecaFrontmatterDesc() -> Descriptor {
    recordDesc(DodecaSchema.frontmatter, DodecaFrontmatter.self, fields: [
        fieldAccess(\DodecaFrontmatter.title, stringDesc()),
        fieldAccess(\DodecaFrontmatter.weight, scalarDesc(.i32)),
        fieldAccess(\DodecaFrontmatter.description, dodecaOptionStringDesc()),
        fieldAccess(\DodecaFrontmatter.template, dodecaOptionStringDesc()),
        fieldAccess(\DodecaFrontmatter.extra, dodecaDynamicDesc()),
    ])
}

private func dodecaParseResultDesc() -> Descriptor {
    let successFrontmatterOffset = MemoryLayout<DodecaParseSuccessPayload>.offset(of: \DodecaParseSuccessPayload.frontmatter)!
    let successHtmlOffset = MemoryLayout<DodecaParseSuccessPayload>.offset(of: \DodecaParseSuccessPayload.html)!
    let successHeadingsOffset = MemoryLayout<DodecaParseSuccessPayload>.offset(of: \DodecaParseSuccessPayload.headings)!
    let successReqsOffset = MemoryLayout<DodecaParseSuccessPayload>.offset(of: \DodecaParseSuccessPayload.reqs)!
    let successHeadInjectionsOffset = MemoryLayout<DodecaParseSuccessPayload>.offset(of: \DodecaParseSuccessPayload.headInjections)!
    let successSourceMapOffset = MemoryLayout<DodecaParseSuccessPayload>.offset(of: \DodecaParseSuccessPayload.sourceMap)!
    let errorMessageOffset = MemoryLayout<DodecaParseErrorPayload>.offset(of: \DodecaParseErrorPayload.message)!

    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: DodecaParseResult.self).pointee {
        case .success: return 0
        case .error: return 1
        }
    }
    let projectPayload: (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void = { value, _, scratch in
        switch value.assumingMemoryBound(to: DodecaParseResult.self).pointee {
        case .success(let payload):
            scratch.assumingMemoryBound(to: DodecaParseSuccessPayload.self).initialize(to: payload)
        case .error(let payload):
            scratch.assumingMemoryBound(to: DodecaParseErrorPayload.self).initialize(to: payload)
        }
    }
    let destroyPayload: (UnsafeMutableRawPointer, Int) -> Void = { scratch, localIndex in
        switch localIndex {
        case 0:
            scratch.assumingMemoryBound(to: DodecaParseSuccessPayload.self).deinitialize(count: 1)
        case 1:
            scratch.assumingMemoryBound(to: DodecaParseErrorPayload.self).deinitialize(count: 1)
        default:
            break
        }
    }
    let inject: (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void = { slot, localIndex, scratch in
        let result: DodecaParseResult
        switch localIndex {
        case 0:
            result = .success(scratch.assumingMemoryBound(to: DodecaParseSuccessPayload.self).move())
        case 1:
            result = .error(scratch.assumingMemoryBound(to: DodecaParseErrorPayload.self).move())
        default:
            fatalError("bad DodecaParseResult variant index")
        }
        slot.assumingMemoryBound(to: DodecaParseResult.self).initialize(to: result)
    }

    return Descriptor(
        schema: .concrete(DodecaSchema.parseResult),
        layout: MemoryLayout<DodecaParseResult>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: projectPayload,
            destroyPayload: destroyPayload,
            inject: inject,
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [
                        FieldAccess(offset: successFrontmatterOffset, descriptor: dodecaFrontmatterDesc()),
                        FieldAccess(offset: successHtmlOffset, descriptor: stringDesc()),
                        FieldAccess(offset: successHeadingsOffset, descriptor: dodecaMarkdownHeadingListDesc()),
                        FieldAccess(offset: successReqsOffset, descriptor: dodecaReqDefinitionListDesc()),
                        FieldAccess(offset: successHeadInjectionsOffset, descriptor: dodecaStringListDesc()),
                        FieldAccess(offset: successSourceMapOffset, descriptor: dodecaSourceMapDesc()),
                    ],
                    payloadLayout: MemoryLayout<DodecaParseSuccessPayload>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [
                        FieldAccess(offset: errorMessageOffset, descriptor: stringDesc()),
                    ],
                    payloadLayout: MemoryLayout<DodecaParseErrorPayload>.phonLayout
                ),
            ]
        ))
    )
}

private func dodecaParseResultDescriptor() -> (root: Descriptor, registry: Registry) {
    (dodecaParseResultDesc(), Registry(dodecaSchemas()))
}

private func dodecaDecodedImageDesc() -> Descriptor {
    recordDesc(DodecaSchema.decodedImage, DodecaDecodedImage.self, fields: [
        fieldAccess(\DodecaDecodedImage.pixels, bytesDesc()),
        fieldAccess(\DodecaDecodedImage.width, scalarDesc(.u32)),
        fieldAccess(\DodecaDecodedImage.height, scalarDesc(.u32)),
        fieldAccess(\DodecaDecodedImage.channels, scalarDesc(.u8)),
    ])
}

private func dodecaResizeInputDesc() -> Descriptor {
    recordDesc(DodecaSchema.resizeInput, DodecaResizeInput.self, fields: [
        fieldAccess(\DodecaResizeInput.pixels, bytesDesc()),
        fieldAccess(\DodecaResizeInput.width, scalarDesc(.u32)),
        fieldAccess(\DodecaResizeInput.height, scalarDesc(.u32)),
        fieldAccess(\DodecaResizeInput.channels, scalarDesc(.u8)),
        fieldAccess(\DodecaResizeInput.targetWidth, scalarDesc(.u32)),
    ])
}

private func dodecaThumbhashInputDesc() -> Descriptor {
    recordDesc(DodecaSchema.thumbhashInput, DodecaThumbhashInput.self, fields: [
        fieldAccess(\DodecaThumbhashInput.pixels, bytesDesc()),
        fieldAccess(\DodecaThumbhashInput.width, scalarDesc(.u32)),
        fieldAccess(\DodecaThumbhashInput.height, scalarDesc(.u32)),
    ])
}

private func dodecaImageResultDesc() -> Descriptor {
    let successImageOffset = MemoryLayout<DodecaImageSuccessPayload>.offset(of: \DodecaImageSuccessPayload.image)!
    let thumbhashDataUrlOffset = MemoryLayout<DodecaThumbhashSuccessPayload>.offset(of: \DodecaThumbhashSuccessPayload.dataUrl)!
    let errorMessageOffset = MemoryLayout<DodecaImageErrorPayload>.offset(of: \DodecaImageErrorPayload.message)!

    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: DodecaImageResult.self).pointee {
        case .success: return 0
        case .thumbhashSuccess: return 1
        case .error: return 2
        }
    }
    let projectPayload: (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void = { value, _, scratch in
        switch value.assumingMemoryBound(to: DodecaImageResult.self).pointee {
        case .success(let payload):
            scratch.assumingMemoryBound(to: DodecaImageSuccessPayload.self).initialize(to: payload)
        case .thumbhashSuccess(let payload):
            scratch.assumingMemoryBound(to: DodecaThumbhashSuccessPayload.self).initialize(to: payload)
        case .error(let payload):
            scratch.assumingMemoryBound(to: DodecaImageErrorPayload.self).initialize(to: payload)
        }
    }
    let destroyPayload: (UnsafeMutableRawPointer, Int) -> Void = { scratch, localIndex in
        switch localIndex {
        case 0:
            scratch.assumingMemoryBound(to: DodecaImageSuccessPayload.self).deinitialize(count: 1)
        case 1:
            scratch.assumingMemoryBound(to: DodecaThumbhashSuccessPayload.self).deinitialize(count: 1)
        case 2:
            scratch.assumingMemoryBound(to: DodecaImageErrorPayload.self).deinitialize(count: 1)
        default:
            break
        }
    }
    let inject: (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void = { slot, localIndex, scratch in
        let result: DodecaImageResult
        switch localIndex {
        case 0:
            result = .success(scratch.assumingMemoryBound(to: DodecaImageSuccessPayload.self).move())
        case 1:
            result = .thumbhashSuccess(scratch.assumingMemoryBound(to: DodecaThumbhashSuccessPayload.self).move())
        case 2:
            result = .error(scratch.assumingMemoryBound(to: DodecaImageErrorPayload.self).move())
        default:
            fatalError("bad DodecaImageResult variant index")
        }
        slot.assumingMemoryBound(to: DodecaImageResult.self).initialize(to: result)
    }

    return Descriptor(
        schema: .concrete(DodecaSchema.imageResult),
        layout: MemoryLayout<DodecaImageResult>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: projectPayload,
            destroyPayload: destroyPayload,
            inject: inject,
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [
                        FieldAccess(offset: successImageOffset, descriptor: dodecaDecodedImageDesc()),
                    ],
                    payloadLayout: MemoryLayout<DodecaImageSuccessPayload>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [
                        FieldAccess(offset: thumbhashDataUrlOffset, descriptor: stringDesc()),
                    ],
                    payloadLayout: MemoryLayout<DodecaThumbhashSuccessPayload>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 2,
                    payloadFields: [
                        FieldAccess(offset: errorMessageOffset, descriptor: stringDesc()),
                    ],
                    payloadLayout: MemoryLayout<DodecaImageErrorPayload>.phonLayout
                ),
            ]
        ))
    )
}

private func dodecaImageProcessorFixtureDescriptor() -> (root: Descriptor, registry: Registry) {
    let root = recordDesc(DodecaSchema.imageProcessorFixture, DodecaImageProcessorFixture.self, fields: [
        fieldAccess(\DodecaImageProcessorFixture.pngData, bytesDesc()),
        fieldAccess(\DodecaImageProcessorFixture.decodedResult, dodecaImageResultDesc()),
        fieldAccess(\DodecaImageProcessorFixture.resizeInput, dodecaResizeInputDesc()),
        fieldAccess(\DodecaImageProcessorFixture.resizeResult, dodecaImageResultDesc()),
        fieldAccess(\DodecaImageProcessorFixture.thumbhashInput, dodecaThumbhashInputDesc()),
        fieldAccess(\DodecaImageProcessorFixture.thumbhashResult, dodecaImageResultDesc()),
        fieldAccess(\DodecaImageProcessorFixture.errorResult, dodecaImageResultDesc()),
    ])
    return (root, Registry(dodecaSchemas()))
}

private func dodecaSearchPageDesc() -> Descriptor {
    recordDesc(DodecaSchema.searchPage, DodecaSearchPage.self, fields: [
        fieldAccess(\DodecaSearchPage.url, stringDesc()),
        fieldAccess(\DodecaSearchPage.source, stringDesc()),
        fieldAccess(\DodecaSearchPage.html, stringDesc()),
    ])
}

private func dodecaSearchPageListDesc() -> Descriptor {
    listDesc(DodecaSchema.searchPageList, DodecaSearchPage.self, element: dodecaSearchPageDesc())
}

private func dodecaSearchFileDesc() -> Descriptor {
    recordDesc(DodecaSchema.searchFile, DodecaSearchFile.self, fields: [
        fieldAccess(\DodecaSearchFile.path, stringDesc()),
        fieldAccess(\DodecaSearchFile.contents, bytesDesc()),
    ])
}

private func dodecaSearchFileListDesc() -> Descriptor {
    listDesc(DodecaSchema.searchFileList, DodecaSearchFile.self, element: dodecaSearchFileDesc())
}

private func dodecaSearchIndexResultDesc() -> Descriptor {
    let successFilesOffset = MemoryLayout<DodecaSearchSuccessPayload>.offset(of: \DodecaSearchSuccessPayload.files)!
    let errorMessageOffset = MemoryLayout<DodecaSearchErrorPayload>.offset(of: \DodecaSearchErrorPayload.message)!

    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: DodecaSearchIndexResult.self).pointee {
        case .success: return 0
        case .error: return 1
        }
    }
    let projectPayload: (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void = { value, _, scratch in
        switch value.assumingMemoryBound(to: DodecaSearchIndexResult.self).pointee {
        case .success(let payload):
            scratch.assumingMemoryBound(to: DodecaSearchSuccessPayload.self).initialize(to: payload)
        case .error(let payload):
            scratch.assumingMemoryBound(to: DodecaSearchErrorPayload.self).initialize(to: payload)
        }
    }
    let destroyPayload: (UnsafeMutableRawPointer, Int) -> Void = { scratch, localIndex in
        switch localIndex {
        case 0:
            scratch.assumingMemoryBound(to: DodecaSearchSuccessPayload.self).deinitialize(count: 1)
        case 1:
            scratch.assumingMemoryBound(to: DodecaSearchErrorPayload.self).deinitialize(count: 1)
        default:
            break
        }
    }
    let inject: (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void = { slot, localIndex, scratch in
        let result: DodecaSearchIndexResult
        switch localIndex {
        case 0:
            result = .success(scratch.assumingMemoryBound(to: DodecaSearchSuccessPayload.self).move())
        case 1:
            result = .error(scratch.assumingMemoryBound(to: DodecaSearchErrorPayload.self).move())
        default:
            fatalError("bad DodecaSearchIndexResult variant index")
        }
        slot.assumingMemoryBound(to: DodecaSearchIndexResult.self).initialize(to: result)
    }

    return Descriptor(
        schema: .concrete(DodecaSchema.searchIndexResult),
        layout: MemoryLayout<DodecaSearchIndexResult>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: projectPayload,
            destroyPayload: destroyPayload,
            inject: inject,
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [
                        FieldAccess(offset: successFilesOffset, descriptor: dodecaSearchFileListDesc()),
                    ],
                    payloadLayout: MemoryLayout<DodecaSearchSuccessPayload>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [
                        FieldAccess(offset: errorMessageOffset, descriptor: stringDesc()),
                    ],
                    payloadLayout: MemoryLayout<DodecaSearchErrorPayload>.phonLayout
                ),
            ]
        ))
    )
}

private func dodecaSearchIndexerFixtureDescriptor() -> (root: Descriptor, registry: Registry) {
    let root = recordDesc(DodecaSchema.searchIndexerFixture, DodecaSearchIndexerFixture.self, fields: [
        fieldAccess(\DodecaSearchIndexerFixture.pages, dodecaSearchPageListDesc()),
        fieldAccess(\DodecaSearchIndexerFixture.result, dodecaSearchIndexResultDesc()),
        fieldAccess(\DodecaSearchIndexerFixture.errorResult, dodecaSearchIndexResultDesc()),
    ])
    return (root, Registry(dodecaSchemas()))
}

private func dibsSchemas() -> [Schema] {
    [
        Schema(id: DibsSchema.sqlValue, kind: .enumeration(name: "SqlValue", variants: [
            Variant(name: "Null", index: 0, payload: .unit),
            Variant(name: "Bool", index: 1, payload: .newtype(.concrete(primitiveId(.bool)))),
            Variant(name: "I16", index: 2, payload: .newtype(.concrete(primitiveId(.i16)))),
            Variant(name: "I32", index: 3, payload: .newtype(.concrete(primitiveId(.i32)))),
            Variant(name: "I64", index: 4, payload: .newtype(.concrete(primitiveId(.i64)))),
            Variant(name: "F32", index: 5, payload: .newtype(.concrete(primitiveId(.f32)))),
            Variant(name: "F64", index: 6, payload: .newtype(.concrete(primitiveId(.f64)))),
            Variant(name: "String", index: 7, payload: .newtype(.concrete(primitiveId(.string)))),
            Variant(name: "Bytes", index: 8, payload: .newtype(.concrete(primitiveId(.bytes)))),
        ])),
        Schema(id: DibsSchema.rowField, kind: .structure(name: "RowField", fields: [
            Field(name: "name", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "value", schema: .concrete(DibsSchema.sqlValue), required: true),
        ])),
        Schema(id: DibsSchema.rowFieldList, kind: .list(element: .concrete(DibsSchema.rowField))),
        Schema(id: DibsSchema.rowFieldListList, kind: .list(element: .concrete(DibsSchema.rowFieldList))),
        Schema(id: DibsSchema.optionU64, kind: .option(element: .concrete(primitiveId(.u64)))),
        Schema(id: DibsSchema.listResponse, kind: .structure(name: "DibsListResponse", fields: [
            Field(name: "rows", schema: .concrete(DibsSchema.rowFieldListList), required: true),
            Field(name: "total", schema: .concrete(DibsSchema.optionU64), required: true),
        ])),
    ]
}

private func dibsOptionU64Desc() -> Descriptor {
    optionDesc(DibsSchema.optionU64, UInt64.self, some: scalarDesc(.u64))
}

private func dibsSqlValueDesc() -> Descriptor {
    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: DibsSqlValue.self).pointee {
        case .null: return 0
        case .bool: return 1
        case .i16: return 2
        case .i32: return 3
        case .i64: return 4
        case .f32: return 5
        case .f64: return 6
        case .string: return 7
        case .bytes: return 8
        }
    }
    let projectPayload: (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void = { value, _, scratch in
        switch value.assumingMemoryBound(to: DibsSqlValue.self).pointee {
        case .null:
            break
        case .bool(let payload):
            scratch.storeBytes(of: payload, as: Bool.self)
        case .i16(let payload):
            scratch.storeBytes(of: payload, as: Int16.self)
        case .i32(let payload):
            scratch.storeBytes(of: payload, as: Int32.self)
        case .i64(let payload):
            scratch.storeBytes(of: payload, as: Int64.self)
        case .f32(let payload):
            scratch.storeBytes(of: payload, as: Float.self)
        case .f64(let payload):
            scratch.storeBytes(of: payload, as: Double.self)
        case .string(let payload):
            scratch.assumingMemoryBound(to: String.self).initialize(to: payload)
        case .bytes(let payload):
            scratch.assumingMemoryBound(to: [UInt8].self).initialize(to: payload)
        }
    }
    let destroyPayload: (UnsafeMutableRawPointer, Int) -> Void = { scratch, localIndex in
        switch localIndex {
        case 7:
            scratch.assumingMemoryBound(to: String.self).deinitialize(count: 1)
        case 8:
            scratch.assumingMemoryBound(to: [UInt8].self).deinitialize(count: 1)
        default:
            break
        }
    }
    let inject: (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void = { slot, localIndex, scratch in
        let value: DibsSqlValue
        switch localIndex {
        case 0:
            value = .null
        case 1:
            value = .bool(scratch.load(as: Bool.self))
        case 2:
            value = .i16(scratch.load(as: Int16.self))
        case 3:
            value = .i32(scratch.load(as: Int32.self))
        case 4:
            value = .i64(scratch.load(as: Int64.self))
        case 5:
            value = .f32(scratch.load(as: Float.self))
        case 6:
            value = .f64(scratch.load(as: Double.self))
        case 7:
            value = .string(scratch.assumingMemoryBound(to: String.self).move())
        case 8:
            value = .bytes(scratch.assumingMemoryBound(to: [UInt8].self).move())
        default:
            fatalError("bad DibsSqlValue variant index")
        }
        slot.assumingMemoryBound(to: DibsSqlValue.self).initialize(to: value)
    }

    return Descriptor(
        schema: .concrete(DibsSchema.sqlValue),
        layout: MemoryLayout<DibsSqlValue>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: projectPayload,
            destroyPayload: destroyPayload,
            inject: inject,
            variants: [
                VariantAccess(wireIndex: 0, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
                VariantAccess(wireIndex: 1, payloadFields: [FieldAccess(offset: 0, descriptor: scalarDesc(.bool))], payloadLayout: MemoryLayout<Bool>.phonLayout),
                VariantAccess(wireIndex: 2, payloadFields: [FieldAccess(offset: 0, descriptor: scalarDesc(.i16))], payloadLayout: MemoryLayout<Int16>.phonLayout),
                VariantAccess(wireIndex: 3, payloadFields: [FieldAccess(offset: 0, descriptor: scalarDesc(.i32))], payloadLayout: MemoryLayout<Int32>.phonLayout),
                VariantAccess(wireIndex: 4, payloadFields: [FieldAccess(offset: 0, descriptor: scalarDesc(.i64))], payloadLayout: MemoryLayout<Int64>.phonLayout),
                VariantAccess(wireIndex: 5, payloadFields: [FieldAccess(offset: 0, descriptor: scalarDesc(.f32))], payloadLayout: MemoryLayout<Float>.phonLayout),
                VariantAccess(wireIndex: 6, payloadFields: [FieldAccess(offset: 0, descriptor: scalarDesc(.f64))], payloadLayout: MemoryLayout<Double>.phonLayout),
                VariantAccess(wireIndex: 7, payloadFields: [FieldAccess(offset: 0, descriptor: stringDesc())], payloadLayout: MemoryLayout<String>.phonLayout),
                VariantAccess(wireIndex: 8, payloadFields: [FieldAccess(offset: 0, descriptor: bytesDesc())], payloadLayout: MemoryLayout<[UInt8]>.phonLayout),
            ]
        ))
    )
}

private func dibsRowFieldDesc() -> Descriptor {
    recordDesc(DibsSchema.rowField, DibsRowField.self, fields: [
        fieldAccess(\DibsRowField.name, stringDesc()),
        fieldAccess(\DibsRowField.value, dibsSqlValueDesc()),
    ])
}

private func dibsRowFieldListDesc() -> Descriptor {
    listDesc(DibsSchema.rowFieldList, DibsRowField.self, element: dibsRowFieldDesc())
}

private func dibsRowFieldListListDesc() -> Descriptor {
    listDesc(DibsSchema.rowFieldListList, [DibsRowField].self, element: dibsRowFieldListDesc())
}

private func dibsListResponseDescriptor() -> (root: Descriptor, registry: Registry) {
    let root = recordDesc(DibsSchema.listResponse, DibsListResponse.self, fields: [
        fieldAccess(\DibsListResponse.rows, dibsRowFieldListListDesc()),
        fieldAccess(\DibsListResponse.total, dibsOptionU64Desc()),
    ])
    return (root, Registry(dibsSchemas()))
}

private func hotmealSchemas() -> [Schema] {
    [
        Schema(id: HotmealSchema.liveReloadEvent, kind: .enumeration(name: "HotmealLiveReloadEvent", variants: [
            Variant(name: "Reload", index: 0, payload: .unit),
            Variant(name: "Patches", index: 1, payload: .structure([
                Field(name: "route", schema: .concrete(primitiveId(.string)), required: true),
                Field(name: "patches_blob", schema: .concrete(primitiveId(.bytes)), required: true),
            ])),
            Variant(name: "HeadChanged", index: 2, payload: .structure([
                Field(name: "route", schema: .concrete(primitiveId(.string)), required: true),
            ])),
        ])),
        Schema(id: HotmealSchema.liveReloadEventList, kind: .list(element: .concrete(HotmealSchema.liveReloadEvent))),
        Schema(id: HotmealSchema.subscribeRequest, kind: .structure(name: "HotmealSubscribeRequest", fields: [
            Field(name: "route", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: HotmealSchema.liveReloadFixture, kind: .structure(name: "HotmealLiveReloadFixture", fields: [
            Field(name: "subscribe", schema: .concrete(HotmealSchema.subscribeRequest), required: true),
            Field(name: "events", schema: .concrete(HotmealSchema.liveReloadEventList), required: true),
        ])),
    ]
}

private func hotmealLiveReloadEventDesc() -> Descriptor {
    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: HotmealLiveReloadEvent.self).pointee {
        case .reload: return 0
        case .patches: return 1
        case .headChanged: return 2
        }
    }
    let projectPayload: (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void = { value, _, scratch in
        switch value.assumingMemoryBound(to: HotmealLiveReloadEvent.self).pointee {
        case .reload:
            break
        case .patches(let route, let patchesBlob):
            scratch.advanced(by: MemoryLayout<(String, [UInt8])>.offset(of: \.0)!)
                .assumingMemoryBound(to: String.self)
                .initialize(to: route)
            scratch.advanced(by: MemoryLayout<(String, [UInt8])>.offset(of: \.1)!)
                .assumingMemoryBound(to: [UInt8].self)
                .initialize(to: patchesBlob)
        case .headChanged(let route):
            scratch.assumingMemoryBound(to: String.self).initialize(to: route)
        }
    }
    let destroyPayload: (UnsafeMutableRawPointer, Int) -> Void = { scratch, localIndex in
        switch localIndex {
        case 1:
            scratch.advanced(by: MemoryLayout<(String, [UInt8])>.offset(of: \.0)!)
                .assumingMemoryBound(to: String.self)
                .deinitialize(count: 1)
            scratch.advanced(by: MemoryLayout<(String, [UInt8])>.offset(of: \.1)!)
                .assumingMemoryBound(to: [UInt8].self)
                .deinitialize(count: 1)
        case 2:
            scratch.assumingMemoryBound(to: String.self).deinitialize(count: 1)
        default:
            break
        }
    }
    let inject: (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void = { slot, localIndex, scratch in
        let event: HotmealLiveReloadEvent
        switch localIndex {
        case 0:
            event = .reload
        case 1:
            let route = scratch.advanced(by: MemoryLayout<(String, [UInt8])>.offset(of: \.0)!)
                .assumingMemoryBound(to: String.self)
                .move()
            let patchesBlob = scratch.advanced(by: MemoryLayout<(String, [UInt8])>.offset(of: \.1)!)
                .assumingMemoryBound(to: [UInt8].self)
                .move()
            event = .patches(route: route, patchesBlob: patchesBlob)
        case 2:
            let route = scratch.assumingMemoryBound(to: String.self).move()
            event = .headChanged(route: route)
        default:
            fatalError("bad HotmealLiveReloadEvent variant index")
        }
        slot.assumingMemoryBound(to: HotmealLiveReloadEvent.self).initialize(to: event)
    }

    return Descriptor(
        schema: .concrete(HotmealSchema.liveReloadEvent),
        layout: MemoryLayout<HotmealLiveReloadEvent>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: projectPayload,
            destroyPayload: destroyPayload,
            inject: inject,
            variants: [
                VariantAccess(wireIndex: 0, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [
                        FieldAccess(offset: MemoryLayout<(String, [UInt8])>.offset(of: \.0)!, descriptor: stringDesc()),
                        FieldAccess(offset: MemoryLayout<(String, [UInt8])>.offset(of: \.1)!, descriptor: bytesDesc()),
                    ],
                    payloadLayout: MemoryLayout<(String, [UInt8])>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 2,
                    payloadFields: [FieldAccess(offset: 0, descriptor: stringDesc())],
                    payloadLayout: MemoryLayout<String>.phonLayout
                ),
            ]
        ))
    )
}

private func hotmealLiveReloadEventListDesc() -> Descriptor {
    listDesc(HotmealSchema.liveReloadEventList, HotmealLiveReloadEvent.self, element: hotmealLiveReloadEventDesc())
}

private func hotmealSubscribeRequestDesc() -> Descriptor {
    recordDesc(HotmealSchema.subscribeRequest, HotmealSubscribeRequest.self, fields: [
        fieldAccess(\HotmealSubscribeRequest.route, stringDesc()),
    ])
}

private func hotmealLiveReloadFixtureDescriptor() -> (root: Descriptor, registry: Registry) {
    let root = recordDesc(HotmealSchema.liveReloadFixture, HotmealLiveReloadFixture.self, fields: [
        fieldAccess(\HotmealLiveReloadFixture.subscribe, hotmealSubscribeRequestDesc()),
        fieldAccess(\HotmealLiveReloadFixture.events, hotmealLiveReloadEventListDesc()),
    ])
    return (root, Registry(hotmealSchemas()))
}

private func traceySchemas() -> [Schema] {
    [
        Schema(id: TraceySchema.optionString, kind: .option(element: .concrete(primitiveId(.string)))),
        Schema(id: TraceySchema.ruleId, kind: .structure(name: "TraceyRuleId", fields: [
            Field(name: "base", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "version", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: TraceySchema.ruleRef, kind: .structure(name: "TraceyRuleRef", fields: [
            Field(name: "id", schema: .concrete(TraceySchema.ruleId), required: true),
            Field(name: "text", schema: .concrete(TraceySchema.optionString), required: true),
        ])),
        Schema(id: TraceySchema.ruleRefList, kind: .list(element: .concrete(TraceySchema.ruleRef))),
        Schema(id: TraceySchema.sectionRules, kind: .structure(name: "TraceySectionRules", fields: [
            Field(name: "section", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "rules", schema: .concrete(TraceySchema.ruleRefList), required: true),
        ])),
        Schema(id: TraceySchema.sectionRulesList, kind: .list(element: .concrete(TraceySchema.sectionRules))),
        Schema(id: TraceySchema.uncoveredRequest, kind: .structure(name: "TraceyUncoveredRequest", fields: [
            Field(name: "spec", schema: .concrete(TraceySchema.optionString), required: true),
            Field(name: "impl_name", schema: .concrete(TraceySchema.optionString), required: true),
            Field(name: "prefix", schema: .concrete(TraceySchema.optionString), required: true),
        ])),
        Schema(id: TraceySchema.uncoveredResponse, kind: .structure(name: "TraceyUncoveredResponse", fields: [
            Field(name: "spec", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "impl_name", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "total_rules", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "uncovered_count", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "by_section", schema: .concrete(TraceySchema.sectionRulesList), required: true),
        ])),
        Schema(id: TraceySchema.implStatus, kind: .structure(name: "TraceyImplStatus", fields: [
            Field(name: "spec", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "impl_name", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "total_rules", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "covered_rules", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "stale_rules", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "verified_rules", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
        Schema(id: TraceySchema.implStatusList, kind: .list(element: .concrete(TraceySchema.implStatus))),
        Schema(id: TraceySchema.statusResponse, kind: .structure(name: "TraceyStatusResponse", fields: [
            Field(name: "impls", schema: .concrete(TraceySchema.implStatusList), required: true),
        ])),
        Schema(id: TraceySchema.coverageChange, kind: .structure(name: "TraceyCoverageChange", fields: [
            Field(name: "rule_id", schema: .concrete(TraceySchema.ruleId), required: true),
            Field(name: "file", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "line", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
        Schema(id: TraceySchema.coverageChangeList, kind: .list(element: .concrete(TraceySchema.coverageChange))),
        Schema(id: TraceySchema.ruleIdList, kind: .list(element: .concrete(TraceySchema.ruleId))),
        Schema(id: TraceySchema.deltaSummary, kind: .structure(name: "TraceyDeltaSummary", fields: [
            Field(name: "newly_covered", schema: .concrete(TraceySchema.coverageChangeList), required: true),
            Field(name: "newly_uncovered", schema: .concrete(TraceySchema.ruleIdList), required: true),
        ])),
        Schema(id: TraceySchema.optionDeltaSummary, kind: .option(element: .concrete(TraceySchema.deltaSummary))),
        Schema(id: TraceySchema.dataUpdate, kind: .structure(name: "TraceyDataUpdate", fields: [
            Field(name: "version", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "delta", schema: .concrete(TraceySchema.optionDeltaSummary), required: true),
        ])),
        Schema(id: TraceySchema.lspDiagnostic, kind: .structure(name: "TraceyLspDiagnostic", fields: [
            Field(name: "severity", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "code", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "message", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "start_line", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "start_char", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "end_line", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "end_char", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: TraceySchema.lspDiagnosticList, kind: .list(element: .concrete(TraceySchema.lspDiagnostic))),
        Schema(id: TraceySchema.lspFileDiagnostics, kind: .structure(name: "TraceyLspFileDiagnostics", fields: [
            Field(name: "path", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "diagnostics", schema: .concrete(TraceySchema.lspDiagnosticList), required: true),
        ])),
        Schema(id: TraceySchema.lspFileDiagnosticsList, kind: .list(element: .concrete(TraceySchema.lspFileDiagnostics))),
        Schema(id: TraceySchema.lspSymbol, kind: .structure(name: "TraceyLspSymbol", fields: [
            Field(name: "name", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "kind", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "path", schema: .concrete(TraceySchema.optionString), required: true),
            Field(name: "start_line", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "start_char", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "end_line", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "end_char", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: TraceySchema.lspSymbolList, kind: .list(element: .concrete(TraceySchema.lspSymbol))),
        Schema(id: TraceySchema.migrationFixture, kind: .structure(name: "TraceyMigrationFixture", fields: [
            Field(name: "status", schema: .concrete(TraceySchema.statusResponse), required: true),
            Field(name: "uncovered_request", schema: .concrete(TraceySchema.uncoveredRequest), required: true),
            Field(name: "uncovered_response", schema: .concrete(TraceySchema.uncoveredResponse), required: true),
            Field(name: "data_update_item", schema: .concrete(TraceySchema.dataUpdate), required: true),
            Field(name: "workspace_diagnostics", schema: .concrete(TraceySchema.lspFileDiagnosticsList), required: true),
            Field(name: "workspace_symbols", schema: .concrete(TraceySchema.lspSymbolList), required: true),
        ])),
    ]
}

private func traceyOptionStringDesc() -> Descriptor {
    optionDesc(TraceySchema.optionString, String.self, some: stringDesc())
}

private func traceyRuleIdDesc() -> Descriptor {
    recordDesc(TraceySchema.ruleId, TraceyRuleId.self, fields: [
        fieldAccess(\TraceyRuleId.base, stringDesc()),
        fieldAccess(\TraceyRuleId.version, scalarDesc(.u32)),
    ])
}

private func traceyRuleRefDesc() -> Descriptor {
    recordDesc(TraceySchema.ruleRef, TraceyRuleRef.self, fields: [
        fieldAccess(\TraceyRuleRef.id, traceyRuleIdDesc()),
        fieldAccess(\TraceyRuleRef.text, traceyOptionStringDesc()),
    ])
}

private func traceyRuleRefListDesc() -> Descriptor {
    listDesc(TraceySchema.ruleRefList, TraceyRuleRef.self, element: traceyRuleRefDesc())
}

private func traceySectionRulesDesc() -> Descriptor {
    recordDesc(TraceySchema.sectionRules, TraceySectionRules.self, fields: [
        fieldAccess(\TraceySectionRules.section, stringDesc()),
        fieldAccess(\TraceySectionRules.rules, traceyRuleRefListDesc()),
    ])
}

private func traceySectionRulesListDesc() -> Descriptor {
    listDesc(TraceySchema.sectionRulesList, TraceySectionRules.self, element: traceySectionRulesDesc())
}

private func traceyUncoveredRequestDesc() -> Descriptor {
    recordDesc(TraceySchema.uncoveredRequest, TraceyUncoveredRequest.self, fields: [
        fieldAccess(\TraceyUncoveredRequest.spec, traceyOptionStringDesc()),
        fieldAccess(\TraceyUncoveredRequest.implName, traceyOptionStringDesc()),
        fieldAccess(\TraceyUncoveredRequest.prefix, traceyOptionStringDesc()),
    ])
}

private func traceyUncoveredResponseDesc() -> Descriptor {
    recordDesc(TraceySchema.uncoveredResponse, TraceyUncoveredResponse.self, fields: [
        fieldAccess(\TraceyUncoveredResponse.spec, stringDesc()),
        fieldAccess(\TraceyUncoveredResponse.implName, stringDesc()),
        fieldAccess(\TraceyUncoveredResponse.totalRules, scalarDesc(.u64)),
        fieldAccess(\TraceyUncoveredResponse.uncoveredCount, scalarDesc(.u64)),
        fieldAccess(\TraceyUncoveredResponse.bySection, traceySectionRulesListDesc()),
    ])
}

private func traceyImplStatusDesc() -> Descriptor {
    recordDesc(TraceySchema.implStatus, TraceyImplStatus.self, fields: [
        fieldAccess(\TraceyImplStatus.spec, stringDesc()),
        fieldAccess(\TraceyImplStatus.implName, stringDesc()),
        fieldAccess(\TraceyImplStatus.totalRules, scalarDesc(.u64)),
        fieldAccess(\TraceyImplStatus.coveredRules, scalarDesc(.u64)),
        fieldAccess(\TraceyImplStatus.staleRules, scalarDesc(.u64)),
        fieldAccess(\TraceyImplStatus.verifiedRules, scalarDesc(.u64)),
    ])
}

private func traceyImplStatusListDesc() -> Descriptor {
    listDesc(TraceySchema.implStatusList, TraceyImplStatus.self, element: traceyImplStatusDesc())
}

private func traceyStatusResponseDesc() -> Descriptor {
    recordDesc(TraceySchema.statusResponse, TraceyStatusResponse.self, fields: [
        fieldAccess(\TraceyStatusResponse.impls, traceyImplStatusListDesc()),
    ])
}

private func traceyCoverageChangeDesc() -> Descriptor {
    recordDesc(TraceySchema.coverageChange, TraceyCoverageChange.self, fields: [
        fieldAccess(\TraceyCoverageChange.ruleId, traceyRuleIdDesc()),
        fieldAccess(\TraceyCoverageChange.file, stringDesc()),
        fieldAccess(\TraceyCoverageChange.line, scalarDesc(.u64)),
    ])
}

private func traceyCoverageChangeListDesc() -> Descriptor {
    listDesc(TraceySchema.coverageChangeList, TraceyCoverageChange.self, element: traceyCoverageChangeDesc())
}

private func traceyRuleIdListDesc() -> Descriptor {
    listDesc(TraceySchema.ruleIdList, TraceyRuleId.self, element: traceyRuleIdDesc())
}

private func traceyDeltaSummaryDesc() -> Descriptor {
    recordDesc(TraceySchema.deltaSummary, TraceyDeltaSummary.self, fields: [
        fieldAccess(\TraceyDeltaSummary.newlyCovered, traceyCoverageChangeListDesc()),
        fieldAccess(\TraceyDeltaSummary.newlyUncovered, traceyRuleIdListDesc()),
    ])
}

private func traceyOptionDeltaSummaryDesc() -> Descriptor {
    optionDesc(TraceySchema.optionDeltaSummary, TraceyDeltaSummary.self, some: traceyDeltaSummaryDesc())
}

private func traceyDataUpdateDesc() -> Descriptor {
    recordDesc(TraceySchema.dataUpdate, TraceyDataUpdate.self, fields: [
        fieldAccess(\TraceyDataUpdate.version, scalarDesc(.u64)),
        fieldAccess(\TraceyDataUpdate.delta, traceyOptionDeltaSummaryDesc()),
    ])
}

private func traceyLspDiagnosticDesc() -> Descriptor {
    recordDesc(TraceySchema.lspDiagnostic, TraceyLspDiagnostic.self, fields: [
        fieldAccess(\TraceyLspDiagnostic.severity, stringDesc()),
        fieldAccess(\TraceyLspDiagnostic.code, stringDesc()),
        fieldAccess(\TraceyLspDiagnostic.message, stringDesc()),
        fieldAccess(\TraceyLspDiagnostic.startLine, scalarDesc(.u32)),
        fieldAccess(\TraceyLspDiagnostic.startChar, scalarDesc(.u32)),
        fieldAccess(\TraceyLspDiagnostic.endLine, scalarDesc(.u32)),
        fieldAccess(\TraceyLspDiagnostic.endChar, scalarDesc(.u32)),
    ])
}

private func traceyLspDiagnosticListDesc() -> Descriptor {
    listDesc(TraceySchema.lspDiagnosticList, TraceyLspDiagnostic.self, element: traceyLspDiagnosticDesc())
}

private func traceyLspFileDiagnosticsDesc() -> Descriptor {
    recordDesc(TraceySchema.lspFileDiagnostics, TraceyLspFileDiagnostics.self, fields: [
        fieldAccess(\TraceyLspFileDiagnostics.path, stringDesc()),
        fieldAccess(\TraceyLspFileDiagnostics.diagnostics, traceyLspDiagnosticListDesc()),
    ])
}

private func traceyLspFileDiagnosticsListDesc() -> Descriptor {
    listDesc(TraceySchema.lspFileDiagnosticsList, TraceyLspFileDiagnostics.self, element: traceyLspFileDiagnosticsDesc())
}

private func traceyLspSymbolDesc() -> Descriptor {
    recordDesc(TraceySchema.lspSymbol, TraceyLspSymbol.self, fields: [
        fieldAccess(\TraceyLspSymbol.name, stringDesc()),
        fieldAccess(\TraceyLspSymbol.kind, stringDesc()),
        fieldAccess(\TraceyLspSymbol.path, traceyOptionStringDesc()),
        fieldAccess(\TraceyLspSymbol.startLine, scalarDesc(.u32)),
        fieldAccess(\TraceyLspSymbol.startChar, scalarDesc(.u32)),
        fieldAccess(\TraceyLspSymbol.endLine, scalarDesc(.u32)),
        fieldAccess(\TraceyLspSymbol.endChar, scalarDesc(.u32)),
    ])
}

private func traceyLspSymbolListDesc() -> Descriptor {
    listDesc(TraceySchema.lspSymbolList, TraceyLspSymbol.self, element: traceyLspSymbolDesc())
}

private func traceyMigrationFixtureDescriptor() -> (root: Descriptor, registry: Registry) {
    let root = recordDesc(TraceySchema.migrationFixture, TraceyMigrationFixture.self, fields: [
        fieldAccess(\TraceyMigrationFixture.status, traceyStatusResponseDesc()),
        fieldAccess(\TraceyMigrationFixture.uncoveredRequest, traceyUncoveredRequestDesc()),
        fieldAccess(\TraceyMigrationFixture.uncoveredResponse, traceyUncoveredResponseDesc()),
        fieldAccess(\TraceyMigrationFixture.dataUpdateItem, traceyDataUpdateDesc()),
        fieldAccess(\TraceyMigrationFixture.workspaceDiagnostics, traceyLspFileDiagnosticsListDesc()),
        fieldAccess(\TraceyMigrationFixture.workspaceSymbols, traceyLspSymbolListDesc()),
    ])
    return (root, Registry(traceySchemas()))
}

private func helixSchemas() -> [Schema] {
    [
        Schema(id: HelixSchema.optionU64, kind: .option(element: .concrete(primitiveId(.u64)))),
        Schema(id: HelixSchema.optionString, kind: .option(element: .concrete(primitiveId(.string)))),
        Schema(id: HelixSchema.u64List, kind: .list(element: .concrete(primitiveId(.u64)))),
        Schema(id: HelixSchema.f32List, kind: .list(element: .concrete(primitiveId(.f32)))),
        Schema(id: HelixSchema.f64List, kind: .list(element: .concrete(primitiveId(.f64)))),
        Schema(id: HelixSchema.stringList, kind: .list(element: .concrete(primitiveId(.string)))),
        Schema(id: HelixSchema.audioTokenRange, kind: .structure(name: "HelixAudioTokenRange", fields: [
            Field(name: "start", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "end", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: HelixSchema.audioRepresentationSpan, kind: .structure(name: "HelixAudioRepresentationSpan", fields: [
            Field(name: "audio", schema: .concrete(HelixSchema.audioTokenRange), required: true),
            Field(name: "audio_representation_version", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: HelixSchema.audioRepresentationSpanList, kind: .list(element: .concrete(HelixSchema.audioRepresentationSpan))),
        Schema(id: HelixSchema.streamMeta, kind: .structure(name: "HelixStreamMeta", fields: [
            Field(name: "schema_version", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "pulse_ids", schema: .concrete(HelixSchema.u64List), required: true),
            Field(name: "timeline_event_count", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "attention_batch_count", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
        Schema(id: HelixSchema.verifyOutcome, kind: .structure(name: "HelixVerifyOutcome", fields: [
            Field(name: "rewind_k", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "accepted_prefix_len", schema: .concrete(HelixSchema.optionU64), required: true),
            Field(name: "divergence_row", schema: .concrete(HelixSchema.optionU64), required: true),
            Field(name: "discarded_speculative_tokens", schema: .concrete(HelixSchema.optionU64), required: true),
        ])),
        Schema(id: HelixSchema.optionVerifyOutcome, kind: .option(element: .concrete(HelixSchema.verifyOutcome))),
        Schema(id: HelixSchema.pulseRollup, kind: .structure(name: "HelixPulseRollup", fields: [
            Field(name: "pulse_id", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "pulse_start_us", schema: .concrete(HelixSchema.optionU64), required: true),
            Field(name: "pulse_duration_us", schema: .concrete(HelixSchema.optionU64), required: true),
            Field(name: "encoder_duration_us", schema: .concrete(HelixSchema.optionU64), required: true),
            Field(name: "refresh_duration_us", schema: .concrete(HelixSchema.optionU64), required: true),
            Field(name: "verify_duration_us", schema: .concrete(HelixSchema.optionU64), required: true),
            Field(name: "decode_duration_us", schema: .concrete(HelixSchema.optionU64), required: true),
            Field(name: "commit_duration_us", schema: .concrete(HelixSchema.optionU64), required: true),
            Field(name: "pulse_mel_frames", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "committed_tokens", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "retained_speculative_tokens", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "resident_committed_tokens", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "evicted_audio_tokens", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "evicted_committed_tokens", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "decoded_tokens", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "hit_eos", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "verify", schema: .concrete(HelixSchema.optionVerifyOutcome), required: true),
            Field(name: "has_attention_batch", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "ar_token_count", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
        Schema(id: HelixSchema.optionPulseRollup, kind: .option(element: .concrete(HelixSchema.pulseRollup))),
        Schema(id: HelixSchema.textTokenSnapshot, kind: .structure(name: "HelixTextTokenSnapshot", fields: [
            Field(name: "text_token_id", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "text", schema: .concrete(HelixSchema.optionString), required: true),
            Field(name: "text_before", schema: .concrete(HelixSchema.optionString), required: true),
            Field(name: "in_verify_batch", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "decoded_this_pulse", schema: .concrete(primitiveId(.bool)), required: true),
        ])),
        Schema(id: HelixSchema.textTokenSnapshotList, kind: .list(element: .concrete(HelixSchema.textTokenSnapshot))),
        Schema(id: HelixSchema.promptLayout, kind: .structure(name: "HelixPromptLayout", fields: [
            Field(name: "pulse_id", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "first_audio_token_id", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "resident_audio_frames", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "changed_audio_spans", schema: .concrete(HelixSchema.audioRepresentationSpanList), required: true),
            Field(name: "text_token_start", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "text_token_end", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "text_tokens", schema: .concrete(HelixSchema.textTokenSnapshotList), required: true),
        ])),
        Schema(id: HelixSchema.optionPromptLayout, kind: .option(element: .concrete(HelixSchema.promptLayout))),
        Schema(id: HelixSchema.attentionHeatmap, kind: .structure(name: "HelixPulseAttentionHeatmap", fields: [
            Field(name: "pulse_id", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "first_audio_token_id", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "audio_token_count", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "text_token_start", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "text_token_count", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "record_count", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "max_value", schema: .concrete(primitiveId(.f32)), required: true),
            Field(name: "mean_audio_mass", schema: .concrete(HelixSchema.f32List), required: true),
            Field(name: "text_token_glyphs", schema: .concrete(HelixSchema.stringList), required: true),
        ])),
        Schema(id: HelixSchema.optionAttentionHeatmap, kind: .option(element: .concrete(HelixSchema.attentionHeatmap))),
        Schema(id: HelixSchema.streamMetrics, kind: .structure(name: "HelixStreamMetrics", fields: [
            Field(name: "pulse_ids", schema: .concrete(HelixSchema.u64List), required: true),
            Field(name: "pulse_duration_us", schema: .concrete(HelixSchema.u64List), required: true),
            Field(name: "decoded_tokens", schema: .concrete(HelixSchema.u64List), required: true),
            Field(name: "committed_tokens", schema: .concrete(HelixSchema.u64List), required: true),
            Field(name: "retained_speculative_tokens", schema: .concrete(HelixSchema.u64List), required: true),
            Field(name: "evicted_audio_tokens", schema: .concrete(HelixSchema.u64List), required: true),
            Field(name: "evicted_committed_tokens", schema: .concrete(HelixSchema.u64List), required: true),
            Field(name: "rewind_k", schema: .concrete(HelixSchema.u64List), required: true),
            Field(name: "ar_token_count", schema: .concrete(HelixSchema.u64List), required: true),
            Field(name: "rolling_wer", schema: .concrete(HelixSchema.f64List), required: true),
            Field(name: "s2d_p50_ms", schema: .concrete(HelixSchema.f64List), required: true),
        ])),
        Schema(id: HelixSchema.pulseAvailable, kind: .structure(name: "HelixPulseAvailable", fields: [
            Field(name: "pulse_id", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
        Schema(id: HelixSchema.runInfo, kind: .structure(name: "HelixRunInfo", fields: [
            Field(name: "backend", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "model_dir", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "input", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "piece", schema: .concrete(HelixSchema.optionString), required: true),
            Field(name: "pulse_ms", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "audio_ring_capacity", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "text_ring_capacity", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "commit_revisable_tail_text_tokens", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "revise_logit_margin", schema: .concrete(primitiveId(.f32)), required: true),
            Field(name: "sample_rate", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "mel_hop_samples", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "num_mel_bins", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "num_mel_frames", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "audio_tokens_per_chunk", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "native_window_tokens", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "realtime_pacing", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "profile_phases", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "attention_trace_schema_version", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "trace_server_schema_version", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: HelixSchema.traceSnapshot, kind: .structure(name: "HelixTraceSnapshot", fields: [
            Field(name: "meta", schema: .concrete(HelixSchema.streamMeta), required: true),
            Field(name: "run_info", schema: .concrete(HelixSchema.runInfo), required: true),
            Field(name: "rollup", schema: .concrete(HelixSchema.optionPulseRollup), required: true),
            Field(name: "prompt_layout", schema: .concrete(HelixSchema.optionPromptLayout), required: true),
            Field(name: "attention_heatmap", schema: .concrete(HelixSchema.optionAttentionHeatmap), required: true),
            Field(name: "stream_metrics", schema: .concrete(HelixSchema.streamMetrics), required: true),
            Field(name: "pulse_available", schema: .concrete(HelixSchema.pulseAvailable), required: true),
        ])),
    ]
}

private func helixOptionU64Desc() -> Descriptor {
    optionDesc(HelixSchema.optionU64, UInt64.self, some: scalarDesc(.u64))
}

private func helixOptionStringDesc() -> Descriptor {
    optionDesc(HelixSchema.optionString, String.self, some: stringDesc())
}

private func helixU64ListDesc() -> Descriptor {
    listDesc(HelixSchema.u64List, UInt64.self, element: scalarDesc(.u64))
}

private func helixF32ListDesc() -> Descriptor {
    listDesc(HelixSchema.f32List, Float.self, element: scalarDesc(.f32))
}

private func helixF64ListDesc() -> Descriptor {
    listDesc(HelixSchema.f64List, Double.self, element: scalarDesc(.f64))
}

private func helixStringListDesc() -> Descriptor {
    listDesc(HelixSchema.stringList, String.self, element: stringDesc())
}

private func helixAudioTokenRangeDesc() -> Descriptor {
    recordDesc(HelixSchema.audioTokenRange, HelixAudioTokenRange.self, fields: [
        fieldAccess(\HelixAudioTokenRange.start, scalarDesc(.u32)),
        fieldAccess(\HelixAudioTokenRange.end, scalarDesc(.u32)),
    ])
}

private func helixAudioRepresentationSpanDesc() -> Descriptor {
    recordDesc(HelixSchema.audioRepresentationSpan, HelixAudioRepresentationSpan.self, fields: [
        fieldAccess(\HelixAudioRepresentationSpan.audio, helixAudioTokenRangeDesc()),
        fieldAccess(\HelixAudioRepresentationSpan.audioRepresentationVersion, scalarDesc(.u32)),
    ])
}

private func helixAudioRepresentationSpanListDesc() -> Descriptor {
    listDesc(HelixSchema.audioRepresentationSpanList, HelixAudioRepresentationSpan.self, element: helixAudioRepresentationSpanDesc())
}

private func helixStreamMetaDesc() -> Descriptor {
    recordDesc(HelixSchema.streamMeta, HelixStreamMeta.self, fields: [
        fieldAccess(\HelixStreamMeta.schemaVersion, scalarDesc(.u32)),
        fieldAccess(\HelixStreamMeta.pulseIds, helixU64ListDesc()),
        fieldAccess(\HelixStreamMeta.timelineEventCount, scalarDesc(.u64)),
        fieldAccess(\HelixStreamMeta.attentionBatchCount, scalarDesc(.u64)),
    ])
}

private func helixVerifyOutcomeDesc() -> Descriptor {
    recordDesc(HelixSchema.verifyOutcome, HelixVerifyOutcome.self, fields: [
        fieldAccess(\HelixVerifyOutcome.rewindK, scalarDesc(.u64)),
        fieldAccess(\HelixVerifyOutcome.acceptedPrefixLen, helixOptionU64Desc()),
        fieldAccess(\HelixVerifyOutcome.divergenceRow, helixOptionU64Desc()),
        fieldAccess(\HelixVerifyOutcome.discardedSpeculativeTokens, helixOptionU64Desc()),
    ])
}

private func helixOptionVerifyOutcomeDesc() -> Descriptor {
    optionDesc(HelixSchema.optionVerifyOutcome, HelixVerifyOutcome.self, some: helixVerifyOutcomeDesc())
}

private func helixPulseRollupDesc() -> Descriptor {
    recordDesc(HelixSchema.pulseRollup, HelixPulseRollup.self, fields: [
        fieldAccess(\HelixPulseRollup.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.pulseStartUs, helixOptionU64Desc()),
        fieldAccess(\HelixPulseRollup.pulseDurationUs, helixOptionU64Desc()),
        fieldAccess(\HelixPulseRollup.encoderDurationUs, helixOptionU64Desc()),
        fieldAccess(\HelixPulseRollup.refreshDurationUs, helixOptionU64Desc()),
        fieldAccess(\HelixPulseRollup.verifyDurationUs, helixOptionU64Desc()),
        fieldAccess(\HelixPulseRollup.decodeDurationUs, helixOptionU64Desc()),
        fieldAccess(\HelixPulseRollup.commitDurationUs, helixOptionU64Desc()),
        fieldAccess(\HelixPulseRollup.pulseMelFrames, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.committedTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.retainedSpeculativeTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.residentCommittedTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.evictedAudioTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.evictedCommittedTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.decodedTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.hitEos, scalarDesc(.bool)),
        fieldAccess(\HelixPulseRollup.verify, helixOptionVerifyOutcomeDesc()),
        fieldAccess(\HelixPulseRollup.hasAttentionBatch, scalarDesc(.bool)),
        fieldAccess(\HelixPulseRollup.arTokenCount, scalarDesc(.u64)),
    ])
}

private func helixOptionPulseRollupDesc() -> Descriptor {
    optionDesc(HelixSchema.optionPulseRollup, HelixPulseRollup.self, some: helixPulseRollupDesc())
}

private func helixTextTokenSnapshotDesc() -> Descriptor {
    recordDesc(HelixSchema.textTokenSnapshot, HelixTextTokenSnapshot.self, fields: [
        fieldAccess(\HelixTextTokenSnapshot.textTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixTextTokenSnapshot.text, helixOptionStringDesc()),
        fieldAccess(\HelixTextTokenSnapshot.textBefore, helixOptionStringDesc()),
        fieldAccess(\HelixTextTokenSnapshot.inVerifyBatch, scalarDesc(.bool)),
        fieldAccess(\HelixTextTokenSnapshot.decodedThisPulse, scalarDesc(.bool)),
    ])
}

private func helixTextTokenSnapshotListDesc() -> Descriptor {
    listDesc(HelixSchema.textTokenSnapshotList, HelixTextTokenSnapshot.self, element: helixTextTokenSnapshotDesc())
}

private func helixPromptLayoutDesc() -> Descriptor {
    recordDesc(HelixSchema.promptLayout, HelixPromptLayout.self, fields: [
        fieldAccess(\HelixPromptLayout.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixPromptLayout.firstAudioTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixPromptLayout.residentAudioFrames, scalarDesc(.u64)),
        fieldAccess(\HelixPromptLayout.changedAudioSpans, helixAudioRepresentationSpanListDesc()),
        fieldAccess(\HelixPromptLayout.textTokenStart, scalarDesc(.u32)),
        fieldAccess(\HelixPromptLayout.textTokenEnd, scalarDesc(.u32)),
        fieldAccess(\HelixPromptLayout.textTokens, helixTextTokenSnapshotListDesc()),
    ])
}

private func helixOptionPromptLayoutDesc() -> Descriptor {
    optionDesc(HelixSchema.optionPromptLayout, HelixPromptLayout.self, some: helixPromptLayoutDesc())
}

private func helixPulseAttentionHeatmapDesc() -> Descriptor {
    recordDesc(HelixSchema.attentionHeatmap, HelixPulseAttentionHeatmap.self, fields: [
        fieldAccess(\HelixPulseAttentionHeatmap.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixPulseAttentionHeatmap.firstAudioTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixPulseAttentionHeatmap.audioTokenCount, scalarDesc(.u32)),
        fieldAccess(\HelixPulseAttentionHeatmap.textTokenStart, scalarDesc(.u32)),
        fieldAccess(\HelixPulseAttentionHeatmap.textTokenCount, scalarDesc(.u32)),
        fieldAccess(\HelixPulseAttentionHeatmap.recordCount, scalarDesc(.u32)),
        fieldAccess(\HelixPulseAttentionHeatmap.maxValue, scalarDesc(.f32)),
        fieldAccess(\HelixPulseAttentionHeatmap.meanAudioMass, helixF32ListDesc()),
        fieldAccess(\HelixPulseAttentionHeatmap.textTokenGlyphs, helixStringListDesc()),
    ])
}

private func helixOptionAttentionHeatmapDesc() -> Descriptor {
    optionDesc(HelixSchema.optionAttentionHeatmap, HelixPulseAttentionHeatmap.self, some: helixPulseAttentionHeatmapDesc())
}

private func helixStreamMetricsDesc() -> Descriptor {
    recordDesc(HelixSchema.streamMetrics, HelixStreamMetrics.self, fields: [
        fieldAccess(\HelixStreamMetrics.pulseIds, helixU64ListDesc()),
        fieldAccess(\HelixStreamMetrics.pulseDurationUs, helixU64ListDesc()),
        fieldAccess(\HelixStreamMetrics.decodedTokens, helixU64ListDesc()),
        fieldAccess(\HelixStreamMetrics.committedTokens, helixU64ListDesc()),
        fieldAccess(\HelixStreamMetrics.retainedSpeculativeTokens, helixU64ListDesc()),
        fieldAccess(\HelixStreamMetrics.evictedAudioTokens, helixU64ListDesc()),
        fieldAccess(\HelixStreamMetrics.evictedCommittedTokens, helixU64ListDesc()),
        fieldAccess(\HelixStreamMetrics.rewindK, helixU64ListDesc()),
        fieldAccess(\HelixStreamMetrics.arTokenCount, helixU64ListDesc()),
        fieldAccess(\HelixStreamMetrics.rollingWer, helixF64ListDesc()),
        fieldAccess(\HelixStreamMetrics.s2dP50Ms, helixF64ListDesc()),
    ])
}

private func helixPulseAvailableDesc() -> Descriptor {
    recordDesc(HelixSchema.pulseAvailable, HelixPulseAvailable.self, fields: [
        fieldAccess(\HelixPulseAvailable.pulseId, scalarDesc(.u64)),
    ])
}

private func helixRunInfoDesc() -> Descriptor {
    recordDesc(HelixSchema.runInfo, HelixRunInfo.self, fields: [
        fieldAccess(\HelixRunInfo.backend, stringDesc()),
        fieldAccess(\HelixRunInfo.modelDir, stringDesc()),
        fieldAccess(\HelixRunInfo.input, stringDesc()),
        fieldAccess(\HelixRunInfo.piece, helixOptionStringDesc()),
        fieldAccess(\HelixRunInfo.pulseMs, scalarDesc(.u32)),
        fieldAccess(\HelixRunInfo.audioRingCapacity, scalarDesc(.u32)),
        fieldAccess(\HelixRunInfo.textRingCapacity, scalarDesc(.u32)),
        fieldAccess(\HelixRunInfo.commitRevisableTailTextTokens, scalarDesc(.u32)),
        fieldAccess(\HelixRunInfo.reviseLogitMargin, scalarDesc(.f32)),
        fieldAccess(\HelixRunInfo.sampleRate, scalarDesc(.u32)),
        fieldAccess(\HelixRunInfo.melHopSamples, scalarDesc(.u32)),
        fieldAccess(\HelixRunInfo.numMelBins, scalarDesc(.u32)),
        fieldAccess(\HelixRunInfo.numMelFrames, scalarDesc(.u32)),
        fieldAccess(\HelixRunInfo.audioTokensPerChunk, scalarDesc(.u32)),
        fieldAccess(\HelixRunInfo.nativeWindowTokens, scalarDesc(.u32)),
        fieldAccess(\HelixRunInfo.realtimePacing, scalarDesc(.bool)),
        fieldAccess(\HelixRunInfo.profilePhases, scalarDesc(.bool)),
        fieldAccess(\HelixRunInfo.attentionTraceSchemaVersion, scalarDesc(.u32)),
        fieldAccess(\HelixRunInfo.traceServerSchemaVersion, scalarDesc(.u32)),
    ])
}

private func helixTraceSnapshotDescriptor() -> (root: Descriptor, registry: Registry) {
    let root = recordDesc(HelixSchema.traceSnapshot, HelixTraceSnapshot.self, fields: [
        fieldAccess(\HelixTraceSnapshot.meta, helixStreamMetaDesc()),
        fieldAccess(\HelixTraceSnapshot.runInfo, helixRunInfoDesc()),
        fieldAccess(\HelixTraceSnapshot.rollup, helixOptionPulseRollupDesc()),
        fieldAccess(\HelixTraceSnapshot.promptLayout, helixOptionPromptLayoutDesc()),
        fieldAccess(\HelixTraceSnapshot.attentionHeatmap, helixOptionAttentionHeatmapDesc()),
        fieldAccess(\HelixTraceSnapshot.streamMetrics, helixStreamMetricsDesc()),
        fieldAccess(\HelixTraceSnapshot.pulseAvailable, helixPulseAvailableDesc()),
    ])
    return (root, Registry(helixSchemas()))
}

private func offCpuDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(StaxSchema.offCpu),
        layout: MemoryLayout<OffCpuBreakdown>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<OffCpuBreakdown>.offset(of: \OffCpuBreakdown.sleepNs)!, descriptor: scalarDesc(.u64)),
            FieldAccess(offset: MemoryLayout<OffCpuBreakdown>.offset(of: \OffCpuBreakdown.ioNs)!, descriptor: scalarDesc(.u64)),
            FieldAccess(offset: MemoryLayout<OffCpuBreakdown>.offset(of: \OffCpuBreakdown.mutexNs)!, descriptor: scalarDesc(.u64)),
        ], construct: .inPlace))
    )
}

private func staxSchemas() -> [Schema] {
    [
        Schema(id: StaxSchema.optionU32, kind: .option(element: .concrete(primitiveId(.u32)))),
        Schema(id: StaxSchema.offCpu, kind: .structure(name: "OffCpuBreakdown", fields: [
            Field(name: "sleep_ns", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "io_ns", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "mutex_ns", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
        Schema(id: StaxSchema.flameNode, kind: .structure(name: "FlameNode", fields: [
            Field(name: "address", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "function_name", schema: .concrete(StaxSchema.optionU32), required: true),
            Field(name: "binary", schema: .concrete(StaxSchema.optionU32), required: true),
            Field(name: "on_cpu_ns", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "off_cpu", schema: .concrete(StaxSchema.offCpu), required: true),
            Field(name: "children", schema: .concrete(StaxSchema.flameNodeList), required: true),
        ])),
        Schema(id: StaxSchema.flameNodeList, kind: .list(element: .concrete(StaxSchema.flameNode))),
        Schema(id: StaxSchema.stringList, kind: .list(element: .concrete(primitiveId(.string)))),
        Schema(id: StaxSchema.update, kind: .structure(name: "StaxFlamegraphUpdate", fields: [
            Field(name: "total_on_cpu_ns", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "strings", schema: .concrete(StaxSchema.stringList), required: true),
            Field(name: "root", schema: .concrete(StaxSchema.flameNode), required: true),
        ])),
        Schema(id: StaxSchema.linuxPerfSessionConfig, kind: .structure(name: "StaxLinuxPerfSessionConfig", fields: [
            Field(name: "target_pid", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "frequency_hz", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "kernel_stacks", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "request_waking", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "request_pmu", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "request_dwarf_unwind", schema: .concrete(primitiveId(.bool)), required: true),
        ])),
        Schema(id: StaxSchema.linuxWakingFieldOffsets, kind: .structure(name: "StaxLinuxWakingFieldOffsets", fields: [
            Field(name: "wakee_pid_offset", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "wakee_pid_size", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(
            id: StaxSchema.optionLinuxWakingFieldOffsets,
            kind: .option(element: .concrete(StaxSchema.linuxWakingFieldOffsets))
        ),
        Schema(id: StaxSchema.linuxPerfSessionError, kind: .enumeration(name: "StaxLinuxPerfSessionError", variants: [
            Variant(name: "NotPrivileged", index: 0, payload: .structure([
                Field(name: "detail", schema: .concrete(primitiveId(.string)), required: true),
            ])),
            Variant(name: "PerfEventOpen", index: 1, payload: .structure([
                Field(name: "cpu", schema: .concrete(primitiveId(.u32)), required: true),
                Field(name: "errno", schema: .concrete(primitiveId(.i32)), required: true),
                Field(name: "detail", schema: .concrete(primitiveId(.string)), required: true),
            ])),
            Variant(name: "NoSuchTarget", index: 2, payload: .newtype(.concrete(primitiveId(.u32)))),
            Variant(name: "NotAuthorized", index: 3, payload: .structure([
                Field(name: "caller_uid", schema: .concrete(primitiveId(.u32)), required: true),
                Field(name: "target_uid", schema: .concrete(primitiveId(.u32)), required: true),
            ])),
        ])),
        Schema(id: StaxSchema.linuxPerfSessionErrorList, kind: .list(element: .concrete(StaxSchema.linuxPerfSessionError))),
        Schema(id: StaxSchema.linuxDaemonStatus, kind: .structure(name: "StaxLinuxDaemonStatus", fields: [
            Field(name: "version", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "host_arch", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "privileged", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "perf_event_paranoid", schema: .concrete(primitiveId(.i32)), required: true),
        ])),
        Schema(id: StaxSchema.linuxBrokerControlFixture, kind: .structure(name: "StaxLinuxBrokerControlFixture", fields: [
            Field(name: "config", schema: .concrete(StaxSchema.linuxPerfSessionConfig), required: true),
            Field(name: "status", schema: .concrete(StaxSchema.linuxDaemonStatus), required: true),
            Field(name: "errors", schema: .concrete(StaxSchema.linuxPerfSessionErrorList), required: true),
            Field(
                name: "waking_field_offsets",
                schema: .concrete(StaxSchema.optionLinuxWakingFieldOffsets),
                required: true
            ),
        ])),
        Schema(id: StaxSchema.macKdBuf, kind: .structure(name: "StaxMacKdBuf", fields: [
            Field(name: "timestamp", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "arg1", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "arg2", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "arg3", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "arg4", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "arg5", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "debugid", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "cpuid", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "unused", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
        Schema(id: StaxSchema.macKdBufList, kind: .list(element: .concrete(StaxSchema.macKdBuf))),
        Schema(id: StaxSchema.macKdBufBatch, kind: .structure(name: "StaxMacKdBufBatch", fields: [
            Field(name: "records", schema: .concrete(StaxSchema.macKdBufList), required: true),
            Field(name: "read_started_mach_ticks", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "drained_mach_ticks", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "queued_for_send_mach_ticks", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "send_started_mach_ticks", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "drained_at_unix_ns", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
    ]
}

private func staxDescriptor() -> (root: Descriptor, registry: Registry, blocks: [SchemaId: Descriptor]) {
    let recurseNode = Descriptor(
        schema: .concrete(StaxSchema.flameNode),
        layout: MemoryLayout<FlameNode>.phonLayout,
        access: .recurse
    )
    let recurseNodeList = Descriptor(
        schema: .concrete(StaxSchema.flameNodeList),
        layout: MemoryLayout<[FlameNode]>.phonLayout,
        access: .recurse
    )

    let nodeBody = Descriptor(
        schema: .concrete(StaxSchema.flameNode),
        layout: MemoryLayout<FlameNode>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<FlameNode>.offset(of: \FlameNode.address)!, descriptor: scalarDesc(.u64)),
            FieldAccess(offset: MemoryLayout<FlameNode>.offset(of: \FlameNode.functionName)!, descriptor: optionU32Desc()),
            FieldAccess(offset: MemoryLayout<FlameNode>.offset(of: \FlameNode.binary)!, descriptor: optionU32Desc()),
            FieldAccess(offset: MemoryLayout<FlameNode>.offset(of: \FlameNode.onCpuNs)!, descriptor: scalarDesc(.u64)),
            FieldAccess(offset: MemoryLayout<FlameNode>.offset(of: \FlameNode.offCpu)!, descriptor: offCpuDesc()),
            FieldAccess(offset: MemoryLayout<FlameNode>.offset(of: \FlameNode.children)!, descriptor: recurseNodeList),
        ], construct: .inPlace))
    )
    let listBody = Descriptor(
        schema: .concrete(StaxSchema.flameNodeList),
        layout: MemoryLayout<[FlameNode]>.phonLayout,
        access: .sequence(SequenceAccess(
            element: recurseNode,
            stride: MemoryLayout<FlameNode>.stride,
            elemAlign: MemoryLayout<FlameNode>.alignment,
            witness: .of(FlameNode.self)
        ))
    )
    let root = Descriptor(
        schema: .concrete(StaxSchema.update),
        layout: MemoryLayout<StaxFlamegraphUpdate>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<StaxFlamegraphUpdate>.offset(of: \StaxFlamegraphUpdate.totalOnCpuNs)!, descriptor: scalarDesc(.u64)),
            FieldAccess(offset: MemoryLayout<StaxFlamegraphUpdate>.offset(of: \StaxFlamegraphUpdate.strings)!, descriptor: stringListDesc()),
            FieldAccess(offset: MemoryLayout<StaxFlamegraphUpdate>.offset(of: \StaxFlamegraphUpdate.root)!, descriptor: recurseNode),
        ], construct: .inPlace))
    )
    return (root, Registry(staxSchemas()), [
        StaxSchema.flameNode: nodeBody,
        StaxSchema.flameNodeList: listBody,
    ])
}

private func staxLinuxPerfSessionConfigDesc() -> Descriptor {
    recordDesc(StaxSchema.linuxPerfSessionConfig, StaxLinuxPerfSessionConfig.self, fields: [
        fieldAccess(\StaxLinuxPerfSessionConfig.targetPid, scalarDesc(.u32)),
        fieldAccess(\StaxLinuxPerfSessionConfig.frequencyHz, scalarDesc(.u32)),
        fieldAccess(\StaxLinuxPerfSessionConfig.kernelStacks, scalarDesc(.bool)),
        fieldAccess(\StaxLinuxPerfSessionConfig.requestWaking, scalarDesc(.bool)),
        fieldAccess(\StaxLinuxPerfSessionConfig.requestPmu, scalarDesc(.bool)),
        fieldAccess(\StaxLinuxPerfSessionConfig.requestDwarfUnwind, scalarDesc(.bool)),
    ])
}

private func staxLinuxWakingFieldOffsetsDesc() -> Descriptor {
    recordDesc(StaxSchema.linuxWakingFieldOffsets, StaxLinuxWakingFieldOffsets.self, fields: [
        fieldAccess(\StaxLinuxWakingFieldOffsets.wakeePidOffset, scalarDesc(.u32)),
        fieldAccess(\StaxLinuxWakingFieldOffsets.wakeePidSize, scalarDesc(.u32)),
    ])
}

private func staxOptionLinuxWakingFieldOffsetsDesc() -> Descriptor {
    optionDesc(
        StaxSchema.optionLinuxWakingFieldOffsets,
        StaxLinuxWakingFieldOffsets.self,
        some: staxLinuxWakingFieldOffsetsDesc()
    )
}

private func staxLinuxPerfSessionErrorDesc() -> Descriptor {
    let notPrivilegedDetailOffset = MemoryLayout<StaxNotPrivilegedPayload>.offset(of: \StaxNotPrivilegedPayload.detail)!
    let perfCpuOffset = MemoryLayout<StaxPerfEventOpenPayload>.offset(of: \StaxPerfEventOpenPayload.cpu)!
    let perfErrnoOffset = MemoryLayout<StaxPerfEventOpenPayload>.offset(of: \StaxPerfEventOpenPayload.errno)!
    let perfDetailOffset = MemoryLayout<StaxPerfEventOpenPayload>.offset(of: \StaxPerfEventOpenPayload.detail)!
    let callerUidOffset = MemoryLayout<StaxNotAuthorizedPayload>.offset(of: \StaxNotAuthorizedPayload.callerUid)!
    let targetUidOffset = MemoryLayout<StaxNotAuthorizedPayload>.offset(of: \StaxNotAuthorizedPayload.targetUid)!

    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: StaxLinuxPerfSessionError.self).pointee {
        case .notPrivileged: return 0
        case .perfEventOpen: return 1
        case .noSuchTarget: return 2
        case .notAuthorized: return 3
        }
    }
    let projectPayload: (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void = { value, _, scratch in
        switch value.assumingMemoryBound(to: StaxLinuxPerfSessionError.self).pointee {
        case .notPrivileged(let detail):
            scratch.advanced(by: notPrivilegedDetailOffset)
                .assumingMemoryBound(to: String.self)
                .initialize(to: detail)
        case .perfEventOpen(let cpu, let errno, let detail):
            scratch.advanced(by: perfCpuOffset).storeBytes(of: cpu, as: UInt32.self)
            scratch.advanced(by: perfErrnoOffset).storeBytes(of: errno, as: Int32.self)
            scratch.advanced(by: perfDetailOffset)
                .assumingMemoryBound(to: String.self)
                .initialize(to: detail)
        case .noSuchTarget(let pid):
            scratch.storeBytes(of: pid, as: UInt32.self)
        case .notAuthorized(let callerUid, let targetUid):
            scratch.advanced(by: callerUidOffset).storeBytes(of: callerUid, as: UInt32.self)
            scratch.advanced(by: targetUidOffset).storeBytes(of: targetUid, as: UInt32.self)
        }
    }
    let destroyPayload: (UnsafeMutableRawPointer, Int) -> Void = { scratch, localIndex in
        switch localIndex {
        case 0:
            scratch.advanced(by: notPrivilegedDetailOffset)
                .assumingMemoryBound(to: String.self)
                .deinitialize(count: 1)
        case 1:
            scratch.advanced(by: perfDetailOffset)
                .assumingMemoryBound(to: String.self)
                .deinitialize(count: 1)
        default:
            break
        }
    }
    let inject: (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void = { slot, localIndex, scratch in
        let error: StaxLinuxPerfSessionError
        switch localIndex {
        case 0:
            let detail = scratch.advanced(by: notPrivilegedDetailOffset)
                .assumingMemoryBound(to: String.self)
                .move()
            error = .notPrivileged(detail: detail)
        case 1:
            let cpu = scratch.advanced(by: perfCpuOffset).load(as: UInt32.self)
            let errno = scratch.advanced(by: perfErrnoOffset).load(as: Int32.self)
            let detail = scratch.advanced(by: perfDetailOffset)
                .assumingMemoryBound(to: String.self)
                .move()
            error = .perfEventOpen(cpu: cpu, errno: errno, detail: detail)
        case 2:
            error = .noSuchTarget(scratch.load(as: UInt32.self))
        case 3:
            let callerUid = scratch.advanced(by: callerUidOffset).load(as: UInt32.self)
            let targetUid = scratch.advanced(by: targetUidOffset).load(as: UInt32.self)
            error = .notAuthorized(callerUid: callerUid, targetUid: targetUid)
        default:
            fatalError("bad StaxLinuxPerfSessionError variant index")
        }
        slot.assumingMemoryBound(to: StaxLinuxPerfSessionError.self).initialize(to: error)
    }

    return Descriptor(
        schema: .concrete(StaxSchema.linuxPerfSessionError),
        layout: MemoryLayout<StaxLinuxPerfSessionError>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: projectPayload,
            destroyPayload: destroyPayload,
            inject: inject,
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [
                        FieldAccess(offset: notPrivilegedDetailOffset, descriptor: stringDesc()),
                    ],
                    payloadLayout: MemoryLayout<StaxNotPrivilegedPayload>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [
                        FieldAccess(offset: perfCpuOffset, descriptor: scalarDesc(.u32)),
                        FieldAccess(offset: perfErrnoOffset, descriptor: scalarDesc(.i32)),
                        FieldAccess(offset: perfDetailOffset, descriptor: stringDesc()),
                    ],
                    payloadLayout: MemoryLayout<StaxPerfEventOpenPayload>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 2,
                    payloadFields: [FieldAccess(offset: 0, descriptor: scalarDesc(.u32))],
                    payloadLayout: MemoryLayout<UInt32>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 3,
                    payloadFields: [
                        FieldAccess(offset: callerUidOffset, descriptor: scalarDesc(.u32)),
                        FieldAccess(offset: targetUidOffset, descriptor: scalarDesc(.u32)),
                    ],
                    payloadLayout: MemoryLayout<StaxNotAuthorizedPayload>.phonLayout
                ),
            ]
        ))
    )
}

private func staxLinuxPerfSessionErrorListDesc() -> Descriptor {
    listDesc(
        StaxSchema.linuxPerfSessionErrorList,
        StaxLinuxPerfSessionError.self,
        element: staxLinuxPerfSessionErrorDesc()
    )
}

private func staxLinuxDaemonStatusDesc() -> Descriptor {
    recordDesc(StaxSchema.linuxDaemonStatus, StaxLinuxDaemonStatus.self, fields: [
        fieldAccess(\StaxLinuxDaemonStatus.version, stringDesc()),
        fieldAccess(\StaxLinuxDaemonStatus.hostArch, stringDesc()),
        fieldAccess(\StaxLinuxDaemonStatus.privileged, scalarDesc(.bool)),
        fieldAccess(\StaxLinuxDaemonStatus.perfEventParanoid, scalarDesc(.i32)),
    ])
}

private func staxLinuxBrokerControlDescriptor() -> (root: Descriptor, registry: Registry) {
    let root = recordDesc(StaxSchema.linuxBrokerControlFixture, StaxLinuxBrokerControlFixture.self, fields: [
        fieldAccess(\StaxLinuxBrokerControlFixture.config, staxLinuxPerfSessionConfigDesc()),
        fieldAccess(\StaxLinuxBrokerControlFixture.status, staxLinuxDaemonStatusDesc()),
        fieldAccess(\StaxLinuxBrokerControlFixture.errors, staxLinuxPerfSessionErrorListDesc()),
        fieldAccess(\StaxLinuxBrokerControlFixture.wakingFieldOffsets, staxOptionLinuxWakingFieldOffsetsDesc()),
    ])
    return (root, Registry(staxSchemas()))
}

private func staxMacKdBufDesc() -> Descriptor {
    recordDesc(StaxSchema.macKdBuf, StaxMacKdBuf.self, fields: [
        fieldAccess(\StaxMacKdBuf.timestamp, scalarDesc(.u64)),
        fieldAccess(\StaxMacKdBuf.arg1, scalarDesc(.u64)),
        fieldAccess(\StaxMacKdBuf.arg2, scalarDesc(.u64)),
        fieldAccess(\StaxMacKdBuf.arg3, scalarDesc(.u64)),
        fieldAccess(\StaxMacKdBuf.arg4, scalarDesc(.u64)),
        fieldAccess(\StaxMacKdBuf.arg5, scalarDesc(.u64)),
        fieldAccess(\StaxMacKdBuf.debugid, scalarDesc(.u32)),
        fieldAccess(\StaxMacKdBuf.cpuid, scalarDesc(.u32)),
        fieldAccess(\StaxMacKdBuf.unused, scalarDesc(.u64)),
    ])
}

private func staxMacKdBufListDesc() -> Descriptor {
    listDesc(
        StaxSchema.macKdBufList,
        StaxMacKdBuf.self,
        element: staxMacKdBufDesc()
    )
}

private func staxMacKdBufBatchDescriptor() -> (root: Descriptor, registry: Registry) {
    let root = recordDesc(StaxSchema.macKdBufBatch, StaxMacKdBufBatch.self, fields: [
        fieldAccess(\StaxMacKdBufBatch.records, staxMacKdBufListDesc()),
        fieldAccess(\StaxMacKdBufBatch.readStartedMachTicks, scalarDesc(.u64)),
        fieldAccess(\StaxMacKdBufBatch.drainedMachTicks, scalarDesc(.u64)),
        fieldAccess(\StaxMacKdBufBatch.queuedForSendMachTicks, scalarDesc(.u64)),
        fieldAccess(\StaxMacKdBufBatch.sendStartedMachTicks, scalarDesc(.u64)),
        fieldAccess(\StaxMacKdBufBatch.drainedAtUnixNs, scalarDesc(.u64)),
    ])
    return (root, Registry(staxSchemas()))
}

private func optionDesc<Wrapped>(
    _ schema: SchemaId,
    _ type: Wrapped.Type,
    some: Descriptor
) -> Descriptor {
    Descriptor(
        schema: .concrete(schema),
        layout: MemoryLayout<Wrapped?>.phonLayout,
        access: .option(OptionAccess(witness: .of(Wrapped.self), some: some))
    )
}

private func listDesc<Element>(
    _ schema: SchemaId,
    _ type: Element.Type,
    element: Descriptor
) -> Descriptor {
    Descriptor(
        schema: .concrete(schema),
        layout: MemoryLayout<[Element]>.phonLayout,
        access: .sequence(SequenceAccess(
            element: element,
            stride: MemoryLayout<Element>.stride,
            elemAlign: MemoryLayout<Element>.alignment,
            witness: .of(Element.self)
        ))
    )
}

private func recordDesc<T>(
    _ schema: SchemaId,
    _ type: T.Type,
    fields: [FieldAccess]
) -> Descriptor {
    Descriptor(
        schema: .concrete(schema),
        layout: MemoryLayout<T>.phonLayout,
        access: .record(RecordAccess(fields: fields, construct: .inPlace))
    )
}

private func fieldAccess<Root, Field>(
    _ keyPath: KeyPath<Root, Field>,
    _ descriptor: Descriptor
) -> FieldAccess {
    FieldAccess(offset: MemoryLayout<Root>.offset(of: keyPath)!, descriptor: descriptor)
}

private func unitEnumDesc<T>(
    _ schema: SchemaId,
    _ type: T.Type,
    variantCount: Int,
    tag: @escaping (UnsafeRawPointer) -> Int,
    make: @escaping (Int) -> T
) -> Descriptor {
    Descriptor(
        schema: .concrete(schema),
        layout: MemoryLayout<T>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: { _, _, _ in },
            inject: { slot, localIndex, _ in
                slot.assumingMemoryBound(to: T.self).initialize(to: make(localIndex))
            },
            variants: (0 ..< variantCount).map { index in
                VariantAccess(wireIndex: UInt32(index), payloadFields: [], payloadLayout: Layout(size: 0, align: 1))
            }
        ))
    )
}

private func styxSchemas() -> [Schema] {
    [
        Schema(id: StyxSchema.optionSpan, kind: .option(element: .concrete(StyxSchema.span))),
        Schema(id: StyxSchema.optionString, kind: .option(element: .concrete(primitiveId(.string)))),
        Schema(id: StyxSchema.optionTag, kind: .option(element: .concrete(StyxSchema.tag))),
        Schema(id: StyxSchema.optionPayload, kind: .option(element: .concrete(StyxSchema.payload))),
        Schema(id: StyxSchema.optionValue, kind: .option(element: .concrete(StyxSchema.value))),
        Schema(id: StyxSchema.stringList, kind: .list(element: .concrete(primitiveId(.string)))),
        Schema(id: StyxSchema.optionU32, kind: .option(element: .concrete(primitiveId(.u32)))),
        Schema(id: StyxSchema.span, kind: .structure(name: "StyxSpan", fields: [
            Field(name: "start", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "end", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: StyxSchema.tag, kind: .structure(name: "StyxTag", fields: [
            Field(name: "name", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "span", schema: .concrete(StyxSchema.optionSpan), required: true),
        ])),
        Schema(id: StyxSchema.scalarKind, kind: .enumeration(name: "StyxScalarKind", variants: [
            Variant(name: "Bare", index: 0, payload: .unit),
            Variant(name: "Quoted", index: 1, payload: .unit),
            Variant(name: "Raw", index: 2, payload: .unit),
            Variant(name: "Heredoc", index: 3, payload: .unit),
        ])),
        Schema(id: StyxSchema.scalar, kind: .structure(name: "StyxScalar", fields: [
            Field(name: "text", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "kind", schema: .concrete(StyxSchema.scalarKind), required: true),
            Field(name: "span", schema: .concrete(StyxSchema.optionSpan), required: true),
        ])),
        Schema(id: StyxSchema.valueList, kind: .list(element: .concrete(StyxSchema.value))),
        Schema(id: StyxSchema.sequence, kind: .structure(name: "StyxSequence", fields: [
            Field(name: "items", schema: .concrete(StyxSchema.valueList), required: true),
            Field(name: "span", schema: .concrete(StyxSchema.optionSpan), required: true),
        ])),
        Schema(id: StyxSchema.entry, kind: .structure(name: "StyxEntry", fields: [
            Field(name: "key", schema: .concrete(StyxSchema.value), required: true),
            Field(name: "value", schema: .concrete(StyxSchema.value), required: true),
            Field(name: "doc_comment", schema: .concrete(StyxSchema.optionString), required: true),
        ])),
        Schema(id: StyxSchema.entryList, kind: .list(element: .concrete(StyxSchema.entry))),
        Schema(id: StyxSchema.object, kind: .structure(name: "StyxObject", fields: [
            Field(name: "entries", schema: .concrete(StyxSchema.entryList), required: true),
            Field(name: "span", schema: .concrete(StyxSchema.optionSpan), required: true),
        ])),
        Schema(id: StyxSchema.payload, kind: .enumeration(name: "StyxPayload", variants: [
            Variant(name: "Scalar", index: 0, payload: .newtype(.concrete(StyxSchema.scalar))),
            Variant(name: "Sequence", index: 1, payload: .newtype(.concrete(StyxSchema.sequence))),
            Variant(name: "Object", index: 2, payload: .newtype(.concrete(StyxSchema.object))),
        ])),
        Schema(id: StyxSchema.value, kind: .structure(name: "StyxValue", fields: [
            Field(name: "tag", schema: .concrete(StyxSchema.optionTag), required: true),
            Field(name: "payload", schema: .concrete(StyxSchema.optionPayload), required: true),
            Field(name: "span", schema: .concrete(StyxSchema.optionSpan), required: true),
        ])),
        Schema(id: StyxSchema.lspPosition, kind: .structure(name: "StyxLspPosition", fields: [
            Field(name: "line", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "character", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: StyxSchema.lspRange, kind: .structure(name: "StyxLspRange", fields: [
            Field(name: "start", schema: .concrete(StyxSchema.lspPosition), required: true),
            Field(name: "end", schema: .concrete(StyxSchema.lspPosition), required: true),
        ])),
        Schema(id: StyxSchema.lspCursor, kind: .structure(name: "StyxLspCursor", fields: [
            Field(name: "line", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "character", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "offset", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: StyxSchema.lspCapability, kind: .enumeration(name: "StyxLspCapability", variants: [
            Variant(name: "Completions", index: 0, payload: .unit),
            Variant(name: "Hover", index: 1, payload: .unit),
            Variant(name: "Diagnostics", index: 2, payload: .unit),
            Variant(name: "CodeActions", index: 3, payload: .unit),
            Variant(name: "Definition", index: 4, payload: .unit),
        ])),
        Schema(id: StyxSchema.lspCapabilityList, kind: .list(element: .concrete(StyxSchema.lspCapability))),
        Schema(id: StyxSchema.lspInitializeParams, kind: .structure(name: "StyxLspInitializeParams", fields: [
            Field(name: "styx_version", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "document_uri", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "schema_id", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: StyxSchema.lspInitializeResult, kind: .structure(name: "StyxLspInitializeResult", fields: [
            Field(name: "name", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "version", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "capabilities", schema: .concrete(StyxSchema.lspCapabilityList), required: true),
        ])),
        Schema(id: StyxSchema.lspCompletionKind, kind: .enumeration(name: "StyxLspCompletionKind", variants: [
            Variant(name: "Field", index: 0, payload: .unit),
            Variant(name: "Type", index: 1, payload: .unit),
            Variant(name: "Function", index: 2, payload: .unit),
            Variant(name: "Keyword", index: 3, payload: .unit),
        ])),
        Schema(id: StyxSchema.optionLspCompletionKind, kind: .option(element: .concrete(StyxSchema.lspCompletionKind))),
        Schema(id: StyxSchema.lspCompletionParams, kind: .structure(name: "StyxLspCompletionParams", fields: [
            Field(name: "document_uri", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "cursor", schema: .concrete(StyxSchema.lspCursor), required: true),
            Field(name: "path", schema: .concrete(StyxSchema.stringList), required: true),
            Field(name: "prefix", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "context", schema: .concrete(StyxSchema.optionValue), required: true),
            Field(name: "tagged_context", schema: .concrete(StyxSchema.optionValue), required: true),
        ])),
        Schema(id: StyxSchema.lspCompletionItem, kind: .structure(name: "StyxLspCompletionItem", fields: [
            Field(name: "label", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "detail", schema: .concrete(StyxSchema.optionString), required: true),
            Field(name: "documentation", schema: .concrete(StyxSchema.optionString), required: true),
            Field(name: "kind", schema: .concrete(StyxSchema.optionLspCompletionKind), required: true),
            Field(name: "sort_text", schema: .concrete(StyxSchema.optionString), required: true),
            Field(name: "insert_text", schema: .concrete(StyxSchema.optionString), required: true),
        ])),
        Schema(id: StyxSchema.lspCompletionItemList, kind: .list(element: .concrete(StyxSchema.lspCompletionItem))),
        Schema(id: StyxSchema.optionLspRange, kind: .option(element: .concrete(StyxSchema.lspRange))),
        Schema(id: StyxSchema.lspHoverParams, kind: .structure(name: "StyxLspHoverParams", fields: [
            Field(name: "document_uri", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "cursor", schema: .concrete(StyxSchema.lspCursor), required: true),
            Field(name: "path", schema: .concrete(StyxSchema.stringList), required: true),
            Field(name: "context", schema: .concrete(StyxSchema.optionValue), required: true),
            Field(name: "tagged_context", schema: .concrete(StyxSchema.optionValue), required: true),
        ])),
        Schema(id: StyxSchema.lspHoverResult, kind: .structure(name: "StyxLspHoverResult", fields: [
            Field(name: "contents", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "range", schema: .concrete(StyxSchema.optionLspRange), required: true),
        ])),
        Schema(id: StyxSchema.optionLspHoverResult, kind: .option(element: .concrete(StyxSchema.lspHoverResult))),
        Schema(id: StyxSchema.lspInlayHintKind, kind: .enumeration(name: "StyxLspInlayHintKind", variants: [
            Variant(name: "Type", index: 0, payload: .unit),
            Variant(name: "Parameter", index: 1, payload: .unit),
        ])),
        Schema(id: StyxSchema.optionLspInlayHintKind, kind: .option(element: .concrete(StyxSchema.lspInlayHintKind))),
        Schema(id: StyxSchema.lspInlayHintParams, kind: .structure(name: "StyxLspInlayHintParams", fields: [
            Field(name: "document_uri", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "range", schema: .concrete(StyxSchema.lspRange), required: true),
            Field(name: "context", schema: .concrete(StyxSchema.optionValue), required: true),
        ])),
        Schema(id: StyxSchema.lspInlayHint, kind: .structure(name: "StyxLspInlayHint", fields: [
            Field(name: "position", schema: .concrete(StyxSchema.lspPosition), required: true),
            Field(name: "label", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "kind", schema: .concrete(StyxSchema.optionLspInlayHintKind), required: true),
            Field(name: "padding_left", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "padding_right", schema: .concrete(primitiveId(.bool)), required: true),
        ])),
        Schema(id: StyxSchema.lspInlayHintList, kind: .list(element: .concrete(StyxSchema.lspInlayHint))),
        Schema(id: StyxSchema.lspDiagnosticSeverity, kind: .enumeration(name: "StyxLspDiagnosticSeverity", variants: [
            Variant(name: "Error", index: 0, payload: .unit),
            Variant(name: "Warning", index: 1, payload: .unit),
            Variant(name: "Information", index: 2, payload: .unit),
            Variant(name: "Hint", index: 3, payload: .unit),
        ])),
        Schema(id: StyxSchema.lspDiagnostic, kind: .structure(name: "StyxLspDiagnostic", fields: [
            Field(name: "span", schema: .concrete(StyxSchema.span), required: true),
            Field(name: "severity", schema: .concrete(StyxSchema.lspDiagnosticSeverity), required: true),
            Field(name: "message", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "source", schema: .concrete(StyxSchema.optionString), required: true),
            Field(name: "code", schema: .concrete(StyxSchema.optionString), required: true),
            Field(name: "data", schema: .concrete(StyxSchema.optionValue), required: true),
        ])),
        Schema(id: StyxSchema.lspDiagnosticList, kind: .list(element: .concrete(StyxSchema.lspDiagnostic))),
        Schema(id: StyxSchema.lspDiagnosticParams, kind: .structure(name: "StyxLspDiagnosticParams", fields: [
            Field(name: "document_uri", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "tree", schema: .concrete(StyxSchema.value), required: true),
            Field(name: "content", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: StyxSchema.lspCodeActionKind, kind: .enumeration(name: "StyxLspCodeActionKind", variants: [
            Variant(name: "QuickFix", index: 0, payload: .unit),
            Variant(name: "Refactor", index: 1, payload: .unit),
        ])),
        Schema(id: StyxSchema.optionLspCodeActionKind, kind: .option(element: .concrete(StyxSchema.lspCodeActionKind))),
        Schema(id: StyxSchema.lspTextEdit, kind: .structure(name: "StyxLspTextEdit", fields: [
            Field(name: "span", schema: .concrete(StyxSchema.span), required: true),
            Field(name: "new_text", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: StyxSchema.lspTextEditList, kind: .list(element: .concrete(StyxSchema.lspTextEdit))),
        Schema(id: StyxSchema.lspDocumentEdit, kind: .structure(name: "StyxLspDocumentEdit", fields: [
            Field(name: "uri", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "edits", schema: .concrete(StyxSchema.lspTextEditList), required: true),
        ])),
        Schema(id: StyxSchema.lspDocumentEditList, kind: .list(element: .concrete(StyxSchema.lspDocumentEdit))),
        Schema(id: StyxSchema.lspWorkspaceEdit, kind: .structure(name: "StyxLspWorkspaceEdit", fields: [
            Field(name: "changes", schema: .concrete(StyxSchema.lspDocumentEditList), required: true),
        ])),
        Schema(id: StyxSchema.optionLspWorkspaceEdit, kind: .option(element: .concrete(StyxSchema.lspWorkspaceEdit))),
        Schema(id: StyxSchema.lspCodeActionParams, kind: .structure(name: "StyxLspCodeActionParams", fields: [
            Field(name: "document_uri", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "span", schema: .concrete(StyxSchema.span), required: true),
            Field(name: "diagnostics", schema: .concrete(StyxSchema.lspDiagnosticList), required: true),
        ])),
        Schema(id: StyxSchema.lspCodeAction, kind: .structure(name: "StyxLspCodeAction", fields: [
            Field(name: "title", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "kind", schema: .concrete(StyxSchema.optionLspCodeActionKind), required: true),
            Field(name: "edit", schema: .concrete(StyxSchema.optionLspWorkspaceEdit), required: true),
            Field(name: "is_preferred", schema: .concrete(primitiveId(.bool)), required: true),
        ])),
        Schema(id: StyxSchema.lspCodeActionList, kind: .list(element: .concrete(StyxSchema.lspCodeAction))),
        Schema(id: StyxSchema.lspDefinitionParams, kind: .structure(name: "StyxLspDefinitionParams", fields: [
            Field(name: "document_uri", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "cursor", schema: .concrete(StyxSchema.lspCursor), required: true),
            Field(name: "path", schema: .concrete(StyxSchema.stringList), required: true),
            Field(name: "context", schema: .concrete(StyxSchema.optionValue), required: true),
            Field(name: "tagged_context", schema: .concrete(StyxSchema.optionValue), required: true),
        ])),
        Schema(id: StyxSchema.lspLocation, kind: .structure(name: "StyxLspLocation", fields: [
            Field(name: "uri", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "span", schema: .concrete(StyxSchema.span), required: true),
        ])),
        Schema(id: StyxSchema.lspLocationList, kind: .list(element: .concrete(StyxSchema.lspLocation))),
        Schema(id: StyxSchema.lspSchemaInfo, kind: .structure(name: "StyxLspSchemaInfo", fields: [
            Field(name: "source", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "uri", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: StyxSchema.optionLspSchemaInfo, kind: .option(element: .concrete(StyxSchema.lspSchemaInfo))),
        Schema(id: StyxSchema.lspGetSubtreeParams, kind: .structure(name: "StyxLspGetSubtreeParams", fields: [
            Field(name: "document_uri", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "path", schema: .concrete(StyxSchema.stringList), required: true),
        ])),
        Schema(id: StyxSchema.lspGetDocumentParams, kind: .structure(name: "StyxLspGetDocumentParams", fields: [
            Field(name: "document_uri", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: StyxSchema.lspGetSourceParams, kind: .structure(name: "StyxLspGetSourceParams", fields: [
            Field(name: "document_uri", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: StyxSchema.lspGetSchemaParams, kind: .structure(name: "StyxLspGetSchemaParams", fields: [
            Field(name: "document_uri", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: StyxSchema.lspOffsetToPositionParams, kind: .structure(name: "StyxLspOffsetToPositionParams", fields: [
            Field(name: "document_uri", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "offset", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: StyxSchema.optionLspPosition, kind: .option(element: .concrete(StyxSchema.lspPosition))),
        Schema(id: StyxSchema.lspPositionToOffsetParams, kind: .structure(name: "StyxLspPositionToOffsetParams", fields: [
            Field(name: "document_uri", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "position", schema: .concrete(StyxSchema.lspPosition), required: true),
        ])),
        Schema(id: StyxSchema.lspSurfaceFixture, kind: .structure(name: "StyxLspSurfaceFixture", fields: [
            Field(name: "initialize_params", schema: .concrete(StyxSchema.lspInitializeParams), required: true),
            Field(name: "initialize_result", schema: .concrete(StyxSchema.lspInitializeResult), required: true),
            Field(name: "completion_params", schema: .concrete(StyxSchema.lspCompletionParams), required: true),
            Field(name: "completions", schema: .concrete(StyxSchema.lspCompletionItemList), required: true),
            Field(name: "hover_params", schema: .concrete(StyxSchema.lspHoverParams), required: true),
            Field(name: "hover_result", schema: .concrete(StyxSchema.optionLspHoverResult), required: true),
            Field(name: "inlay_hint_params", schema: .concrete(StyxSchema.lspInlayHintParams), required: true),
            Field(name: "inlay_hints", schema: .concrete(StyxSchema.lspInlayHintList), required: true),
            Field(name: "diagnostic_params", schema: .concrete(StyxSchema.lspDiagnosticParams), required: true),
            Field(name: "diagnostics", schema: .concrete(StyxSchema.lspDiagnosticList), required: true),
            Field(name: "code_action_params", schema: .concrete(StyxSchema.lspCodeActionParams), required: true),
            Field(name: "code_actions", schema: .concrete(StyxSchema.lspCodeActionList), required: true),
            Field(name: "definition_params", schema: .concrete(StyxSchema.lspDefinitionParams), required: true),
            Field(name: "locations", schema: .concrete(StyxSchema.lspLocationList), required: true),
            Field(name: "get_subtree_params", schema: .concrete(StyxSchema.lspGetSubtreeParams), required: true),
            Field(name: "subtree", schema: .concrete(StyxSchema.optionValue), required: true),
            Field(name: "get_document_params", schema: .concrete(StyxSchema.lspGetDocumentParams), required: true),
            Field(name: "document", schema: .concrete(StyxSchema.optionValue), required: true),
            Field(name: "get_source_params", schema: .concrete(StyxSchema.lspGetSourceParams), required: true),
            Field(name: "source", schema: .concrete(StyxSchema.optionString), required: true),
            Field(name: "get_schema_params", schema: .concrete(StyxSchema.lspGetSchemaParams), required: true),
            Field(name: "schema", schema: .concrete(StyxSchema.optionLspSchemaInfo), required: true),
            Field(name: "offset_to_position_params", schema: .concrete(StyxSchema.lspOffsetToPositionParams), required: true),
            Field(name: "position", schema: .concrete(StyxSchema.optionLspPosition), required: true),
            Field(name: "position_to_offset_params", schema: .concrete(StyxSchema.lspPositionToOffsetParams), required: true),
            Field(name: "offset", schema: .concrete(StyxSchema.optionU32), required: true),
        ])),
    ]
}

private func styxSpanDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(StyxSchema.span),
        layout: MemoryLayout<StyxSpan>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<StyxSpan>.offset(of: \StyxSpan.start)!, descriptor: scalarDesc(.u32)),
            FieldAccess(offset: MemoryLayout<StyxSpan>.offset(of: \StyxSpan.end)!, descriptor: scalarDesc(.u32)),
        ], construct: .inPlace))
    )
}

private func styxOptionSpanDesc() -> Descriptor {
    optionDesc(StyxSchema.optionSpan, StyxSpan.self, some: styxSpanDesc())
}

private func styxOptionStringDesc() -> Descriptor {
    optionDesc(StyxSchema.optionString, String.self, some: stringDesc())
}

private func styxTagDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(StyxSchema.tag),
        layout: MemoryLayout<StyxTag>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<StyxTag>.offset(of: \StyxTag.name)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<StyxTag>.offset(of: \StyxTag.span)!, descriptor: styxOptionSpanDesc()),
        ], construct: .inPlace))
    )
}

private func styxOptionTagDesc() -> Descriptor {
    optionDesc(StyxSchema.optionTag, StyxTag.self, some: styxTagDesc())
}

private func styxScalarKindDesc() -> Descriptor {
    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: StyxScalarKind.self).pointee {
        case .bare: return 0
        case .quoted: return 1
        case .raw: return 2
        case .heredoc: return 3
        }
    }
    let inject: (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void = { slot, localIndex, _ in
        let kind: StyxScalarKind
        switch localIndex {
        case 0: kind = .bare
        case 1: kind = .quoted
        case 2: kind = .raw
        case 3: kind = .heredoc
        default: fatalError("bad StyxScalarKind variant index")
        }
        slot.assumingMemoryBound(to: StyxScalarKind.self).initialize(to: kind)
    }
    return Descriptor(
        schema: .concrete(StyxSchema.scalarKind),
        layout: MemoryLayout<StyxScalarKind>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: { _, _, _ in },
            inject: inject,
            variants: [
                VariantAccess(wireIndex: 0, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
                VariantAccess(wireIndex: 1, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
                VariantAccess(wireIndex: 2, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
                VariantAccess(wireIndex: 3, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
            ]
        ))
    )
}

private func styxScalarDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(StyxSchema.scalar),
        layout: MemoryLayout<StyxScalar>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<StyxScalar>.offset(of: \StyxScalar.text)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<StyxScalar>.offset(of: \StyxScalar.kind)!, descriptor: styxScalarKindDesc()),
            FieldAccess(offset: MemoryLayout<StyxScalar>.offset(of: \StyxScalar.span)!, descriptor: styxOptionSpanDesc()),
        ], construct: .inPlace))
    )
}

private func styxValueListDesc(recurseValue: Descriptor) -> Descriptor {
    Descriptor(
        schema: .concrete(StyxSchema.valueList),
        layout: MemoryLayout<[StyxValue]>.phonLayout,
        access: .sequence(SequenceAccess(
            element: recurseValue,
            stride: MemoryLayout<StyxValue>.stride,
            elemAlign: MemoryLayout<StyxValue>.alignment,
            witness: .of(StyxValue.self)
        ))
    )
}

private func styxSequenceDesc(recurseValue: Descriptor) -> Descriptor {
    Descriptor(
        schema: .concrete(StyxSchema.sequence),
        layout: MemoryLayout<StyxSequence>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<StyxSequence>.offset(of: \StyxSequence.items)!, descriptor: styxValueListDesc(recurseValue: recurseValue)),
            FieldAccess(offset: MemoryLayout<StyxSequence>.offset(of: \StyxSequence.span)!, descriptor: styxOptionSpanDesc()),
        ], construct: .inPlace))
    )
}

private func styxEntryDesc(recurseValue: Descriptor) -> Descriptor {
    Descriptor(
        schema: .concrete(StyxSchema.entry),
        layout: MemoryLayout<StyxEntry>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<StyxEntry>.offset(of: \StyxEntry.key)!, descriptor: recurseValue),
            FieldAccess(offset: MemoryLayout<StyxEntry>.offset(of: \StyxEntry.value)!, descriptor: recurseValue),
            FieldAccess(offset: MemoryLayout<StyxEntry>.offset(of: \StyxEntry.docComment)!, descriptor: styxOptionStringDesc()),
        ], construct: .inPlace))
    )
}

private func styxEntryListDesc(recurseValue: Descriptor) -> Descriptor {
    Descriptor(
        schema: .concrete(StyxSchema.entryList),
        layout: MemoryLayout<[StyxEntry]>.phonLayout,
        access: .sequence(SequenceAccess(
            element: styxEntryDesc(recurseValue: recurseValue),
            stride: MemoryLayout<StyxEntry>.stride,
            elemAlign: MemoryLayout<StyxEntry>.alignment,
            witness: .of(StyxEntry.self)
        ))
    )
}

private func styxObjectDesc(recurseValue: Descriptor) -> Descriptor {
    Descriptor(
        schema: .concrete(StyxSchema.object),
        layout: MemoryLayout<StyxObject>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<StyxObject>.offset(of: \StyxObject.entries)!, descriptor: styxEntryListDesc(recurseValue: recurseValue)),
            FieldAccess(offset: MemoryLayout<StyxObject>.offset(of: \StyxObject.span)!, descriptor: styxOptionSpanDesc()),
        ], construct: .inPlace))
    )
}

private func styxPayloadDesc(recurseValue: Descriptor) -> Descriptor {
    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: StyxPayload.self).pointee {
        case .scalar: return 0
        case .sequence: return 1
        case .object: return 2
        }
    }
    let projectPayload: (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void = { value, _, scratch in
        switch value.assumingMemoryBound(to: StyxPayload.self).pointee {
        case .scalar(let scalar):
            scratch.assumingMemoryBound(to: StyxScalar.self).initialize(to: scalar)
        case .sequence(let sequence):
            scratch.assumingMemoryBound(to: StyxSequence.self).initialize(to: sequence)
        case .object(let object):
            scratch.assumingMemoryBound(to: StyxObject.self).initialize(to: object)
        }
    }
    let destroyPayload: (UnsafeMutableRawPointer, Int) -> Void = { scratch, localIndex in
        switch localIndex {
        case 0:
            scratch.assumingMemoryBound(to: StyxScalar.self).deinitialize(count: 1)
        case 1:
            scratch.assumingMemoryBound(to: StyxSequence.self).deinitialize(count: 1)
        case 2:
            scratch.assumingMemoryBound(to: StyxObject.self).deinitialize(count: 1)
        default:
            fatalError("bad StyxPayload variant index")
        }
    }
    let inject: (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void = { slot, localIndex, scratch in
        let payload: StyxPayload
        switch localIndex {
        case 0:
            payload = .scalar(scratch.assumingMemoryBound(to: StyxScalar.self).move())
        case 1:
            payload = .sequence(scratch.assumingMemoryBound(to: StyxSequence.self).move())
        case 2:
            payload = .object(scratch.assumingMemoryBound(to: StyxObject.self).move())
        default:
            fatalError("bad StyxPayload variant index")
        }
        slot.assumingMemoryBound(to: StyxPayload.self).initialize(to: payload)
    }

    let scalar = styxScalarDesc()
    let sequence = styxSequenceDesc(recurseValue: recurseValue)
    let object = styxObjectDesc(recurseValue: recurseValue)
    return Descriptor(
        schema: .concrete(StyxSchema.payload),
        layout: MemoryLayout<StyxPayload>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: projectPayload,
            destroyPayload: destroyPayload,
            inject: inject,
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [FieldAccess(offset: 0, descriptor: scalar)],
                    payloadLayout: MemoryLayout<StyxScalar>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [FieldAccess(offset: 0, descriptor: sequence)],
                    payloadLayout: MemoryLayout<StyxSequence>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 2,
                    payloadFields: [FieldAccess(offset: 0, descriptor: object)],
                    payloadLayout: MemoryLayout<StyxObject>.phonLayout
                ),
            ]
        ))
    )
}

private func styxOptionPayloadDesc(recurseValue: Descriptor) -> Descriptor {
    optionDesc(StyxSchema.optionPayload, StyxPayload.self, some: styxPayloadDesc(recurseValue: recurseValue))
}

private func styxValueBodyDesc(recurseValue: Descriptor) -> Descriptor {
    Descriptor(
        schema: .concrete(StyxSchema.value),
        layout: MemoryLayout<StyxValue>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<StyxValue>.offset(of: \StyxValue.tag)!, descriptor: styxOptionTagDesc()),
            FieldAccess(offset: MemoryLayout<StyxValue>.offset(of: \StyxValue.payload)!, descriptor: styxOptionPayloadDesc(recurseValue: recurseValue)),
            FieldAccess(offset: MemoryLayout<StyxValue>.offset(of: \StyxValue.span)!, descriptor: styxOptionSpanDesc()),
        ], construct: .inPlace))
    )
}

private func styxDescriptor() -> (root: Descriptor, registry: Registry, blocks: [SchemaId: Descriptor]) {
    let recurseValue = Descriptor(
        schema: .concrete(StyxSchema.value),
        layout: MemoryLayout<StyxValue>.phonLayout,
        access: .recurse
    )
    return (
        root: recurseValue,
        registry: Registry(styxSchemas()),
        blocks: [StyxSchema.value: styxValueBodyDesc(recurseValue: recurseValue)]
    )
}

private func styxStringListDesc() -> Descriptor {
    listDesc(StyxSchema.stringList, String.self, element: stringDesc())
}

private func styxOptionU32Desc() -> Descriptor {
    optionDesc(StyxSchema.optionU32, UInt32.self, some: scalarDesc(.u32))
}

private func styxOptionValueDesc(recurseValue: Descriptor) -> Descriptor {
    optionDesc(StyxSchema.optionValue, StyxValue.self, some: recurseValue)
}

private func styxLspPositionDesc() -> Descriptor {
    recordDesc(StyxSchema.lspPosition, StyxLspPosition.self, fields: [
        fieldAccess(\StyxLspPosition.line, scalarDesc(.u32)),
        fieldAccess(\StyxLspPosition.character, scalarDesc(.u32)),
    ])
}

private func styxOptionLspPositionDesc() -> Descriptor {
    optionDesc(StyxSchema.optionLspPosition, StyxLspPosition.self, some: styxLspPositionDesc())
}

private func styxLspRangeDesc() -> Descriptor {
    recordDesc(StyxSchema.lspRange, StyxLspRange.self, fields: [
        fieldAccess(\StyxLspRange.start, styxLspPositionDesc()),
        fieldAccess(\StyxLspRange.end, styxLspPositionDesc()),
    ])
}

private func styxOptionLspRangeDesc() -> Descriptor {
    optionDesc(StyxSchema.optionLspRange, StyxLspRange.self, some: styxLspRangeDesc())
}

private func styxLspCursorDesc() -> Descriptor {
    recordDesc(StyxSchema.lspCursor, StyxLspCursor.self, fields: [
        fieldAccess(\StyxLspCursor.line, scalarDesc(.u32)),
        fieldAccess(\StyxLspCursor.character, scalarDesc(.u32)),
        fieldAccess(\StyxLspCursor.offset, scalarDesc(.u32)),
    ])
}

private func styxLspCapabilityDesc() -> Descriptor {
    unitEnumDesc(
        StyxSchema.lspCapability,
        StyxLspCapability.self,
        variantCount: 5,
        tag: { ptr in
            switch ptr.assumingMemoryBound(to: StyxLspCapability.self).pointee {
            case .completions: return 0
            case .hover: return 1
            case .diagnostics: return 2
            case .codeActions: return 3
            case .definition: return 4
            }
        },
        make: { localIndex in
            switch localIndex {
            case 0: return .completions
            case 1: return .hover
            case 2: return .diagnostics
            case 3: return .codeActions
            case 4: return .definition
            default: fatalError("bad StyxLspCapability variant index")
            }
        }
    )
}

private func styxLspCapabilityListDesc() -> Descriptor {
    listDesc(StyxSchema.lspCapabilityList, StyxLspCapability.self, element: styxLspCapabilityDesc())
}

private func styxLspInitializeParamsDesc() -> Descriptor {
    recordDesc(StyxSchema.lspInitializeParams, StyxLspInitializeParams.self, fields: [
        fieldAccess(\StyxLspInitializeParams.styxVersion, stringDesc()),
        fieldAccess(\StyxLspInitializeParams.documentUri, stringDesc()),
        fieldAccess(\StyxLspInitializeParams.schemaId, stringDesc()),
    ])
}

private func styxLspInitializeResultDesc() -> Descriptor {
    recordDesc(StyxSchema.lspInitializeResult, StyxLspInitializeResult.self, fields: [
        fieldAccess(\StyxLspInitializeResult.name, stringDesc()),
        fieldAccess(\StyxLspInitializeResult.version, stringDesc()),
        fieldAccess(\StyxLspInitializeResult.capabilities, styxLspCapabilityListDesc()),
    ])
}

private func styxLspCompletionKindDesc() -> Descriptor {
    unitEnumDesc(
        StyxSchema.lspCompletionKind,
        StyxLspCompletionKind.self,
        variantCount: 4,
        tag: { ptr in
            switch ptr.assumingMemoryBound(to: StyxLspCompletionKind.self).pointee {
            case .field: return 0
            case .type: return 1
            case .function: return 2
            case .keyword: return 3
            }
        },
        make: { localIndex in
            switch localIndex {
            case 0: return .field
            case 1: return .type
            case 2: return .function
            case 3: return .keyword
            default: fatalError("bad StyxLspCompletionKind variant index")
            }
        }
    )
}

private func styxOptionLspCompletionKindDesc() -> Descriptor {
    optionDesc(StyxSchema.optionLspCompletionKind, StyxLspCompletionKind.self, some: styxLspCompletionKindDesc())
}

private func styxLspCompletionParamsDesc(recurseValue: Descriptor) -> Descriptor {
    recordDesc(StyxSchema.lspCompletionParams, StyxLspCompletionParams.self, fields: [
        fieldAccess(\StyxLspCompletionParams.documentUri, stringDesc()),
        fieldAccess(\StyxLspCompletionParams.cursor, styxLspCursorDesc()),
        fieldAccess(\StyxLspCompletionParams.path, styxStringListDesc()),
        fieldAccess(\StyxLspCompletionParams.prefix, stringDesc()),
        fieldAccess(\StyxLspCompletionParams.context, styxOptionValueDesc(recurseValue: recurseValue)),
        fieldAccess(\StyxLspCompletionParams.taggedContext, styxOptionValueDesc(recurseValue: recurseValue)),
    ])
}

private func styxLspCompletionItemDesc() -> Descriptor {
    recordDesc(StyxSchema.lspCompletionItem, StyxLspCompletionItem.self, fields: [
        fieldAccess(\StyxLspCompletionItem.label, stringDesc()),
        fieldAccess(\StyxLspCompletionItem.detail, styxOptionStringDesc()),
        fieldAccess(\StyxLspCompletionItem.documentation, styxOptionStringDesc()),
        fieldAccess(\StyxLspCompletionItem.kind, styxOptionLspCompletionKindDesc()),
        fieldAccess(\StyxLspCompletionItem.sortText, styxOptionStringDesc()),
        fieldAccess(\StyxLspCompletionItem.insertText, styxOptionStringDesc()),
    ])
}

private func styxLspCompletionItemListDesc() -> Descriptor {
    listDesc(StyxSchema.lspCompletionItemList, StyxLspCompletionItem.self, element: styxLspCompletionItemDesc())
}

private func styxLspHoverParamsDesc(recurseValue: Descriptor) -> Descriptor {
    recordDesc(StyxSchema.lspHoverParams, StyxLspHoverParams.self, fields: [
        fieldAccess(\StyxLspHoverParams.documentUri, stringDesc()),
        fieldAccess(\StyxLspHoverParams.cursor, styxLspCursorDesc()),
        fieldAccess(\StyxLspHoverParams.path, styxStringListDesc()),
        fieldAccess(\StyxLspHoverParams.context, styxOptionValueDesc(recurseValue: recurseValue)),
        fieldAccess(\StyxLspHoverParams.taggedContext, styxOptionValueDesc(recurseValue: recurseValue)),
    ])
}

private func styxLspHoverResultDesc() -> Descriptor {
    recordDesc(StyxSchema.lspHoverResult, StyxLspHoverResult.self, fields: [
        fieldAccess(\StyxLspHoverResult.contents, stringDesc()),
        fieldAccess(\StyxLspHoverResult.range, styxOptionLspRangeDesc()),
    ])
}

private func styxOptionLspHoverResultDesc() -> Descriptor {
    optionDesc(StyxSchema.optionLspHoverResult, StyxLspHoverResult.self, some: styxLspHoverResultDesc())
}

private func styxLspInlayHintKindDesc() -> Descriptor {
    unitEnumDesc(
        StyxSchema.lspInlayHintKind,
        StyxLspInlayHintKind.self,
        variantCount: 2,
        tag: { ptr in
            switch ptr.assumingMemoryBound(to: StyxLspInlayHintKind.self).pointee {
            case .type: return 0
            case .parameter: return 1
            }
        },
        make: { localIndex in
            switch localIndex {
            case 0: return .type
            case 1: return .parameter
            default: fatalError("bad StyxLspInlayHintKind variant index")
            }
        }
    )
}

private func styxOptionLspInlayHintKindDesc() -> Descriptor {
    optionDesc(StyxSchema.optionLspInlayHintKind, StyxLspInlayHintKind.self, some: styxLspInlayHintKindDesc())
}

private func styxLspInlayHintParamsDesc(recurseValue: Descriptor) -> Descriptor {
    recordDesc(StyxSchema.lspInlayHintParams, StyxLspInlayHintParams.self, fields: [
        fieldAccess(\StyxLspInlayHintParams.documentUri, stringDesc()),
        fieldAccess(\StyxLspInlayHintParams.range, styxLspRangeDesc()),
        fieldAccess(\StyxLspInlayHintParams.context, styxOptionValueDesc(recurseValue: recurseValue)),
    ])
}

private func styxLspInlayHintDesc() -> Descriptor {
    recordDesc(StyxSchema.lspInlayHint, StyxLspInlayHint.self, fields: [
        fieldAccess(\StyxLspInlayHint.position, styxLspPositionDesc()),
        fieldAccess(\StyxLspInlayHint.label, stringDesc()),
        fieldAccess(\StyxLspInlayHint.kind, styxOptionLspInlayHintKindDesc()),
        fieldAccess(\StyxLspInlayHint.paddingLeft, scalarDesc(.bool)),
        fieldAccess(\StyxLspInlayHint.paddingRight, scalarDesc(.bool)),
    ])
}

private func styxLspInlayHintListDesc() -> Descriptor {
    listDesc(StyxSchema.lspInlayHintList, StyxLspInlayHint.self, element: styxLspInlayHintDesc())
}

private func styxLspDiagnosticSeverityDesc() -> Descriptor {
    unitEnumDesc(
        StyxSchema.lspDiagnosticSeverity,
        StyxLspDiagnosticSeverity.self,
        variantCount: 4,
        tag: { ptr in
            switch ptr.assumingMemoryBound(to: StyxLspDiagnosticSeverity.self).pointee {
            case .error: return 0
            case .warning: return 1
            case .information: return 2
            case .hint: return 3
            }
        },
        make: { localIndex in
            switch localIndex {
            case 0: return .error
            case 1: return .warning
            case 2: return .information
            case 3: return .hint
            default: fatalError("bad StyxLspDiagnosticSeverity variant index")
            }
        }
    )
}

private func styxLspDiagnosticDesc(recurseValue: Descriptor) -> Descriptor {
    recordDesc(StyxSchema.lspDiagnostic, StyxLspDiagnostic.self, fields: [
        fieldAccess(\StyxLspDiagnostic.span, styxSpanDesc()),
        fieldAccess(\StyxLspDiagnostic.severity, styxLspDiagnosticSeverityDesc()),
        fieldAccess(\StyxLspDiagnostic.message, stringDesc()),
        fieldAccess(\StyxLspDiagnostic.source, styxOptionStringDesc()),
        fieldAccess(\StyxLspDiagnostic.code, styxOptionStringDesc()),
        fieldAccess(\StyxLspDiagnostic.data, styxOptionValueDesc(recurseValue: recurseValue)),
    ])
}

private func styxLspDiagnosticListDesc(recurseValue: Descriptor) -> Descriptor {
    listDesc(StyxSchema.lspDiagnosticList, StyxLspDiagnostic.self, element: styxLspDiagnosticDesc(recurseValue: recurseValue))
}

private func styxLspDiagnosticParamsDesc(recurseValue: Descriptor) -> Descriptor {
    recordDesc(StyxSchema.lspDiagnosticParams, StyxLspDiagnosticParams.self, fields: [
        fieldAccess(\StyxLspDiagnosticParams.documentUri, stringDesc()),
        fieldAccess(\StyxLspDiagnosticParams.tree, recurseValue),
        fieldAccess(\StyxLspDiagnosticParams.content, stringDesc()),
    ])
}

private func styxLspCodeActionKindDesc() -> Descriptor {
    unitEnumDesc(
        StyxSchema.lspCodeActionKind,
        StyxLspCodeActionKind.self,
        variantCount: 2,
        tag: { ptr in
            switch ptr.assumingMemoryBound(to: StyxLspCodeActionKind.self).pointee {
            case .quickFix: return 0
            case .refactor: return 1
            }
        },
        make: { localIndex in
            switch localIndex {
            case 0: return .quickFix
            case 1: return .refactor
            default: fatalError("bad StyxLspCodeActionKind variant index")
            }
        }
    )
}

private func styxOptionLspCodeActionKindDesc() -> Descriptor {
    optionDesc(StyxSchema.optionLspCodeActionKind, StyxLspCodeActionKind.self, some: styxLspCodeActionKindDesc())
}

private func styxLspTextEditDesc() -> Descriptor {
    recordDesc(StyxSchema.lspTextEdit, StyxLspTextEdit.self, fields: [
        fieldAccess(\StyxLspTextEdit.span, styxSpanDesc()),
        fieldAccess(\StyxLspTextEdit.newText, stringDesc()),
    ])
}

private func styxLspTextEditListDesc() -> Descriptor {
    listDesc(StyxSchema.lspTextEditList, StyxLspTextEdit.self, element: styxLspTextEditDesc())
}

private func styxLspDocumentEditDesc() -> Descriptor {
    recordDesc(StyxSchema.lspDocumentEdit, StyxLspDocumentEdit.self, fields: [
        fieldAccess(\StyxLspDocumentEdit.uri, stringDesc()),
        fieldAccess(\StyxLspDocumentEdit.edits, styxLspTextEditListDesc()),
    ])
}

private func styxLspDocumentEditListDesc() -> Descriptor {
    listDesc(StyxSchema.lspDocumentEditList, StyxLspDocumentEdit.self, element: styxLspDocumentEditDesc())
}

private func styxLspWorkspaceEditDesc() -> Descriptor {
    recordDesc(StyxSchema.lspWorkspaceEdit, StyxLspWorkspaceEdit.self, fields: [
        fieldAccess(\StyxLspWorkspaceEdit.changes, styxLspDocumentEditListDesc()),
    ])
}

private func styxOptionLspWorkspaceEditDesc() -> Descriptor {
    optionDesc(StyxSchema.optionLspWorkspaceEdit, StyxLspWorkspaceEdit.self, some: styxLspWorkspaceEditDesc())
}

private func styxLspCodeActionParamsDesc(recurseValue: Descriptor) -> Descriptor {
    recordDesc(StyxSchema.lspCodeActionParams, StyxLspCodeActionParams.self, fields: [
        fieldAccess(\StyxLspCodeActionParams.documentUri, stringDesc()),
        fieldAccess(\StyxLspCodeActionParams.span, styxSpanDesc()),
        fieldAccess(\StyxLspCodeActionParams.diagnostics, styxLspDiagnosticListDesc(recurseValue: recurseValue)),
    ])
}

private func styxLspCodeActionDesc() -> Descriptor {
    recordDesc(StyxSchema.lspCodeAction, StyxLspCodeAction.self, fields: [
        fieldAccess(\StyxLspCodeAction.title, stringDesc()),
        fieldAccess(\StyxLspCodeAction.kind, styxOptionLspCodeActionKindDesc()),
        fieldAccess(\StyxLspCodeAction.edit, styxOptionLspWorkspaceEditDesc()),
        fieldAccess(\StyxLspCodeAction.isPreferred, scalarDesc(.bool)),
    ])
}

private func styxLspCodeActionListDesc() -> Descriptor {
    listDesc(StyxSchema.lspCodeActionList, StyxLspCodeAction.self, element: styxLspCodeActionDesc())
}

private func styxLspDefinitionParamsDesc(recurseValue: Descriptor) -> Descriptor {
    recordDesc(StyxSchema.lspDefinitionParams, StyxLspDefinitionParams.self, fields: [
        fieldAccess(\StyxLspDefinitionParams.documentUri, stringDesc()),
        fieldAccess(\StyxLspDefinitionParams.cursor, styxLspCursorDesc()),
        fieldAccess(\StyxLspDefinitionParams.path, styxStringListDesc()),
        fieldAccess(\StyxLspDefinitionParams.context, styxOptionValueDesc(recurseValue: recurseValue)),
        fieldAccess(\StyxLspDefinitionParams.taggedContext, styxOptionValueDesc(recurseValue: recurseValue)),
    ])
}

private func styxLspLocationDesc() -> Descriptor {
    recordDesc(StyxSchema.lspLocation, StyxLspLocation.self, fields: [
        fieldAccess(\StyxLspLocation.uri, stringDesc()),
        fieldAccess(\StyxLspLocation.span, styxSpanDesc()),
    ])
}

private func styxLspLocationListDesc() -> Descriptor {
    listDesc(StyxSchema.lspLocationList, StyxLspLocation.self, element: styxLspLocationDesc())
}

private func styxLspSchemaInfoDesc() -> Descriptor {
    recordDesc(StyxSchema.lspSchemaInfo, StyxLspSchemaInfo.self, fields: [
        fieldAccess(\StyxLspSchemaInfo.source, stringDesc()),
        fieldAccess(\StyxLspSchemaInfo.uri, stringDesc()),
    ])
}

private func styxOptionLspSchemaInfoDesc() -> Descriptor {
    optionDesc(StyxSchema.optionLspSchemaInfo, StyxLspSchemaInfo.self, some: styxLspSchemaInfoDesc())
}

private func styxLspGetSubtreeParamsDesc() -> Descriptor {
    recordDesc(StyxSchema.lspGetSubtreeParams, StyxLspGetSubtreeParams.self, fields: [
        fieldAccess(\StyxLspGetSubtreeParams.documentUri, stringDesc()),
        fieldAccess(\StyxLspGetSubtreeParams.path, styxStringListDesc()),
    ])
}

private func styxLspGetDocumentParamsDesc() -> Descriptor {
    recordDesc(StyxSchema.lspGetDocumentParams, StyxLspGetDocumentParams.self, fields: [
        fieldAccess(\StyxLspGetDocumentParams.documentUri, stringDesc()),
    ])
}

private func styxLspGetSourceParamsDesc() -> Descriptor {
    recordDesc(StyxSchema.lspGetSourceParams, StyxLspGetSourceParams.self, fields: [
        fieldAccess(\StyxLspGetSourceParams.documentUri, stringDesc()),
    ])
}

private func styxLspGetSchemaParamsDesc() -> Descriptor {
    recordDesc(StyxSchema.lspGetSchemaParams, StyxLspGetSchemaParams.self, fields: [
        fieldAccess(\StyxLspGetSchemaParams.documentUri, stringDesc()),
    ])
}

private func styxLspOffsetToPositionParamsDesc() -> Descriptor {
    recordDesc(StyxSchema.lspOffsetToPositionParams, StyxLspOffsetToPositionParams.self, fields: [
        fieldAccess(\StyxLspOffsetToPositionParams.documentUri, stringDesc()),
        fieldAccess(\StyxLspOffsetToPositionParams.offset, scalarDesc(.u32)),
    ])
}

private func styxLspPositionToOffsetParamsDesc() -> Descriptor {
    recordDesc(StyxSchema.lspPositionToOffsetParams, StyxLspPositionToOffsetParams.self, fields: [
        fieldAccess(\StyxLspPositionToOffsetParams.documentUri, stringDesc()),
        fieldAccess(\StyxLspPositionToOffsetParams.position, styxLspPositionDesc()),
    ])
}

private func styxLspSurfaceFixtureDesc(recurseValue: Descriptor) -> Descriptor {
    recordDesc(StyxSchema.lspSurfaceFixture, StyxLspSurfaceFixture.self, fields: [
        fieldAccess(\StyxLspSurfaceFixture.initializeParams, styxLspInitializeParamsDesc()),
        fieldAccess(\StyxLspSurfaceFixture.initializeResult, styxLspInitializeResultDesc()),
        fieldAccess(\StyxLspSurfaceFixture.completionParams, styxLspCompletionParamsDesc(recurseValue: recurseValue)),
        fieldAccess(\StyxLspSurfaceFixture.completions, styxLspCompletionItemListDesc()),
        fieldAccess(\StyxLspSurfaceFixture.hoverParams, styxLspHoverParamsDesc(recurseValue: recurseValue)),
        fieldAccess(\StyxLspSurfaceFixture.hoverResult, styxOptionLspHoverResultDesc()),
        fieldAccess(\StyxLspSurfaceFixture.inlayHintParams, styxLspInlayHintParamsDesc(recurseValue: recurseValue)),
        fieldAccess(\StyxLspSurfaceFixture.inlayHints, styxLspInlayHintListDesc()),
        fieldAccess(\StyxLspSurfaceFixture.diagnosticParams, styxLspDiagnosticParamsDesc(recurseValue: recurseValue)),
        fieldAccess(\StyxLspSurfaceFixture.diagnostics, styxLspDiagnosticListDesc(recurseValue: recurseValue)),
        fieldAccess(\StyxLspSurfaceFixture.codeActionParams, styxLspCodeActionParamsDesc(recurseValue: recurseValue)),
        fieldAccess(\StyxLspSurfaceFixture.codeActions, styxLspCodeActionListDesc()),
        fieldAccess(\StyxLspSurfaceFixture.definitionParams, styxLspDefinitionParamsDesc(recurseValue: recurseValue)),
        fieldAccess(\StyxLspSurfaceFixture.locations, styxLspLocationListDesc()),
        fieldAccess(\StyxLspSurfaceFixture.getSubtreeParams, styxLspGetSubtreeParamsDesc()),
        fieldAccess(\StyxLspSurfaceFixture.subtree, styxOptionValueDesc(recurseValue: recurseValue)),
        fieldAccess(\StyxLspSurfaceFixture.getDocumentParams, styxLspGetDocumentParamsDesc()),
        fieldAccess(\StyxLspSurfaceFixture.document, styxOptionValueDesc(recurseValue: recurseValue)),
        fieldAccess(\StyxLspSurfaceFixture.getSourceParams, styxLspGetSourceParamsDesc()),
        fieldAccess(\StyxLspSurfaceFixture.source, styxOptionStringDesc()),
        fieldAccess(\StyxLspSurfaceFixture.getSchemaParams, styxLspGetSchemaParamsDesc()),
        fieldAccess(\StyxLspSurfaceFixture.schema, styxOptionLspSchemaInfoDesc()),
        fieldAccess(\StyxLspSurfaceFixture.offsetToPositionParams, styxLspOffsetToPositionParamsDesc()),
        fieldAccess(\StyxLspSurfaceFixture.position, styxOptionLspPositionDesc()),
        fieldAccess(\StyxLspSurfaceFixture.positionToOffsetParams, styxLspPositionToOffsetParamsDesc()),
        fieldAccess(\StyxLspSurfaceFixture.offset, styxOptionU32Desc()),
    ])
}

private func styxLspSurfaceDescriptor() -> (root: Descriptor, registry: Registry, blocks: [SchemaId: Descriptor]) {
    let recurseValue = Descriptor(
        schema: .concrete(StyxSchema.value),
        layout: MemoryLayout<StyxValue>.phonLayout,
        access: .recurse
    )
    return (
        root: styxLspSurfaceFixtureDesc(recurseValue: recurseValue),
        registry: Registry(styxSchemas()),
        blocks: [StyxSchema.value: styxValueBodyDesc(recurseValue: recurseValue)]
    )
}

private func styxSpan(_ start: UInt32, _ end: UInt32) -> StyxSpan {
    StyxSpan(start: start, end: end)
}

private func styxScalarValue(_ text: String, _ kind: StyxScalarKind, _ start: UInt32, _ end: UInt32) -> StyxValue {
    StyxValue(
        tag: nil,
        payload: .scalar(StyxScalar(text: text, kind: kind, span: styxSpan(start, end))),
        span: styxSpan(start, end)
    )
}

private func sampleStyxValue() -> StyxValue {
    StyxValue(
        tag: StyxTag(name: "schema", span: styxSpan(0, 7)),
        payload: .object(StyxObject(entries: [
            StyxEntry(
                key: styxScalarValue("title", .bare, 9, 14),
                value: styxScalarValue("Phon migration", .quoted, 15, 31),
                docComment: "page title"
            ),
            StyxEntry(
                key: styxScalarValue("features", .bare, 33, 41),
                value: StyxValue(
                    tag: StyxTag(name: "seq", span: styxSpan(42, 46)),
                    payload: .sequence(StyxSequence(items: [
                        styxScalarValue("jit", .bare, 47, 50),
                        StyxValue(
                            tag: StyxTag(name: "object", span: styxSpan(51, 58)),
                            payload: .object(StyxObject(entries: [
                                StyxEntry(
                                    key: styxScalarValue("lang", .bare, 59, 63),
                                    value: styxScalarValue("rust", .raw, 64, 70),
                                    docComment: nil
                                ),
                            ], span: styxSpan(58, 71))),
                            span: styxSpan(51, 71)
                        ),
                    ], span: styxSpan(46, 72))),
                    span: styxSpan(42, 72)
                ),
                docComment: nil
            ),
        ], span: styxSpan(8, 73))),
        span: styxSpan(0, 73)
    )
}

private func sampleStyxLspUri() -> String {
    "file:///workspace/queries.styx"
}

private func sampleStyxLspSource() -> String {
    "@query { from products select (id name) }"
}

private func sampleStyxLspPosition() -> StyxLspPosition {
    StyxLspPosition(line: 0, character: 16)
}

private func sampleStyxLspCursor() -> StyxLspCursor {
    StyxLspCursor(line: 0, character: 16, offset: 16)
}

private func sampleStyxLspRange() -> StyxLspRange {
    StyxLspRange(
        start: StyxLspPosition(line: 0, character: 0),
        end: StyxLspPosition(line: 0, character: 38)
    )
}

private func sampleStyxLspDiagnostics() -> [StyxLspDiagnostic] {
    [
        StyxLspDiagnostic(
            span: StyxSpan(start: 23, end: 29),
            severity: .warning,
            message: "column `legacy` is deprecated",
            source: "dibs",
            code: "deprecated-column",
            data: sampleStyxValue()
        ),
    ]
}

private func sampleStyxLspSurfaceFixture() -> StyxLspSurfaceFixture {
    let uri = sampleStyxLspUri()
    let value = sampleStyxValue()
    let cursor = sampleStyxLspCursor()
    let diagnostics = sampleStyxLspDiagnostics()

    return StyxLspSurfaceFixture(
        initializeParams: StyxLspInitializeParams(
            styxVersion: "4.0",
            documentUri: uri,
            schemaId: "crate:dibs-queries@1"
        ),
        initializeResult: StyxLspInitializeResult(
            name: "dibs-styx-extension",
            version: "0.1.0",
            capabilities: [.completions, .hover, .diagnostics, .codeActions, .definition]
        ),
        completionParams: StyxLspCompletionParams(
            documentUri: uri,
            cursor: cursor,
            path: ["AllProducts", "@query", "select"],
            prefix: "na",
            context: value,
            taggedContext: value
        ),
        completions: [
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
        ],
        hoverParams: StyxLspHoverParams(
            documentUri: uri,
            cursor: cursor,
            path: ["AllProducts", "@query", "from"],
            context: value,
            taggedContext: value
        ),
        hoverResult: StyxLspHoverResult(
            contents: "**products** table\n\nBacked by `Product`.",
            range: StyxLspRange(
                start: StyxLspPosition(line: 0, character: 14),
                end: StyxLspPosition(line: 0, character: 22)
            )
        ),
        inlayHintParams: StyxLspInlayHintParams(
            documentUri: uri,
            range: sampleStyxLspRange(),
            context: value
        ),
        inlayHints: [
            StyxLspInlayHint(
                position: StyxLspPosition(line: 0, character: 9),
                label: "Product",
                kind: .type,
                paddingLeft: true,
                paddingRight: false
            ),
        ],
        diagnosticParams: StyxLspDiagnosticParams(
            documentUri: uri,
            tree: value,
            content: sampleStyxLspSource()
        ),
        diagnostics: diagnostics,
        codeActionParams: StyxLspCodeActionParams(
            documentUri: uri,
            span: StyxSpan(start: 23, end: 29),
            diagnostics: diagnostics
        ),
        codeActions: [
            StyxLspCodeAction(
                title: "Replace legacy column",
                kind: .quickFix,
                edit: StyxLspWorkspaceEdit(changes: [
                    StyxLspDocumentEdit(
                        uri: uri,
                        edits: [
                            StyxLspTextEdit(
                                span: StyxSpan(start: 23, end: 29),
                                newText: "name"
                            ),
                        ]
                    ),
                ]),
                isPreferred: true
            ),
        ],
        definitionParams: StyxLspDefinitionParams(
            documentUri: uri,
            cursor: cursor,
            path: ["AllProducts", "@query", "from"],
            context: value,
            taggedContext: value
        ),
        locations: [
            StyxLspLocation(uri: "file:///workspace/schema.styx", span: StyxSpan(start: 120, end: 128)),
        ],
        getSubtreeParams: StyxLspGetSubtreeParams(documentUri: uri, path: ["AllProducts", "@query"]),
        subtree: value,
        getDocumentParams: StyxLspGetDocumentParams(documentUri: uri),
        document: value,
        getSourceParams: StyxLspGetSourceParams(documentUri: uri),
        source: sampleStyxLspSource(),
        getSchemaParams: StyxLspGetSchemaParams(documentUri: uri),
        schema: StyxLspSchemaInfo(
            source: "@schema { @ @object{ name @string } }",
            uri: "styx-embedded://crate:dibs-queries@1"
        ),
        offsetToPositionParams: StyxLspOffsetToPositionParams(documentUri: uri, offset: 16),
        position: sampleStyxLspPosition(),
        positionToOffsetParams: StyxLspPositionToOffsetParams(documentUri: uri, position: sampleStyxLspPosition()),
        offset: 16
    )
}

private func sampleDodecaDynamicObject() -> Value {
    .object([
        .init(key: "k", value: .number(.canonical(unsigned: 1))),
        .init(key: "flag", value: .bool(true)),
    ])
}

private func sampleDodecaFrontmatterExtra() -> Value {
    .object([
        .init(key: "sidebar", value: .bool(true)),
        .init(key: "icon", value: .string("book")),
        .init(key: "custom_value", value: .number(.canonical(unsigned: 42))),
    ])
}

private func sampleDodecaTemplateCall() -> DodecaTemplateCall {
    DodecaTemplateCall(
        contextId: "ctx-1",
        name: "get_section",
        args: [sampleDodecaDynamicObject(), .string("docs")],
        kwargs: [
            DodecaStringValue(key: "path", value: .string("/guide/")),
            DodecaStringValue(key: "meta", value: sampleDodecaDynamicObject()),
        ]
    )
}

private func sampleDodecaLoadDataResult() -> DodecaLoadDataResult {
    .success(
        value: .object([
            .init(key: "title", value: .string("Phon")),
            .init(key: "sidebar", value: .bool(true)),
            .init(key: "count", value: .number(.i64(42))),
        ])
    )
}

private func sampleDodecaParseResult() -> DodecaParseResult {
    .success(DodecaParseSuccessPayload(
        frontmatter: DodecaFrontmatter(
            title: "Phon migration",
            weight: 10,
            description: "Generated fixture for Dodeca markdown",
            template: "page.html",
            extra: sampleDodecaFrontmatterExtra()
        ),
        html: "<h1 data-sid=\"h1\">Intro</h1><p data-sid=\"p1\">Generated fixture</p>",
        headings: [
            DodecaMarkdownHeading(title: "Intro", id: "intro", level: 1),
        ],
        reqs: [
            DodecaReqDefinition(id: "vox.dodeca.markdown", anchorId: "r-vox-dodeca-markdown"),
        ],
        headInjections: [
            "<link rel=\"stylesheet\" href=\"/assets/arborium.css\">",
        ],
        sourceMap: DodecaSourceMap(
            sourcePath: "content/guide.md",
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
    ))
}

private func sampleDodecaDecodedImage(seed: UInt8, width: UInt32, height: UInt32) -> DodecaDecodedImage {
    let count = Int(width * height * 4)
    return DodecaDecodedImage(
        pixels: (0 ..< count).map { seed &+ UInt8($0 & 0xff) },
        width: width,
        height: height,
        channels: 4
    )
}

private func sampleDodecaImageProcessorFixture() -> DodecaImageProcessorFixture {
    let decoded = sampleDodecaDecodedImage(seed: 0x20, width: 4, height: 3)
    let resized = sampleDodecaDecodedImage(seed: 0x80, width: 2, height: 2)
    return DodecaImageProcessorFixture(
        pngData: [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d],
        decodedResult: .success(DodecaImageSuccessPayload(image: decoded)),
        resizeInput: DodecaResizeInput(
            pixels: decoded.pixels,
            width: decoded.width,
            height: decoded.height,
            channels: decoded.channels,
            targetWidth: 2
        ),
        resizeResult: .success(DodecaImageSuccessPayload(image: resized)),
        thumbhashInput: DodecaThumbhashInput(
            pixels: decoded.pixels,
            width: decoded.width,
            height: decoded.height
        ),
        thumbhashResult: .thumbhashSuccess(DodecaThumbhashSuccessPayload(
            dataUrl: "data:image/png;base64,thumbhash-fixture"
        )),
        errorResult: .error(DodecaImageErrorPayload(message: "unsupported image format"))
    )
}

private func sampleDodecaSearchIndexerFixture() -> DodecaSearchIndexerFixture {
    DodecaSearchIndexerFixture(
        pages: [
            DodecaSearchPage(
                url: "/guide/intro/",
                source: "docs",
                html: "<main><h1>Intro</h1><p>Phon and Vox migration.</p></main>"
            ),
            DodecaSearchPage(
                url: "/reference/jit/",
                source: "docs",
                html: "<main><h1>JIT</h1><p>Typed plans lower to native code.</p></main>"
            ),
        ],
        result: .success(DodecaSearchSuccessPayload(files: [
            DodecaSearchFile(path: "/search/meta", contents: [0x91, 0x02, 0x01, 0x00]),
            DodecaSearchFile(path: "/search/chunk-0", contents: [0xde, 0xad, 0xbe, 0xef, 0x42]),
        ])),
        errorResult: .error(DodecaSearchErrorPayload(message: "search index extraction failed"))
    )
}

private func sampleDodecaHtmlProcessInput() -> DodecaHtmlProcessInput {
    DodecaHtmlProcessInput(
        html: "<main><img src=\"/hero.png\"></main>",
        pathMap: ["/old.css": "/assets/new.css"],
        knownRoutes: Set(["/", "/guide/"]),
        codeMetadata: [
            "sample.rs": DodecaCodeExecutionMetadata(
                language: "rust",
                dependencies: [
                    DodecaResolvedDependency(name: "facet", version: "0.29"),
                ],
                durationMs: 12
            ),
        ],
        injections: [
            DodecaInjection(location: .head, content: "<meta charset=\"utf-8\">"),
        ],
        imageVariants: [
            "/hero.png": DodecaResponsiveImageInfo(
                jxlSrcset: [DodecaStringU32(string: "/hero-640.jxl", value: 640)],
                webpSrcset: [DodecaStringU32(string: "/hero-640.webp", value: 640)]
            ),
        ],
        viteCssMap: [
            "/entry.ts": ["/assets/entry.css", "/assets/chunk.css"],
        ],
        mount: DodecaMountLocalization(
            segment: "wiki",
            routes: Set(["/wiki/", "/wiki/exec/"])
        )
    )
}

private func sampleDibsListResponse() -> DibsListResponse {
    DibsListResponse(
        rows: [[
            DibsRowField(name: "id", value: .i64(123)),
            DibsRowField(name: "payload", value: .bytes([1, 2, 3, 5, 8])),
        ]],
        total: 1
    )
}

private func sampleHotmealLiveReloadFixture() -> HotmealLiveReloadFixture {
    HotmealLiveReloadFixture(
        subscribe: HotmealSubscribeRequest(route: "/guide/"),
        events: [
            .reload,
            .patches(route: "/guide/", patchesBlob: [0xde, 0xad, 0xbe, 0xef]),
            .headChanged(route: "/guide/"),
        ]
    )
}

private func sampleStaxLinuxBrokerControlFixture() -> StaxLinuxBrokerControlFixture {
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

private func sampleStaxMacKdBufBatch() -> StaxMacKdBufBatch {
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
    )
}

private func sampleTraceyMigrationFixture() -> TraceyMigrationFixture {
    let ruleId = TraceyRuleId(base: "compat.plan-first", version: 1)
    return TraceyMigrationFixture(
        status: TraceyStatusResponse(impls: [
            TraceyImplStatus(
                spec: "phon",
                implName: "swift",
                totalRules: 69,
                coveredRules: 55,
                staleRules: 0,
                verifiedRules: 46
            ),
        ]),
        uncoveredRequest: TraceyUncoveredRequest(
            spec: "phon",
            implName: "swift",
            prefix: "compat"
        ),
        uncoveredResponse: TraceyUncoveredResponse(
            spec: "phon",
            implName: "swift",
            totalRules: 69,
            uncoveredCount: 1,
            bySection: [
                TraceySectionRules(
                    section: "Compatibility",
                    rules: [
                        TraceyRuleRef(
                            id: ruleId,
                            text: "Compatibility plans are built before decode."
                        ),
                    ]
                ),
            ]
        ),
        dataUpdateItem: TraceyDataUpdate(
            version: 42,
            delta: TraceyDeltaSummary(
                newlyCovered: [
                    TraceyCoverageChange(
                        ruleId: ruleId,
                        file: "swift/phon-engine/Sources/PhonEngine/TypedEngine.swift",
                        line: 10
                    ),
                ],
                newlyUncovered: [
                    TraceyRuleId(base: "type-system.channel", version: 1),
                ]
            )
        ),
        workspaceDiagnostics: [
            TraceyLspFileDiagnostics(
                path: "docs/content/spec.md",
                diagnostics: [
                    TraceyLspDiagnostic(
                        severity: "warning",
                        code: "stale",
                        message: "reference points to an older rule version",
                        startLine: 120,
                        startChar: 4,
                        endLine: 120,
                        endChar: 22
                    ),
                ]
            ),
        ],
        workspaceSymbols: [
            TraceyLspSymbol(
                name: ruleId.base,
                kind: "definition",
                path: "docs/content/spec.md",
                startLine: 1021,
                startChar: 2,
                endLine: 1021,
                endChar: 28
            ),
        ]
    )
}

private func sampleHelixTraceSnapshot() -> HelixTraceSnapshot {
    let pulseId: UInt64 = 17
    let audioRange = HelixAudioTokenRange(start: 32, end: 40)

    return HelixTraceSnapshot(
        meta: HelixStreamMeta(
            schemaVersion: 1,
            pulseIds: [16, pulseId],
            timelineEventCount: 420,
            attentionBatchCount: 17
        ),
        runInfo: HelixRunInfo(
            backend: "metal",
            modelDir: "/weights/qwen3-asr",
            input: "/audio/sample.wav",
            piece: "ceramic",
            pulseMs: 120,
            audioRingCapacity: 512,
            textRingCapacity: 256,
            commitRevisableTailTextTokens: 8,
            reviseLogitMargin: 1.5,
            sampleRate: 16_000,
            melHopSamples: 160,
            numMelBins: 128,
            numMelFrames: 2_048,
            audioTokensPerChunk: 8,
            nativeWindowTokens: 64,
            realtimePacing: true,
            profilePhases: false,
            attentionTraceSchemaVersion: 2,
            traceServerSchemaVersion: 1
        ),
        rollup: HelixPulseRollup(
            pulseId: pulseId,
            pulseStartUs: 1_000_000,
            pulseDurationUs: 44_000,
            encoderDurationUs: 12_000,
            refreshDurationUs: 8_000,
            verifyDurationUs: 4_000,
            decodeDurationUs: 16_000,
            commitDurationUs: 1_000,
            pulseMelFrames: 24,
            committedTokens: 3,
            retainedSpeculativeTokens: 5,
            residentCommittedTokens: 80,
            evictedAudioTokens: 2,
            evictedCommittedTokens: 1,
            decodedTokens: 6,
            hitEos: false,
            verify: HelixVerifyOutcome(
                rewindK: 2,
                acceptedPrefixLen: 3,
                divergenceRow: 4,
                discardedSpeculativeTokens: nil
            ),
            hasAttentionBatch: true,
            arTokenCount: 6
        ),
        promptLayout: HelixPromptLayout(
            pulseId: pulseId,
            firstAudioTokenId: audioRange.start,
            residentAudioFrames: 8,
            changedAudioSpans: [
                HelixAudioRepresentationSpan(
                    audio: audioRange,
                    audioRepresentationVersion: 3
                ),
            ],
            textTokenStart: 90,
            textTokenEnd: 92,
            textTokens: [
                HelixTextTokenSnapshot(
                    textTokenId: 90,
                    text: "pho",
                    textBefore: "fo",
                    inVerifyBatch: true,
                    decodedThisPulse: true
                ),
                HelixTextTokenSnapshot(
                    textTokenId: 91,
                    text: "n",
                    textBefore: nil,
                    inVerifyBatch: false,
                    decodedThisPulse: true
                ),
            ]
        ),
        attentionHeatmap: HelixPulseAttentionHeatmap(
            pulseId: pulseId,
            firstAudioTokenId: audioRange.start,
            audioTokenCount: 4,
            textTokenStart: 90,
            textTokenCount: 2,
            recordCount: 8,
            maxValue: 0.75,
            meanAudioMass: [0.1, 0.2, 0.3, 0.4, 0.05, 0.15, 0.25, 0.35],
            textTokenGlyphs: ["pho", "n"]
        ),
        streamMetrics: HelixStreamMetrics(
            pulseIds: [16, pulseId],
            pulseDurationUs: [42_000, 44_000],
            decodedTokens: [5, 6],
            committedTokens: [2, 3],
            retainedSpeculativeTokens: [4, 5],
            evictedAudioTokens: [0, 2],
            evictedCommittedTokens: [0, 1],
            rewindK: [0, 2],
            arTokenCount: [5, 6],
            rollingWer: [0.18, 0.16],
            s2dP50Ms: [220.0, 210.0]
        ),
        pulseAvailable: HelixPulseAvailable(pulseId: pulseId)
    )
}

// r[verify descriptors.fact-driven]
// r[verify type-system.rust-subset]
// r[verify exec.jit-optional]
@Test
func swiftDodecaStringSetFixtureRoundTripsAcrossEngines() throws {
    let setup = dodecaRoutesDescriptor()
    try assertTypedEquivalence(
        DodecaRoutes(routes: Set(["/guide/", "/"])),
        descriptor: setup.root,
        registry: setup.registry,
        "dodeca routes set"
    )
}

// r[verify descriptors.fact-driven]
// r[verify type-system.dynamic]
// r[verify exec.jit-optional]
@Test
func swiftDodecaTemplateCallFixtureRoundTripsAcrossEngines() throws {
    let setup = dodecaTemplateCallDescriptor()

    try assertTypedEquivalence(
        sampleDodecaTemplateCall(),
        descriptor: setup.root,
        registry: setup.registry,
        "dodeca dynamic template call"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "dodeca.template_call", phase: "fixture")
    #expect(report.isEmpty, "Dodeca dynamic template calls should stay native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify type-system.dynamic]
// r[verify type-system.variant-payloads]
// r[verify exec.jit-optional]
@Test
func swiftDodecaLoadDataResultFixtureRoundTripsAcrossEngines() throws {
    let setup = dodecaLoadDataResultDescriptor()

    try assertTypedEquivalence(
        sampleDodecaLoadDataResult(),
        descriptor: setup.root,
        registry: setup.registry,
        "dodeca data-loader result"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "dodeca.load_data", phase: "fixture")
    #expect(report.isEmpty, "Dodeca data-loader results should stay native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify descriptors.thunk-binding]
// r[verify type-system.dynamic]
// r[verify type-system.variant-payloads]
// r[verify exec.jit-optional]
@Test
func swiftDodecaParseResultFixtureRoundTripsAcrossEngines() throws {
    let setup = dodecaParseResultDescriptor()

    try assertTypedEquivalence(
        sampleDodecaParseResult(),
        descriptor: setup.root,
        registry: setup.registry,
        "dodeca parse result"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "dodeca.markdown.parse", phase: "fixture")
    #expect(report.isEmpty, "Dodeca parse results should stay native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify type-system.variant-payloads]
// r[verify exec.jit-optional]
@Test
func swiftDodecaImageProcessorFixtureRoundTripsAcrossEngines() throws {
    let setup = dodecaImageProcessorFixtureDescriptor()

    try assertTypedEquivalence(
        sampleDodecaImageProcessorFixture(),
        descriptor: setup.root,
        registry: setup.registry,
        "dodeca image processor roots"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "dodeca.image.process", phase: "fixture")
    #expect(report.isEmpty, "Dodeca image processor roots should stay native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify type-system.variant-payloads]
// r[verify exec.jit-optional]
@Test
func swiftDodecaSearchIndexerFixtureRoundTripsAcrossEngines() throws {
    let setup = dodecaSearchIndexerFixtureDescriptor()

    try assertTypedEquivalence(
        sampleDodecaSearchIndexerFixture(),
        descriptor: setup.root,
        registry: setup.registry,
        "dodeca search indexer roots"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "dodeca.search.build_index", phase: "fixture")
    #expect(report.isEmpty, "Dodeca search indexer roots should stay native-clean in the Swift JIT")
}

// r[verify compat.type-match]
// r[verify descriptors.fact-driven]
// r[verify descriptors.thunk-binding]
// r[verify validate.uniqueness]
// r[verify exec.jit-optional]
@Test
func swiftDodecaHtmlProcessInputFixtureRoundTripsAcrossEngines() throws {
    let setup = dodecaHtmlProcessInputDescriptor()

    try assertTypedEquivalence(
        sampleDodecaHtmlProcessInput(),
        descriptor: setup.root,
        registry: setup.registry,
        "dodeca html processor"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "dodeca.html.process", phase: "fixture")
    #expect(report.isEmpty, "Dodeca HTML processor maps/sets/tuple vectors should stay native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify ir.memory]
// r[verify type-system.variant-payloads]
// r[verify exec.jit-optional]
@Test
func swiftDibsSqlValueRowsFixtureRoundTripsAcrossEngines() throws {
    let setup = dibsListResponseDescriptor()

    try assertTypedEquivalence(
        sampleDibsListResponse(),
        descriptor: setup.root,
        registry: setup.registry,
        "dibs sql rows"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "dibs.list", phase: "response")
    #expect(report.isEmpty, "Dibs SQL value rows should stay native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify type-system.variant-payloads]
// r[verify codegen.schema-is-source-of-truth]
// r[verify exec.jit-optional]
@Test
func swiftDibsSquelServiceFixtureRoundTripsAcrossEngines() throws {
    let setup = dibsSquelServiceDescriptor()

    try assertTypedEquivalence(
        sampleDibsSquelServiceFixture(),
        descriptor: setup.root,
        registry: setup.registry,
        "dibs generated squel service roots"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "dibs.squel", phase: "fixture")
    #expect(report.isEmpty, "Dibs generated Squel service roots should stay native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify type-system.variant-payloads]
// r[verify type-system.channel]
// r[verify exec.jit-optional]
@Test
func swiftDibsMigrationServiceFixtureRoundTripsAcrossEngines() throws {
    let setup = dibsMigrationServiceDescriptor()

    try assertTypedEquivalence(
        sampleDibsMigrationServiceFixture(),
        descriptor: setup.root,
        registry: setup.registry,
        "dibs migration service roots"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "dibs.migration", phase: "fixture")
    #expect(report.isEmpty, "Dibs migration service roots should stay native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify type-system.variant-payloads]
// r[verify exec.jit-optional]
@Test
func swiftHotmealLiveReloadFixtureRoundTripsAcrossEngines() throws {
    let setup = hotmealLiveReloadFixtureDescriptor()

    try assertTypedEquivalence(
        sampleHotmealLiveReloadFixture(),
        descriptor: setup.root,
        registry: setup.registry,
        "hotmeal live reload"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "hotmeal.live_reload", phase: "fixture")
    #expect(report.isEmpty, "Hotmeal live-reload payloads should stay native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify type-system.variant-payloads]
// r[verify exec.jit-optional]
@Test
func swiftTraceyMigrationFixtureRoundTripsAcrossEngines() throws {
    let setup = traceyMigrationFixtureDescriptor()

    try assertTypedEquivalence(
        sampleTraceyMigrationFixture(),
        descriptor: setup.root,
        registry: setup.registry,
        "tracey migration"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "tracey.migration", phase: "fixture")
    #expect(report.isEmpty, "Tracey migration DTOs should stay native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify ir.memory]
// r[verify exec.jit-optional]
@Test
func swiftHelixTraceSnapshotFixtureRoundTripsAcrossEngines() throws {
    let setup = helixTraceSnapshotDescriptor()

    try assertTypedEquivalence(
        sampleHelixTraceSnapshot(),
        descriptor: setup.root,
        registry: setup.registry,
        "helix trace snapshot"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "helix.trace_snapshot", phase: "fixture")
    #expect(report.isEmpty, "Helix trace-server payloads should stay native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify ir.memory]
// r[verify type-system.variant-payloads]
// r[verify type-system.dynamic]
// r[verify exec.jit-optional]
// r[verify typed.no-dynamic-bounce]
@Test
func swiftHelixTraceServiceSurfaceFixtureRoundTripsAcrossEngines() throws {
    let setup = helixTraceServiceSurfaceDescriptor()

    try assertTypedEquivalence(
        sampleHelixTraceServiceSurface(),
        descriptor: setup.root,
        registry: setup.registry,
        "helix trace service surface"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "helix.trace_service_surface", phase: "fixture")
    #expect(report.isEmpty, "Helix TraceService aggregate payloads should stay native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify ir.memory]
// r[verify exec.jit-optional]
// r[verify exec.strict-recording]
@Test
func swiftStaxRecursiveFlamegraphFixtureRoundTripsAcrossEngines() throws {
    let setup = staxDescriptor()
    let value = StaxFlamegraphUpdate(
        totalOnCpuNs: 9_000,
        strings: ["main", "poll", "libbee.dylib"],
        root: FlameNode(
            address: 0x1000,
            functionName: 0,
            binary: 2,
            onCpuNs: 9_000,
            offCpu: OffCpuBreakdown(sleepNs: 100, ioNs: 200, mutexNs: 300),
            children: [
                FlameNode(
                    address: 0x1040,
                    functionName: 1,
                    binary: 2,
                    onCpuNs: 4_500,
                    offCpu: OffCpuBreakdown(sleepNs: 10, ioNs: 20, mutexNs: 30),
                    children: []
                ),
            ]
        )
    )

    try assertTypedEquivalence(
        value,
        descriptor: setup.root,
        registry: setup.registry,
        "stax recursive flamegraph",
        blocks: setup.blocks
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry,
        blocks: setup.blocks
    ).scoped(method: "stax.flamegraph", phase: "fixture")
    #expect(report.isEmpty, "recursive Stax fixture should be native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify ir.memory]
// r[verify type-system.variant-payloads]
// r[verify exec.jit-optional]
// r[verify exec.strict-recording]
@Test
func swiftStaxLinuxBrokerControlFixtureRoundTripsAcrossEngines() throws {
    let setup = staxLinuxBrokerControlDescriptor()

    try assertTypedEquivalence(
        sampleStaxLinuxBrokerControlFixture(),
        descriptor: setup.root,
        registry: setup.registry,
        "stax linux broker control"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "stax.linux.broker_control", phase: "fixture")
    #expect(report.isEmpty, "Stax Linux broker-control DTOs should stay native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify ir.memory]
// r[verify type-system.channel]
// r[verify exec.jit-optional]
// r[verify exec.strict-recording]
@Test
func swiftStaxMacKdBufBatchFixtureRoundTripsAcrossEngines() throws {
    let setup = staxMacKdBufBatchDescriptor()

    try assertTypedEquivalence(
        sampleStaxMacKdBufBatch(),
        descriptor: setup.root,
        registry: setup.registry,
        "stax macOS KdBufBatch stream item"
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry
    ).scoped(method: "stax.macos.kdbuf_batch", phase: "fixture")
    #expect(report.isEmpty, "Stax macOS KdBufBatch stream item should stay native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify ir.memory]
// r[verify type-system.variant-payloads]
// r[verify exec.jit-optional]
// r[verify exec.strict-recording]
@Test
func swiftStyxRecursiveValueFixtureRoundTripsAcrossEngines() throws {
    let setup = styxDescriptor()

    try assertTypedEquivalence(
        sampleStyxValue(),
        descriptor: setup.root,
        registry: setup.registry,
        "styx recursive value",
        blocks: setup.blocks
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry,
        blocks: setup.blocks
    ).scoped(method: "styx.value", phase: "fixture")
    #expect(report.isEmpty, "recursive Styx fixture should be native-clean in the Swift JIT")
}

// r[verify descriptors.fact-driven]
// r[verify ir.memory]
// r[verify type-system.variant-payloads]
// r[verify exec.jit-optional]
// r[verify exec.strict-recording]
@Test
func swiftStyxLspSurfaceFixtureRoundTripsAcrossEngines() throws {
    let setup = styxLspSurfaceDescriptor()

    try assertTypedEquivalence(
        sampleStyxLspSurfaceFixture(),
        descriptor: setup.root,
        registry: setup.registry,
        "styx lsp surface",
        blocks: setup.blocks
    )

    let report = try nativeFallbackReport(
        descriptor: setup.root,
        registry: setup.registry,
        blocks: setup.blocks
    ).scoped(method: "styx.lsp.surface", phase: "fixture")
    #expect(report.isEmpty, "recursive Styx LSP fixture should be native-clean in the Swift JIT")
}

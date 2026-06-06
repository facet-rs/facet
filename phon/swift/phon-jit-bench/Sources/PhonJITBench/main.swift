import Dispatch
import Foundation
import PhonDibsEcosystemFixtures
import PhonEcosystemFixtures
import PhonEngine
import PhonIR
import PhonJIT
import PhonSchema

nonisolated(unsafe)
private var blackHoleSink: UInt = 0

@inline(never)
private func blackHole<T>(_ value: T) {
    withUnsafePointer(to: value) { pointer in
        blackHoleSink &+= UInt(bitPattern: pointer)
    }
}

@discardableResult
private func bench(_ label: String, iters: Int, _ body: () throws -> Void) rethrows -> Double {
    for _ in 0..<max(iters / 20, 1_000) {
        try body()
    }

    let started = DispatchTime.now().uptimeNanoseconds
    for _ in 0..<iters {
        try body()
    }
    let elapsed = DispatchTime.now().uptimeNanoseconds - started
    let ns = Double(elapsed) / Double(iters)
    let padded = label.padding(toLength: 22, withPad: " ", startingAt: 0)
    print("  \(padded) \(String(format: "%10.1f", ns)) ns/op")
    return ns
}

private func decode<T>(_ type: T.Type, _ decoder: TypedDecodeFn, _ bytes: [UInt8]) throws -> T {
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<T>.size,
        alignment: MemoryLayout<T>.alignment
    )
    defer { raw.deallocate() }
    try decoder(bytes, raw)
    return raw.assumingMemoryBound(to: T.self).move()
}

private func benchCase<T: Equatable>(
    _ label: String,
    method: String,
    phase: String,
    setup: (Descriptor, Registry),
    value: T,
    iters: Int,
    blocks: [SchemaId: Descriptor] = [:],
    allowFallback: Bool = false
) throws {
    let lowered = try lowerTyped(setup.0, setup.1, blocks)
    let report = PhonJIT.nativeFallbackReport(lowered).scoped(method: method, phase: phase)
    if report.isEmpty {
        print("\(label)  ->  native-clean")
    } else if allowFallback {
        print("\(label)  ->  fallback report: \(report)")
    } else {
        preconditionFailure("\(method) \(phase) has native JIT fallbacks: \(report)")
    }

    let interpreter = InterpreterEngine()
    let jit = JITEngine()
    let interpEncode = try interpreter.compileEncode(lowered)
    let interpDecode = try interpreter.compileDecode(lowered)
    let jitEncode = try jit.compileEncode(lowered)
    let jitDecode = try jit.compileDecode(lowered)

    var stored = value
    let interpWire = withUnsafeBytes(of: &stored) { interpEncode($0.baseAddress!) }
    let jitWire = withUnsafeBytes(of: &stored) { jitEncode($0.baseAddress!) }
    precondition(jitWire == interpWire, "\(label): JIT encode mismatch")

    let interpDecoded: T = try decode(T.self, interpDecode, interpWire)
    precondition(interpDecoded == value, "\(label): interpreter decode mismatch")
    let jitDecoded: T = try decode(T.self, jitDecode, interpWire)
    precondition(jitDecoded == value, "\(label): JIT decode mismatch")

    print("  wire bytes             \(interpWire.count)")

    let encI = bench("encode interpreter", iters: iters) {
        let bytes = withUnsafeBytes(of: &stored) { interpEncode($0.baseAddress!) }
        blackHole(bytes)
    }
    let decI = try bench("decode interpreter", iters: iters) {
        let decoded: T = try decode(T.self, interpDecode, interpWire)
        blackHole(decoded)
    }
    let encJ = bench("encode jit", iters: iters) {
        let bytes = withUnsafeBytes(of: &stored) { jitEncode($0.baseAddress!) }
        blackHole(bytes)
    }
    let decJ = try bench("decode jit", iters: iters) {
        let decoded: T = try decode(T.self, jitDecode, interpWire)
        blackHole(decoded)
    }

    let encSpeedup = String(format: "%5.2f", encI / encJ)
    let decSpeedup = String(format: "%5.2f", decI / decJ)
    print("  speedup               encode \(encSpeedup)x   decode \(decSpeedup)x\n")
}

private func benchDecodeCase<T: Equatable>(
    _ label: String,
    method: String,
    phase: String,
    writerRoot: SchemaId,
    reader: Descriptor,
    registry: Registry,
    bytes: [UInt8],
    expected: T,
    iters: Int
) throws {
    let lowered = try lowerDecode(writerRoot, reader, registry)
    let report = PhonJIT.nativeFallbackReport(lowered).scoped(method: method, phase: phase)
    let decodeFallbacks = report.records.filter { $0.direction == "decode" }
    if decodeFallbacks.isEmpty {
        print("\(label)  ->  native-clean decode")
    } else {
        preconditionFailure("\(method) \(phase) has native decode fallbacks: \(decodeFallbacks)")
    }

    let interpreter = InterpreterEngine()
    let jit = JITEngine()
    let interpDecode = try interpreter.compileDecode(lowered)
    let jitDecode = try jit.compileDecode(lowered)

    let interpDecoded: T = try decode(T.self, interpDecode, bytes)
    precondition(interpDecoded == expected, "\(label): interpreter decode mismatch")
    let jitDecoded: T = try decode(T.self, jitDecode, bytes)
    precondition(jitDecoded == expected, "\(label): JIT decode mismatch")

    print("  wire bytes             \(bytes.count)")

    let decI = try bench("decode interpreter", iters: iters) {
        let decoded: T = try decode(T.self, interpDecode, bytes)
        blackHole(decoded)
    }
    let decJ = try bench("decode jit", iters: iters) {
        let decoded: T = try decode(T.self, jitDecode, bytes)
        blackHole(decoded)
    }

    let decSpeedup = String(format: "%5.2f", decI / decJ)
    print("  speedup               decode \(decSpeedup)x\n")
}

private func boolDesc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.bool)), layout: Layout(size: 1, align: 1), access: .scalar)
}

private func u8Desc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.u8)), layout: Layout(size: 1, align: 1), access: .scalar)
}

private func u32Desc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.u32)), layout: Layout(size: 4, align: 4), access: .scalar)
}

private func u64Desc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.u64)), layout: Layout(size: 8, align: 8), access: .scalar)
}

private func i16Desc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.i16)), layout: Layout(size: 2, align: 2), access: .scalar)
}

private func i32Desc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.i32)), layout: Layout(size: 4, align: 4), access: .scalar)
}

private func i64Desc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.i64)), layout: Layout(size: 8, align: 8), access: .scalar)
}

private func f32Desc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.f32)), layout: Layout(size: 4, align: 4), access: .scalar)
}

private func f64Desc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.f64)), layout: Layout(size: 8, align: 8), access: .scalar)
}

private func stringWitness() -> BytesWitness {
    BytesWitness(
        count: { field in field.assumingMemoryBound(to: String.self).pointee.utf8.count },
        copyInto: { field, dst in
            var string = field.assumingMemoryBound(to: String.self).pointee
            string.withUTF8 { buf in
                if buf.count > 0 {
                    dst.copyMemory(from: buf.baseAddress!, byteCount: buf.count)
                }
            }
        },
        construct: { field, src, count in
            let buf = UnsafeBufferPointer(start: src.assumingMemoryBound(to: UInt8.self), count: count)
            guard let string = String(validating: buf, as: UTF8.self) else {
                return false
            }
            field.assumingMemoryBound(to: String.self).initialize(to: string)
            return true
        }
    )
}

private func stringDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(primitiveId(.string)),
        layout: Layout(size: MemoryLayout<String>.size, align: MemoryLayout<String>.alignment),
        access: .bytes(BytesAccess(stride: 1, elemAlign: 1, witness: stringWitness()))
    )
}

private func bytesDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(primitiveId(.bytes)),
        layout: MemoryLayout<[UInt8]>.phonLayout,
        access: .bytes(BytesAccess(stride: 1, elemAlign: 1, witness: .byteArray))
    )
}

private func floatArrayWitness() -> BytesWitness {
    BytesWitness(
        count: { field in field.assumingMemoryBound(to: [Float].self).pointee.count },
        copyInto: { field, dst in
            field.assumingMemoryBound(to: [Float].self).pointee.withUnsafeBytes { buf in
                if buf.count > 0 {
                    dst.copyMemory(from: buf.baseAddress!, byteCount: buf.count)
                }
            }
        },
        construct: { field, src, count in
            field.assumingMemoryBound(to: [Float].self).initialize(to: Array(unsafeUninitializedCapacity: count) { dst, n in
                if count > 0 {
                    dst.baseAddress!.initialize(from: src.assumingMemoryBound(to: Float.self), count: count)
                }
                n = count
            })
            return true
        }
    )
}

private func arraySeqWitness<T>(of _: T.Type) -> SeqWitness {
    SeqWitness(
        count: { handle in handle.assumingMemoryBound(to: [T].self).pointee.count },
        copyElements: { handle, dst in
            handle.assumingMemoryBound(to: [T].self).pointee.withUnsafeBytes { buf in
                if buf.count > 0 {
                    dst.copyMemory(from: buf.baseAddress!, byteCount: buf.count)
                }
            }
        },
        construct: { handle, src, count in
            handle.assumingMemoryBound(to: [T].self).initialize(to: Array(unsafeUninitializedCapacity: count) { dst, n in
                if count > 0 {
                    dst.baseAddress!.moveInitialize(from: src.assumingMemoryBound(to: T.self), count: count)
                }
                n = count
            })
        }
    )
}

private struct MarkedTextArgs: Equatable {
    var text: String
    var animationBudgetMs: UInt32
}

private struct AdvanceTranscriptArgs: Equatable {
    var text: String
    var committedLen: UInt32
    var animationBudgetMs: UInt32
}

private struct ImeKeyEventArgs: Equatable {
    var eventType: String
    var keyCode: UInt32
    var characters: String
}

private struct FeedArgsHot: Equatable {
    var sessionId: String
    var samples: [Float]
}

private struct ConfidenceHot: Equatable {
    var meanLp: Float
    var minLp: Float
    var meanM: Float
    var minM: Float
}

private struct AlignedWordHot: Equatable {
    var word: String
    var start: Double
    var end: Double
    var confidence: ConfidenceHot
}

private struct CorrectionEditHot: Equatable {
    var editId: String
    var spanStart: UInt32
    var spanEnd: UInt32
    var original: String
    var replacement: String
    var term: String
    var aliasId: Int32
    var rankerProb: Double
    var gateProb: Double
}

private struct FeedResultHot: Equatable {
    var text: String
    var committedUtf16Len: UInt32
    var alignments: [AlignedWordHot]
    var isFinal: Bool
    var detectedLanguage: String
    var correctionEdits: [CorrectionEditHot]
    var correctionSessionId: String
}

private enum BeeErrorHot: Equatable {
    case engineNotLoaded
    case sessionNotFound(String)
    case loadFailed(String)
    case transcriptionError(String)
    case correctionError(String)
    case notImplemented
}

private enum FeedResponseHot: Equatable {
    case ok(FeedResultHot?)
    case err(BeeErrorHot)
}

private struct DodecaRoutesHot: Equatable {
    var routes: Set<String>
}

private struct DodecaResolvedDependencyHot: Equatable {
    var name: String
    var version: String?
}

private struct DodecaCodeExecutionMetadataHot: Equatable {
    var language: String
    var dependencies: [DodecaResolvedDependencyHot]
    var durationMs: UInt64
}

private enum DodecaInjectionLocationHot: Equatable {
    case head
    case body
}

private struct DodecaInjectionHot: Equatable {
    var location: DodecaInjectionLocationHot
    var content: String
}

private struct DodecaStringU32Hot: Equatable {
    var string: String
    var value: UInt32
}

private struct DodecaResponsiveImageInfoHot: Equatable {
    var jxlSrcset: [DodecaStringU32Hot]
    var webpSrcset: [DodecaStringU32Hot]
}

private struct DodecaMountLocalizationHot: Equatable {
    var segment: String
    var routes: Set<String>
}

private struct DodecaHtmlProcessInputHot: Equatable {
    var html: String
    var pathMap: [String: String]?
    var knownRoutes: Set<String>?
    var codeMetadata: [String: DodecaCodeExecutionMetadataHot]?
    var injections: [DodecaInjectionHot]
    var imageVariants: [String: DodecaResponsiveImageInfoHot]?
    var viteCssMap: [String: [String]]?
    var mount: DodecaMountLocalizationHot?
}

private struct DodecaStringValueHot: Equatable {
    var key: String
    var value: Value
}

private struct DodecaTemplateCallHot: Equatable {
    var contextId: String
    var name: String
    var args: [Value]
    var kwargs: [DodecaStringValueHot]
}

private enum DodecaLoadDataResultHot: Equatable {
    case success(value: Value)
    case error(message: String)
}

private struct DodecaMarkdownHeadingHot: Equatable {
    var title: String
    var id: String
    var level: UInt8
}

private struct DodecaReqDefinitionHot: Equatable {
    var id: String
    var anchorId: String
}

private enum DodecaSourceKindHot: Equatable {
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

private struct DodecaSourceMapEntryHot: Equatable {
    var id: String
    var kind: DodecaSourceKindHot
    var lineStart: UInt32
    var lineEnd: UInt32
    var byteStart: UInt64
    var byteEnd: UInt64
}

private struct DodecaSourceMapHot: Equatable {
    var sourcePath: String?
    var entries: [DodecaSourceMapEntryHot]
}

private struct DodecaFrontmatterHot: Equatable {
    var title: String
    var weight: Int32
    var description: String?
    var template: String?
    var extra: Value
}

private struct DodecaParseSuccessPayloadHot: Equatable {
    var frontmatter: DodecaFrontmatterHot
    var html: String
    var headings: [DodecaMarkdownHeadingHot]
    var reqs: [DodecaReqDefinitionHot]
    var headInjections: [String]
    var sourceMap: DodecaSourceMapHot
}

private struct DodecaParseErrorPayloadHot: Equatable {
    var message: String
}

private enum DodecaParseResultHot: Equatable {
    case success(DodecaParseSuccessPayloadHot)
    case error(DodecaParseErrorPayloadHot)
}

private struct DodecaDecodedImageHot: Equatable {
    var pixels: [UInt8]
    var width: UInt32
    var height: UInt32
    var channels: UInt8
}

private struct DodecaImageSuccessPayloadHot: Equatable {
    var image: DodecaDecodedImageHot
}

private struct DodecaThumbhashSuccessPayloadHot: Equatable {
    var dataUrl: String
}

private struct DodecaImageErrorPayloadHot: Equatable {
    var message: String
}

private enum DodecaImageResultHot: Equatable {
    case success(DodecaImageSuccessPayloadHot)
    case thumbhashSuccess(DodecaThumbhashSuccessPayloadHot)
    case error(DodecaImageErrorPayloadHot)
}

private struct DodecaResizeInputHot: Equatable {
    var pixels: [UInt8]
    var width: UInt32
    var height: UInt32
    var channels: UInt8
    var targetWidth: UInt32
}

private struct DodecaThumbhashInputHot: Equatable {
    var pixels: [UInt8]
    var width: UInt32
    var height: UInt32
}

private struct DodecaImageProcessorFixtureHot: Equatable {
    var pngData: [UInt8]
    var decodedResult: DodecaImageResultHot
    var resizeInput: DodecaResizeInputHot
    var resizeResult: DodecaImageResultHot
    var thumbhashInput: DodecaThumbhashInputHot
    var thumbhashResult: DodecaImageResultHot
    var errorResult: DodecaImageResultHot
}

private struct DodecaSearchPageHot: Equatable {
    var url: String
    var source: String
    var html: String
}

private struct DodecaSearchFileHot: Equatable {
    var path: String
    var contents: [UInt8]
}

private struct DodecaSearchSuccessPayloadHot: Equatable {
    var files: [DodecaSearchFileHot]
}

private struct DodecaSearchErrorPayloadHot: Equatable {
    var message: String
}

private enum DodecaSearchIndexResultHot: Equatable {
    case success(DodecaSearchSuccessPayloadHot)
    case error(DodecaSearchErrorPayloadHot)
}

private struct DodecaSearchIndexerFixtureHot: Equatable {
    var pages: [DodecaSearchPageHot]
    var result: DodecaSearchIndexResultHot
    var errorResult: DodecaSearchIndexResultHot
}

private enum DodecaBenchId {
    static let optionString = SchemaId(10)
    static let resolvedDependency = SchemaId(11)
    static let resolvedDependencyList = SchemaId(12)
    static let codeExecutionMetadata = SchemaId(13)
    static let injectionLocation = SchemaId(14)
    static let injection = SchemaId(15)
    static let injectionList = SchemaId(16)
    static let stringU32Tuple = SchemaId(17)
    static let stringU32TupleList = SchemaId(18)
    static let responsiveImageInfo = SchemaId(19)
    static let routeSet = SchemaId(20)
    static let mountLocalization = SchemaId(21)
    static let mapStringString = SchemaId(22)
    static let optionMapStringString = SchemaId(23)
    static let optionStringSet = SchemaId(24)
    static let mapStringCodeMetadata = SchemaId(25)
    static let optionMapStringCodeMetadata = SchemaId(26)
    static let mapStringResponsiveImageInfo = SchemaId(27)
    static let optionMapStringResponsiveImageInfo = SchemaId(28)
    static let stringList = SchemaId(29)
    static let mapStringStringList = SchemaId(30)
    static let optionMapStringStringList = SchemaId(31)
    static let optionMountLocalization = SchemaId(32)
    static let htmlProcessInput = SchemaId(33)
    static let dynamic = SchemaId(34)
    static let dynamicList = SchemaId(35)
    static let stringValueTuple = SchemaId(36)
    static let stringValueTupleList = SchemaId(37)
    static let templateCall = SchemaId(38)
    static let loadDataResult = SchemaId(39)
    static let markdownHeading = SchemaId(40)
    static let markdownHeadingList = SchemaId(41)
    static let reqDefinition = SchemaId(42)
    static let reqDefinitionList = SchemaId(43)
    static let sourceKind = SchemaId(44)
    static let sourceMapEntry = SchemaId(45)
    static let sourceMapEntryList = SchemaId(46)
    static let sourceMap = SchemaId(47)
    static let frontmatter = SchemaId(48)
    static let parseResult = SchemaId(49)
    static let decodedImage = SchemaId(50)
    static let imageResult = SchemaId(51)
    static let resizeInput = SchemaId(52)
    static let thumbhashInput = SchemaId(53)
    static let imageProcessorFixture = SchemaId(54)
    static let searchPage = SchemaId(55)
    static let searchPageList = SchemaId(56)
    static let searchFile = SchemaId(57)
    static let searchFileList = SchemaId(58)
    static let searchIndexResult = SchemaId(59)
    static let searchIndexerFixture = SchemaId(60)
}

private enum DibsSqlValueHot: Equatable {
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

private struct DibsRowFieldHot: Equatable {
    var name: String
    var value: DibsSqlValueHot
}

private struct DibsListResponseHot: Equatable {
    var rows: [[DibsRowFieldHot]]
    var total: UInt64?
}

private struct OffCpuBreakdownHot: Equatable {
    var sleepNs: UInt64
    var ioNs: UInt64
    var mutexNs: UInt64
}

private struct FlameNodeHot: Equatable {
    var address: UInt64
    var functionName: UInt32?
    var binary: UInt32?
    var onCpuNs: UInt64
    var offCpu: OffCpuBreakdownHot
    var children: [FlameNodeHot]
}

private struct StaxFlamegraphUpdateHot: Equatable {
    var totalOnCpuNs: UInt64
    var strings: [String]
    var root: FlameNodeHot
}

private struct StaxLinuxPerfSessionConfigHot: Equatable {
    var targetPid: UInt32
    var frequencyHz: UInt32
    var kernelStacks: Bool
    var requestWaking: Bool
    var requestPmu: Bool
    var requestDwarfUnwind: Bool
}

private struct StaxLinuxWakingFieldOffsetsHot: Equatable {
    var wakeePidOffset: UInt32
    var wakeePidSize: UInt32
}

private enum StaxLinuxPerfSessionErrorHot: Equatable {
    case notPrivileged(detail: String)
    case perfEventOpen(cpu: UInt32, errno: Int32, detail: String)
    case noSuchTarget(UInt32)
    case notAuthorized(callerUid: UInt32, targetUid: UInt32)
}

private struct StaxLinuxDaemonStatusHot: Equatable {
    var version: String
    var hostArch: String
    var privileged: Bool
    var perfEventParanoid: Int32
}

private struct StaxLinuxBrokerControlFixtureHot: Equatable {
    var config: StaxLinuxPerfSessionConfigHot
    var status: StaxLinuxDaemonStatusHot
    var errors: [StaxLinuxPerfSessionErrorHot]
    var wakingFieldOffsets: StaxLinuxWakingFieldOffsetsHot?
}

private struct StaxNotPrivilegedPayloadHot {
    var detail: String
}

private struct StaxPerfEventOpenPayloadHot {
    var cpu: UInt32
    var errno: Int32
    var detail: String
}

private struct StaxNotAuthorizedPayloadHot {
    var callerUid: UInt32
    var targetUid: UInt32
}

private struct DibsMigrationLogHot: Equatable {
    var version: UInt64
    var level: String
    var message: String
    var appliedCount: UInt32
    var elapsedMs: UInt64
}

private struct HelixPulseAvailableHot: Equatable {
    var pulseId: UInt64
}

private struct TraceyRuleIdHot: Equatable {
    var base: String
    var version: UInt32
}

private struct TraceyDataUpdateHot: Equatable {
    var version: UInt64
    var newlyCovered: [TraceyRuleIdHot]
    var newlyUncovered: [TraceyRuleIdHot]
}

private struct CompatReaderXHot: Equatable {
    var x: UInt32
}

private struct CompatReaderXCHot: Equatable {
    var x: UInt32
    var c: UInt32?
}

private struct CompatMovePayloadHot: Equatable {
    var y: UInt32
    var x: UInt32
    var extra: UInt32?
}

private enum CompatCommandHot: Equatable {
    case move(CompatMovePayloadHot)
    case stop
}

private enum CompatBenchId {
    static let writerXY = SchemaId(9_001)
    static let readerX = SchemaId(9_002)
    static let writerX = SchemaId(9_003)
    static let readerXC = SchemaId(9_004)
    static let optionU32 = SchemaId(9_005)
    static let writerCommand = SchemaId(9_006)
    static let readerCommand = SchemaId(9_007)
}

private func compatWriterOnlySkipFixture() -> (
    writerRoot: SchemaId,
    reader: Descriptor,
    registry: Registry,
    bytes: [UInt8],
    expected: CompatReaderXHot
) {
    let schemas = resolveIds([
        Schema(id: CompatBenchId.writerXY, kind: .structure(name: "CompatP", fields: [
            Field(name: "x", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "y", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: CompatBenchId.readerX, kind: .structure(name: "CompatP", fields: [
            Field(name: "x", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
    ])
    let reader = Descriptor(
        schema: .concrete(schemas[1].id),
        layout: MemoryLayout<CompatReaderXHot>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<CompatReaderXHot>.offset(of: \CompatReaderXHot.x)!, descriptor: u32Desc()),
            ],
            construct: .inPlace
        ))
    )
    return (
        writerRoot: schemas[0].id,
        reader: reader,
        registry: Registry(schemas),
        bytes: [7, 0, 0, 0, 99, 0, 0, 0],
        expected: CompatReaderXHot(x: 7)
    )
}

private func compatReaderOnlyDefaultFixture() -> (
    writerRoot: SchemaId,
    reader: Descriptor,
    registry: Registry,
    bytes: [UInt8],
    expected: CompatReaderXCHot
) {
    let schemas = resolveIds([
        Schema(id: CompatBenchId.writerX, kind: .structure(name: "CompatP", fields: [
            Field(name: "x", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: CompatBenchId.readerXC, kind: .structure(name: "CompatP", fields: [
            Field(name: "x", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "c", schema: .concrete(CompatBenchId.optionU32), required: false),
        ])),
        Schema(id: CompatBenchId.optionU32, kind: .option(element: .concrete(primitiveId(.u32)))),
    ])
    let optDesc = Descriptor(
        schema: .concrete(schemas[2].id),
        layout: Layout(size: MemoryLayout<UInt32?>.size, align: MemoryLayout<UInt32?>.alignment),
        access: .option(OptionAccess(witness: .of(UInt32.self), some: u32Desc()))
    )
    let reader = Descriptor(
        schema: .concrete(schemas[1].id),
        layout: MemoryLayout<CompatReaderXCHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<CompatReaderXCHot>.offset(of: \CompatReaderXCHot.x)!, descriptor: u32Desc()),
            FieldAccess(
                offset: MemoryLayout<CompatReaderXCHot>.offset(of: \CompatReaderXCHot.c)!,
                descriptor: optDesc,
                defaultInit: { $0.assumingMemoryBound(to: UInt32?.self).initialize(to: nil) }
            ),
        ], construct: .inPlace))
    )
    return (
        writerRoot: schemas[0].id,
        reader: reader,
        registry: Registry(schemas),
        bytes: [7, 0, 0, 0],
        expected: CompatReaderXCHot(x: 7, c: nil)
    )
}

private func compatEnumPayloadDriftFixture() -> (
    writerRoot: SchemaId,
    reader: Descriptor,
    registry: Registry,
    bytes: [UInt8],
    expected: CompatCommandHot
) {
    let schemas = resolveIds([
        Schema(id: CompatBenchId.writerCommand, kind: .enumeration(name: "CmdCompat", variants: [
            Variant(name: "Move", index: 3, payload: .structure([
                Field(name: "x", schema: .concrete(primitiveId(.u32)), required: true),
                Field(name: "transient", schema: .concrete(primitiveId(.u64)), required: true),
                Field(name: "y", schema: .concrete(primitiveId(.u32)), required: true),
            ])),
            Variant(name: "Stop", index: 4, payload: .unit),
        ])),
        Schema(id: CompatBenchId.optionU32, kind: .option(element: .concrete(primitiveId(.u32)))),
        Schema(id: CompatBenchId.readerCommand, kind: .enumeration(name: "CmdCompat", variants: [
            Variant(name: "Move", index: 0, payload: .structure([
                Field(name: "y", schema: .concrete(primitiveId(.u32)), required: true),
                Field(name: "x", schema: .concrete(primitiveId(.u32)), required: true),
                Field(name: "extra", schema: .concrete(CompatBenchId.optionU32), required: false),
            ])),
            Variant(name: "Stop", index: 1, payload: .unit),
        ])),
    ])
    let optionDesc = Descriptor(
        schema: .concrete(schemas[1].id),
        layout: Layout(size: MemoryLayout<UInt32?>.size, align: MemoryLayout<UInt32?>.alignment),
        access: .option(OptionAccess(witness: .of(UInt32.self), some: u32Desc()))
    )
    let payloadLayout = MemoryLayout<CompatMovePayloadHot>.phonLayout
    let reader = Descriptor(
        schema: .concrete(schemas[2].id),
        layout: MemoryLayout<CompatCommandHot>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: { value in
                switch value.assumingMemoryBound(to: CompatCommandHot.self).pointee {
                case .move: return 0
                case .stop: return 1
                }
            },
            projectPayload: { value, localIndex, scratch in
                guard localIndex == 0 else { return }
                guard case .move(let payload) = value.assumingMemoryBound(to: CompatCommandHot.self).pointee else {
                    return
                }
                scratch.assumingMemoryBound(to: CompatMovePayloadHot.self).initialize(to: payload)
            },
            destroyPayload: { scratch, localIndex in
                guard localIndex == 0 else { return }
                scratch.assumingMemoryBound(to: CompatMovePayloadHot.self).deinitialize(count: 1)
            },
            inject: { slot, localIndex, scratch in
                let value: CompatCommandHot
                switch localIndex {
                case 0:
                    value = .move(scratch.assumingMemoryBound(to: CompatMovePayloadHot.self).move())
                case 1:
                    value = .stop
                default:
                    fatalError("bad compat command variant")
                }
                slot.assumingMemoryBound(to: CompatCommandHot.self).initialize(to: value)
            },
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [
                        FieldAccess(offset: MemoryLayout<CompatMovePayloadHot>.offset(of: \CompatMovePayloadHot.y)!, descriptor: u32Desc()),
                        FieldAccess(offset: MemoryLayout<CompatMovePayloadHot>.offset(of: \CompatMovePayloadHot.x)!, descriptor: u32Desc()),
                        FieldAccess(
                            offset: MemoryLayout<CompatMovePayloadHot>.offset(of: \CompatMovePayloadHot.extra)!,
                            descriptor: optionDesc,
                            defaultInit: { $0.assumingMemoryBound(to: UInt32?.self).initialize(to: nil) }
                        ),
                    ],
                    payloadLayout: payloadLayout
                ),
                VariantAccess(wireIndex: 1, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
            ]
        ))
    )
    return (
        writerRoot: schemas[0].id,
        reader: reader,
        registry: Registry(schemas),
        bytes: [3, 0, 0, 0, 3, 0, 0, 0, 231, 3, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0],
        expected: .move(CompatMovePayloadHot(y: 4, x: 3, extra: nil))
    )
}

private func markedTextArgsDescriptor() -> (Descriptor, Registry) {
    let schema = Schema(id: SchemaId(1), kind: .structure(name: "MarkedTextArgs", fields: [
        Field(name: "text", schema: .concrete(primitiveId(.string)), required: true),
        Field(name: "animationBudgetMs", schema: .concrete(primitiveId(.u32)), required: true),
    ]))
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<MarkedTextArgs>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<MarkedTextArgs>.offset(of: \MarkedTextArgs.text)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<MarkedTextArgs>.offset(of: \MarkedTextArgs.animationBudgetMs)!, descriptor: u32Desc()),
        ], construct: .inPlace))
    )
    return (desc, Registry([schema]))
}

private func advanceTranscriptArgsDescriptor() -> (Descriptor, Registry) {
    let schema = Schema(id: SchemaId(1), kind: .structure(name: "AdvanceTranscriptArgs", fields: [
        Field(name: "text", schema: .concrete(primitiveId(.string)), required: true),
        Field(name: "committedLen", schema: .concrete(primitiveId(.u32)), required: true),
        Field(name: "animationBudgetMs", schema: .concrete(primitiveId(.u32)), required: true),
    ]))
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<AdvanceTranscriptArgs>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<AdvanceTranscriptArgs>.offset(of: \AdvanceTranscriptArgs.text)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<AdvanceTranscriptArgs>.offset(of: \AdvanceTranscriptArgs.committedLen)!, descriptor: u32Desc()),
            FieldAccess(offset: MemoryLayout<AdvanceTranscriptArgs>.offset(of: \AdvanceTranscriptArgs.animationBudgetMs)!, descriptor: u32Desc()),
        ], construct: .inPlace))
    )
    return (desc, Registry([schema]))
}

private func imeKeyEventArgsDescriptor() -> (Descriptor, Registry) {
    let schema = Schema(id: SchemaId(1), kind: .structure(name: "ImeKeyEventArgs", fields: [
        Field(name: "eventType", schema: .concrete(primitiveId(.string)), required: true),
        Field(name: "keyCode", schema: .concrete(primitiveId(.u32)), required: true),
        Field(name: "characters", schema: .concrete(primitiveId(.string)), required: true),
    ]))
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<ImeKeyEventArgs>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<ImeKeyEventArgs>.offset(of: \ImeKeyEventArgs.eventType)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<ImeKeyEventArgs>.offset(of: \ImeKeyEventArgs.keyCode)!, descriptor: u32Desc()),
            FieldAccess(offset: MemoryLayout<ImeKeyEventArgs>.offset(of: \ImeKeyEventArgs.characters)!, descriptor: stringDesc()),
        ], construct: .inPlace))
    )
    return (desc, Registry([schema]))
}

private func feedArgsDescriptor() -> (Descriptor, Registry) {
    let samplesId = SchemaId(2)
    let samplesSchema = Schema(id: samplesId, kind: .list(element: .concrete(primitiveId(.f32))))
    let root = Schema(id: SchemaId(1), kind: .structure(name: "FeedArgs", fields: [
        Field(name: "sessionId", schema: .concrete(primitiveId(.string)), required: true),
        Field(name: "samples", schema: .concrete(samplesId), required: true),
    ]))
    let samplesDesc = Descriptor(
        schema: .concrete(samplesId),
        layout: Layout(size: MemoryLayout<[Float]>.size, align: MemoryLayout<[Float]>.alignment),
        access: .bytes(BytesAccess(
            stride: MemoryLayout<Float>.stride,
            elemAlign: MemoryLayout<Float>.alignment,
            witness: floatArrayWitness()
        ))
    )
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)),
        layout: MemoryLayout<FeedArgsHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<FeedArgsHot>.offset(of: \FeedArgsHot.sessionId)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<FeedArgsHot>.offset(of: \FeedArgsHot.samples)!, descriptor: samplesDesc),
        ], construct: .inPlace))
    )
    return (desc, Registry([samplesSchema, root]))
}

private func confidenceHotDesc(_ id: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<ConfidenceHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<ConfidenceHot>.offset(of: \ConfidenceHot.meanLp)!, descriptor: f32Desc()),
            FieldAccess(offset: MemoryLayout<ConfidenceHot>.offset(of: \ConfidenceHot.minLp)!, descriptor: f32Desc()),
            FieldAccess(offset: MemoryLayout<ConfidenceHot>.offset(of: \ConfidenceHot.meanM)!, descriptor: f32Desc()),
            FieldAccess(offset: MemoryLayout<ConfidenceHot>.offset(of: \ConfidenceHot.minM)!, descriptor: f32Desc()),
        ], construct: .inPlace))
    )
}

private func alignedWordHotDesc(_ id: SchemaId, confidenceId: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<AlignedWordHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<AlignedWordHot>.offset(of: \AlignedWordHot.word)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<AlignedWordHot>.offset(of: \AlignedWordHot.start)!, descriptor: f64Desc()),
            FieldAccess(offset: MemoryLayout<AlignedWordHot>.offset(of: \AlignedWordHot.end)!, descriptor: f64Desc()),
            FieldAccess(offset: MemoryLayout<AlignedWordHot>.offset(of: \AlignedWordHot.confidence)!, descriptor: confidenceHotDesc(confidenceId)),
        ], construct: .inPlace))
    )
}

private func correctionEditHotDesc(_ id: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<CorrectionEditHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.editId)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.spanStart)!, descriptor: u32Desc()),
            FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.spanEnd)!, descriptor: u32Desc()),
            FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.original)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.replacement)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.term)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.aliasId)!, descriptor: i32Desc()),
            FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.rankerProb)!, descriptor: f64Desc()),
            FieldAccess(offset: MemoryLayout<CorrectionEditHot>.offset(of: \CorrectionEditHot.gateProb)!, descriptor: f64Desc()),
        ], construct: .inPlace))
    )
}

private func feedResultHotDesc(
    _ id: SchemaId,
    alignedListId: SchemaId,
    alignedWordId: SchemaId,
    confidenceId: SchemaId,
    correctionListId: SchemaId,
    correctionEditId: SchemaId
) -> Descriptor {
    let alignedListDesc = Descriptor(
        schema: .concrete(alignedListId),
        layout: Layout(size: MemoryLayout<[AlignedWordHot]>.size, align: MemoryLayout<[AlignedWordHot]>.alignment),
        access: .sequence(SequenceAccess(
            element: alignedWordHotDesc(alignedWordId, confidenceId: confidenceId),
            stride: MemoryLayout<AlignedWordHot>.stride,
            elemAlign: MemoryLayout<AlignedWordHot>.alignment,
            witness: arraySeqWitness(of: AlignedWordHot.self)
        ))
    )
    let correctionListDesc = Descriptor(
        schema: .concrete(correctionListId),
        layout: Layout(size: MemoryLayout<[CorrectionEditHot]>.size, align: MemoryLayout<[CorrectionEditHot]>.alignment),
        access: .sequence(SequenceAccess(
            element: correctionEditHotDesc(correctionEditId),
            stride: MemoryLayout<CorrectionEditHot>.stride,
            elemAlign: MemoryLayout<CorrectionEditHot>.alignment,
            witness: arraySeqWitness(of: CorrectionEditHot.self)
        ))
    )
    return Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<FeedResultHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<FeedResultHot>.offset(of: \FeedResultHot.text)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<FeedResultHot>.offset(of: \FeedResultHot.committedUtf16Len)!, descriptor: u32Desc()),
            FieldAccess(offset: MemoryLayout<FeedResultHot>.offset(of: \FeedResultHot.alignments)!, descriptor: alignedListDesc),
            FieldAccess(offset: MemoryLayout<FeedResultHot>.offset(of: \FeedResultHot.isFinal)!, descriptor: boolDesc()),
            FieldAccess(offset: MemoryLayout<FeedResultHot>.offset(of: \FeedResultHot.detectedLanguage)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<FeedResultHot>.offset(of: \FeedResultHot.correctionEdits)!, descriptor: correctionListDesc),
            FieldAccess(offset: MemoryLayout<FeedResultHot>.offset(of: \FeedResultHot.correctionSessionId)!, descriptor: stringDesc()),
        ], construct: .inPlace))
    )
}

private func feedResultOptionWitness() -> OptionWitness {
    OptionWitness(
        projectSome: { option, scratch in
            guard let value = option.assumingMemoryBound(to: FeedResultHot?.self).pointee else {
                return false
            }
            scratch.assumingMemoryBound(to: FeedResultHot.self).initialize(to: value)
            return true
        },
        initSome: { option, value in
            option.assumingMemoryBound(to: FeedResultHot?.self).initialize(
                to: .some(value.assumingMemoryBound(to: FeedResultHot.self).move())
            )
        },
        initNone: { option in
            option.assumingMemoryBound(to: FeedResultHot?.self).initialize(to: .none)
        }
    )
}

private func beeErrorHotDesc(_ id: SchemaId) -> Descriptor {
    let stringPayload = VariantAccess(
        wireIndex: 0,
        payloadFields: [FieldAccess(offset: 0, descriptor: stringDesc())],
        payloadLayout: Layout(size: MemoryLayout<String>.size, align: MemoryLayout<String>.alignment)
    )
    let variants = [
        VariantAccess(wireIndex: 0, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
        VariantAccess(wireIndex: 1, payloadFields: stringPayload.payloadFields, payloadLayout: stringPayload.payloadLayout),
        VariantAccess(wireIndex: 2, payloadFields: stringPayload.payloadFields, payloadLayout: stringPayload.payloadLayout),
        VariantAccess(wireIndex: 3, payloadFields: stringPayload.payloadFields, payloadLayout: stringPayload.payloadLayout),
        VariantAccess(wireIndex: 4, payloadFields: stringPayload.payloadFields, payloadLayout: stringPayload.payloadLayout),
        VariantAccess(wireIndex: 5, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
    ]
    return Descriptor(
        schema: .concrete(id),
        layout: Layout(size: MemoryLayout<BeeErrorHot>.size, align: MemoryLayout<BeeErrorHot>.alignment),
        access: .enumeration(EnumAccess(
            tag: { value in
                switch value.assumingMemoryBound(to: BeeErrorHot.self).pointee {
                case .engineNotLoaded: return 0
                case .sessionNotFound: return 1
                case .loadFailed: return 2
                case .transcriptionError: return 3
                case .correctionError: return 4
                case .notImplemented: return 5
                }
            },
            projectPayload: { value, localIndex, scratch in
                let message: String?
                switch value.assumingMemoryBound(to: BeeErrorHot.self).pointee {
                case .engineNotLoaded, .notImplemented:
                    message = nil
                case .sessionNotFound(let text), .loadFailed(let text),
                     .transcriptionError(let text), .correctionError(let text):
                    message = text
                }
                if let message {
                    scratch.assumingMemoryBound(to: String.self).initialize(to: message)
                } else {
                    _ = localIndex
                }
            },
            destroyPayload: { scratch, localIndex in
                guard (1...4).contains(localIndex) else {
                    return
                }
                scratch.assumingMemoryBound(to: String.self).deinitialize(count: 1)
            },
            inject: { slot, localIndex, scratch in
                let value: BeeErrorHot
                switch localIndex {
                case 0:
                    value = .engineNotLoaded
                case 1:
                    value = .sessionNotFound(scratch.assumingMemoryBound(to: String.self).move())
                case 2:
                    value = .loadFailed(scratch.assumingMemoryBound(to: String.self).move())
                case 3:
                    value = .transcriptionError(scratch.assumingMemoryBound(to: String.self).move())
                case 4:
                    value = .correctionError(scratch.assumingMemoryBound(to: String.self).move())
                case 5:
                    value = .notImplemented
                default:
                    fatalError("bad BeeError variant")
                }
                slot.assumingMemoryBound(to: BeeErrorHot.self).initialize(to: value)
            },
            variants: variants
        ))
    )
}

private func feedResponseDescriptor() -> (Descriptor, Registry) {
    let responseId = SchemaId(1)
    let optionId = SchemaId(2)
    let feedResultId = SchemaId(3)
    let alignedListId = SchemaId(4)
    let alignedWordId = SchemaId(5)
    let confidenceId = SchemaId(6)
    let correctionListId = SchemaId(7)
    let correctionEditId = SchemaId(8)
    let errorId = SchemaId(9)

    let schemas = [
        Schema(id: responseId, kind: .enumeration(name: "FeedResponse", variants: [
            Variant(name: "Ok", index: 0, payload: .newtype(.concrete(optionId))),
            Variant(name: "Err", index: 1, payload: .newtype(.concrete(errorId))),
        ])),
        Schema(id: optionId, kind: .option(element: .concrete(feedResultId))),
        Schema(id: feedResultId, kind: .structure(name: "FeedResult", fields: [
            Field(name: "text", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "committedUtf16Len", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "alignments", schema: .concrete(alignedListId), required: true),
            Field(name: "isFinal", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "detectedLanguage", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "correctionEdits", schema: .concrete(correctionListId), required: true),
            Field(name: "correctionSessionId", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: alignedListId, kind: .list(element: .concrete(alignedWordId))),
        Schema(id: alignedWordId, kind: .structure(name: "AlignedWord", fields: [
            Field(name: "word", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "start", schema: .concrete(primitiveId(.f64)), required: true),
            Field(name: "end", schema: .concrete(primitiveId(.f64)), required: true),
            Field(name: "confidence", schema: .concrete(confidenceId), required: true),
        ])),
        Schema(id: confidenceId, kind: .structure(name: "Confidence", fields: [
            Field(name: "meanLp", schema: .concrete(primitiveId(.f32)), required: true),
            Field(name: "minLp", schema: .concrete(primitiveId(.f32)), required: true),
            Field(name: "meanM", schema: .concrete(primitiveId(.f32)), required: true),
            Field(name: "minM", schema: .concrete(primitiveId(.f32)), required: true),
        ])),
        Schema(id: correctionListId, kind: .list(element: .concrete(correctionEditId))),
        Schema(id: correctionEditId, kind: .structure(name: "CorrectionEdit", fields: [
            Field(name: "editId", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "spanStart", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "spanEnd", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "original", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "replacement", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "term", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "aliasId", schema: .concrete(primitiveId(.i32)), required: true),
            Field(name: "rankerProb", schema: .concrete(primitiveId(.f64)), required: true),
            Field(name: "gateProb", schema: .concrete(primitiveId(.f64)), required: true),
        ])),
        Schema(id: errorId, kind: .enumeration(name: "BeeError", variants: [
            Variant(name: "EngineNotLoaded", index: 0, payload: .unit),
            Variant(name: "SessionNotFound", index: 1, payload: .newtype(.concrete(primitiveId(.string)))),
            Variant(name: "LoadFailed", index: 2, payload: .newtype(.concrete(primitiveId(.string)))),
            Variant(name: "TranscriptionError", index: 3, payload: .newtype(.concrete(primitiveId(.string)))),
            Variant(name: "CorrectionError", index: 4, payload: .newtype(.concrete(primitiveId(.string)))),
            Variant(name: "NotImplemented", index: 5, payload: .unit),
        ])),
    ]

    let optionDesc = Descriptor(
        schema: .concrete(optionId),
        layout: Layout(size: MemoryLayout<FeedResultHot?>.size, align: MemoryLayout<FeedResultHot?>.alignment),
        access: .option(OptionAccess(
            witness: feedResultOptionWitness(),
            some: feedResultHotDesc(
                feedResultId,
                alignedListId: alignedListId,
                alignedWordId: alignedWordId,
                confidenceId: confidenceId,
                correctionListId: correctionListId,
                correctionEditId: correctionEditId
            )
        ))
    )
    let errorDesc = beeErrorHotDesc(errorId)
    let responseDesc = Descriptor(
        schema: .concrete(responseId),
        layout: Layout(size: MemoryLayout<FeedResponseHot>.size, align: MemoryLayout<FeedResponseHot>.alignment),
        access: .enumeration(EnumAccess(
            tag: { value in
                switch value.assumingMemoryBound(to: FeedResponseHot.self).pointee {
                case .ok: return 0
                case .err: return 1
                }
            },
            projectPayload: { value, localIndex, scratch in
                switch (localIndex, value.assumingMemoryBound(to: FeedResponseHot.self).pointee) {
                case (0, .ok(let result)):
                    scratch.assumingMemoryBound(to: FeedResultHot?.self).initialize(to: result)
                case (1, .err(let error)):
                    scratch.assumingMemoryBound(to: BeeErrorHot.self).initialize(to: error)
                default:
                    break
                }
            },
            destroyPayload: { scratch, localIndex in
                switch localIndex {
                case 0:
                    scratch.assumingMemoryBound(to: FeedResultHot?.self).deinitialize(count: 1)
                case 1:
                    scratch.assumingMemoryBound(to: BeeErrorHot.self).deinitialize(count: 1)
                default:
                    break
                }
            },
            inject: { slot, localIndex, scratch in
                let value: FeedResponseHot
                switch localIndex {
                case 0:
                    value = .ok(scratch.assumingMemoryBound(to: FeedResultHot?.self).move())
                case 1:
                    value = .err(scratch.assumingMemoryBound(to: BeeErrorHot.self).move())
                default:
                    fatalError("bad feed response variant")
                }
                slot.assumingMemoryBound(to: FeedResponseHot.self).initialize(to: value)
            },
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [FieldAccess(offset: 0, descriptor: optionDesc)],
                    payloadLayout: Layout(size: MemoryLayout<FeedResultHot?>.size, align: MemoryLayout<FeedResultHot?>.alignment)
                ),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [FieldAccess(offset: 0, descriptor: errorDesc)],
                    payloadLayout: Layout(size: MemoryLayout<BeeErrorHot>.size, align: MemoryLayout<BeeErrorHot>.alignment)
                ),
            ]
        ))
    )

    return (responseDesc, Registry(schemas))
}

private func dodecaRoutesDescriptor() -> (Descriptor, Registry) {
    let routesId = SchemaId(1)
    let routeSetId = SchemaId(2)
    let schemas = [
        Schema(id: routesId, kind: .structure(name: "DodecaRoutes", fields: [
            Field(name: "routes", schema: .concrete(routeSetId), required: true),
        ])),
        Schema(id: routeSetId, kind: .set(element: .concrete(primitiveId(.string)))),
    ]
    let routeSetDesc = Descriptor(
        schema: .concrete(routeSetId),
        layout: MemoryLayout<Set<String>>.phonLayout,
        access: .sequence(SequenceAccess(
            element: stringDesc(),
            stride: MemoryLayout<String>.stride,
            elemAlign: MemoryLayout<String>.alignment,
            witness: .setOf(String.self)
        ))
    )
    let desc = Descriptor(
        schema: .concrete(routesId),
        layout: MemoryLayout<DodecaRoutesHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<DodecaRoutesHot>.offset(of: \DodecaRoutesHot.routes)!, descriptor: routeSetDesc),
        ], construct: .inPlace))
    )
    return (desc, Registry(schemas))
}

private func dodecaSchemas() -> [Schema] {
    [
        Schema(id: DodecaBenchId.optionString, kind: .option(element: .concrete(primitiveId(.string)))),
        Schema(id: DodecaBenchId.resolvedDependency, kind: .structure(name: "ResolvedDependency", fields: [
            Field(name: "name", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "version", schema: .concrete(DodecaBenchId.optionString), required: true),
        ])),
        Schema(id: DodecaBenchId.resolvedDependencyList, kind: .list(element: .concrete(DodecaBenchId.resolvedDependency))),
        Schema(id: DodecaBenchId.codeExecutionMetadata, kind: .structure(name: "CodeExecutionMetadata", fields: [
            Field(name: "language", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "dependencies", schema: .concrete(DodecaBenchId.resolvedDependencyList), required: true),
            Field(name: "duration_ms", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
        Schema(id: DodecaBenchId.injectionLocation, kind: .enumeration(name: "InjectionLocation", variants: [
            Variant(name: "Head", index: 0, payload: .unit),
            Variant(name: "Body", index: 1, payload: .unit),
        ])),
        Schema(id: DodecaBenchId.injection, kind: .structure(name: "Injection", fields: [
            Field(name: "location", schema: .concrete(DodecaBenchId.injectionLocation), required: true),
            Field(name: "content", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: DodecaBenchId.injectionList, kind: .list(element: .concrete(DodecaBenchId.injection))),
        Schema(id: DodecaBenchId.stringU32Tuple, kind: .tuple(elements: [
            .concrete(primitiveId(.string)),
            .concrete(primitiveId(.u32)),
        ])),
        Schema(id: DodecaBenchId.stringU32TupleList, kind: .list(element: .concrete(DodecaBenchId.stringU32Tuple))),
        Schema(id: DodecaBenchId.responsiveImageInfo, kind: .structure(name: "ResponsiveImageInfo", fields: [
            Field(name: "jxl_srcset", schema: .concrete(DodecaBenchId.stringU32TupleList), required: true),
            Field(name: "webp_srcset", schema: .concrete(DodecaBenchId.stringU32TupleList), required: true),
        ])),
        Schema(id: DodecaBenchId.routeSet, kind: .set(element: .concrete(primitiveId(.string)))),
        Schema(id: DodecaBenchId.mountLocalization, kind: .structure(name: "MountLocalization", fields: [
            Field(name: "segment", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "routes", schema: .concrete(DodecaBenchId.routeSet), required: true),
        ])),
        Schema(id: DodecaBenchId.mapStringString, kind: .map(key: .concrete(primitiveId(.string)), value: .concrete(primitiveId(.string)))),
        Schema(id: DodecaBenchId.optionMapStringString, kind: .option(element: .concrete(DodecaBenchId.mapStringString))),
        Schema(id: DodecaBenchId.optionStringSet, kind: .option(element: .concrete(DodecaBenchId.routeSet))),
        Schema(id: DodecaBenchId.mapStringCodeMetadata, kind: .map(key: .concrete(primitiveId(.string)), value: .concrete(DodecaBenchId.codeExecutionMetadata))),
        Schema(id: DodecaBenchId.optionMapStringCodeMetadata, kind: .option(element: .concrete(DodecaBenchId.mapStringCodeMetadata))),
        Schema(id: DodecaBenchId.mapStringResponsiveImageInfo, kind: .map(key: .concrete(primitiveId(.string)), value: .concrete(DodecaBenchId.responsiveImageInfo))),
        Schema(id: DodecaBenchId.optionMapStringResponsiveImageInfo, kind: .option(element: .concrete(DodecaBenchId.mapStringResponsiveImageInfo))),
        Schema(id: DodecaBenchId.stringList, kind: .list(element: .concrete(primitiveId(.string)))),
        Schema(id: DodecaBenchId.mapStringStringList, kind: .map(key: .concrete(primitiveId(.string)), value: .concrete(DodecaBenchId.stringList))),
        Schema(id: DodecaBenchId.optionMapStringStringList, kind: .option(element: .concrete(DodecaBenchId.mapStringStringList))),
        Schema(id: DodecaBenchId.optionMountLocalization, kind: .option(element: .concrete(DodecaBenchId.mountLocalization))),
        Schema(id: DodecaBenchId.htmlProcessInput, kind: .structure(name: "DodecaHtmlProcessInput", fields: [
            Field(name: "html", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "path_map", schema: .concrete(DodecaBenchId.optionMapStringString), required: true),
            Field(name: "known_routes", schema: .concrete(DodecaBenchId.optionStringSet), required: true),
            Field(name: "code_metadata", schema: .concrete(DodecaBenchId.optionMapStringCodeMetadata), required: true),
            Field(name: "injections", schema: .concrete(DodecaBenchId.injectionList), required: true),
            Field(name: "image_variants", schema: .concrete(DodecaBenchId.optionMapStringResponsiveImageInfo), required: true),
            Field(name: "vite_css_map", schema: .concrete(DodecaBenchId.optionMapStringStringList), required: true),
            Field(name: "mount", schema: .concrete(DodecaBenchId.optionMountLocalization), required: true),
        ])),
        Schema(id: DodecaBenchId.dynamic, kind: .dynamic),
        Schema(id: DodecaBenchId.dynamicList, kind: .list(element: .concrete(DodecaBenchId.dynamic))),
        Schema(id: DodecaBenchId.stringValueTuple, kind: .tuple(elements: [
            .concrete(primitiveId(.string)),
            .concrete(DodecaBenchId.dynamic),
        ])),
        Schema(id: DodecaBenchId.stringValueTupleList, kind: .list(element: .concrete(DodecaBenchId.stringValueTuple))),
        Schema(id: DodecaBenchId.templateCall, kind: .structure(name: "DodecaTemplateCall", fields: [
            Field(name: "context_id", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "name", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "args", schema: .concrete(DodecaBenchId.dynamicList), required: true),
            Field(name: "kwargs", schema: .concrete(DodecaBenchId.stringValueTupleList), required: true),
        ])),
        Schema(id: DodecaBenchId.loadDataResult, kind: .enumeration(name: "DodecaLoadDataResult", variants: [
            Variant(name: "Success", index: 0, payload: .structure([
                Field(name: "value", schema: .concrete(DodecaBenchId.dynamic), required: true),
            ])),
            Variant(name: "Error", index: 1, payload: .structure([
                Field(name: "message", schema: .concrete(primitiveId(.string)), required: true),
            ])),
        ])),
        Schema(id: DodecaBenchId.markdownHeading, kind: .structure(name: "DodecaMarkdownHeading", fields: [
            Field(name: "title", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "id", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "level", schema: .concrete(primitiveId(.u8)), required: true),
        ])),
        Schema(id: DodecaBenchId.markdownHeadingList, kind: .list(element: .concrete(DodecaBenchId.markdownHeading))),
        Schema(id: DodecaBenchId.reqDefinition, kind: .structure(name: "DodecaReqDefinition", fields: [
            Field(name: "id", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "anchor_id", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: DodecaBenchId.reqDefinitionList, kind: .list(element: .concrete(DodecaBenchId.reqDefinition))),
        Schema(id: DodecaBenchId.sourceKind, kind: .enumeration(name: "DodecaSourceKind", variants: [
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
        Schema(id: DodecaBenchId.sourceMapEntry, kind: .structure(name: "DodecaSourceMapEntry", fields: [
            Field(name: "id", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "kind", schema: .concrete(DodecaBenchId.sourceKind), required: true),
            Field(name: "line_start", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "line_end", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "byte_start", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "byte_end", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
        Schema(id: DodecaBenchId.sourceMapEntryList, kind: .list(element: .concrete(DodecaBenchId.sourceMapEntry))),
        Schema(id: DodecaBenchId.sourceMap, kind: .structure(name: "DodecaSourceMap", fields: [
            Field(name: "source_path", schema: .concrete(DodecaBenchId.optionString), required: true),
            Field(name: "entries", schema: .concrete(DodecaBenchId.sourceMapEntryList), required: true),
        ])),
        Schema(id: DodecaBenchId.frontmatter, kind: .structure(name: "DodecaFrontmatter", fields: [
            Field(name: "title", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "weight", schema: .concrete(primitiveId(.i32)), required: true),
            Field(name: "description", schema: .concrete(DodecaBenchId.optionString), required: true),
            Field(name: "template", schema: .concrete(DodecaBenchId.optionString), required: true),
            Field(name: "extra", schema: .concrete(DodecaBenchId.dynamic), required: true),
        ])),
        Schema(id: DodecaBenchId.parseResult, kind: .enumeration(name: "DodecaParseResult", variants: [
            Variant(name: "Success", index: 0, payload: .structure([
                Field(name: "frontmatter", schema: .concrete(DodecaBenchId.frontmatter), required: true),
                Field(name: "html", schema: .concrete(primitiveId(.string)), required: true),
                Field(name: "headings", schema: .concrete(DodecaBenchId.markdownHeadingList), required: true),
                Field(name: "reqs", schema: .concrete(DodecaBenchId.reqDefinitionList), required: true),
                Field(name: "head_injections", schema: .concrete(DodecaBenchId.stringList), required: true),
                Field(name: "source_map", schema: .concrete(DodecaBenchId.sourceMap), required: true),
            ])),
            Variant(name: "Error", index: 1, payload: .structure([
                Field(name: "message", schema: .concrete(primitiveId(.string)), required: true),
            ])),
        ])),
        Schema(id: DodecaBenchId.decodedImage, kind: .structure(name: "DecodedImage", fields: [
            Field(name: "pixels", schema: .concrete(primitiveId(.bytes)), required: true),
            Field(name: "width", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "height", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "channels", schema: .concrete(primitiveId(.u8)), required: true),
        ])),
        Schema(id: DodecaBenchId.imageResult, kind: .enumeration(name: "ImageResult", variants: [
            Variant(name: "Success", index: 0, payload: .structure([
                Field(name: "image", schema: .concrete(DodecaBenchId.decodedImage), required: true),
            ])),
            Variant(name: "ThumbhashSuccess", index: 1, payload: .structure([
                Field(name: "data_url", schema: .concrete(primitiveId(.string)), required: true),
            ])),
            Variant(name: "Error", index: 2, payload: .structure([
                Field(name: "message", schema: .concrete(primitiveId(.string)), required: true),
            ])),
        ])),
        Schema(id: DodecaBenchId.resizeInput, kind: .structure(name: "ResizeInput", fields: [
            Field(name: "pixels", schema: .concrete(primitiveId(.bytes)), required: true),
            Field(name: "width", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "height", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "channels", schema: .concrete(primitiveId(.u8)), required: true),
            Field(name: "target_width", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: DodecaBenchId.thumbhashInput, kind: .structure(name: "ThumbhashInput", fields: [
            Field(name: "pixels", schema: .concrete(primitiveId(.bytes)), required: true),
            Field(name: "width", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "height", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: DodecaBenchId.imageProcessorFixture, kind: .structure(name: "DodecaImageProcessorFixture", fields: [
            Field(name: "png_data", schema: .concrete(primitiveId(.bytes)), required: true),
            Field(name: "decoded_result", schema: .concrete(DodecaBenchId.imageResult), required: true),
            Field(name: "resize_input", schema: .concrete(DodecaBenchId.resizeInput), required: true),
            Field(name: "resize_result", schema: .concrete(DodecaBenchId.imageResult), required: true),
            Field(name: "thumbhash_input", schema: .concrete(DodecaBenchId.thumbhashInput), required: true),
            Field(name: "thumbhash_result", schema: .concrete(DodecaBenchId.imageResult), required: true),
            Field(name: "error_result", schema: .concrete(DodecaBenchId.imageResult), required: true),
        ])),
        Schema(id: DodecaBenchId.searchPage, kind: .structure(name: "SearchPage", fields: [
            Field(name: "url", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "source", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "html", schema: .concrete(primitiveId(.string)), required: true),
        ])),
        Schema(id: DodecaBenchId.searchPageList, kind: .list(element: .concrete(DodecaBenchId.searchPage))),
        Schema(id: DodecaBenchId.searchFile, kind: .structure(name: "SearchFile", fields: [
            Field(name: "path", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "contents", schema: .concrete(primitiveId(.bytes)), required: true),
        ])),
        Schema(id: DodecaBenchId.searchFileList, kind: .list(element: .concrete(DodecaBenchId.searchFile))),
        Schema(id: DodecaBenchId.searchIndexResult, kind: .enumeration(name: "SearchIndexResult", variants: [
            Variant(name: "Success", index: 0, payload: .structure([
                Field(name: "files", schema: .concrete(DodecaBenchId.searchFileList), required: true),
            ])),
            Variant(name: "Error", index: 1, payload: .structure([
                Field(name: "message", schema: .concrete(primitiveId(.string)), required: true),
            ])),
        ])),
        Schema(id: DodecaBenchId.searchIndexerFixture, kind: .structure(name: "DodecaSearchIndexerFixture", fields: [
            Field(name: "pages", schema: .concrete(DodecaBenchId.searchPageList), required: true),
            Field(name: "result", schema: .concrete(DodecaBenchId.searchIndexResult), required: true),
            Field(name: "error_result", schema: .concrete(DodecaBenchId.searchIndexResult), required: true),
        ])),
    ]
}

private func dodecaOptionDesc<Wrapped>(_ schema: SchemaId, _ type: Wrapped.Type, some: Descriptor) -> Descriptor {
    Descriptor(
        schema: .concrete(schema),
        layout: MemoryLayout<Wrapped?>.phonLayout,
        access: .option(OptionAccess(witness: .of(Wrapped.self), some: some))
    )
}

private func dodecaListDesc<Element>(_ schema: SchemaId, _ type: Element.Type, element: Descriptor) -> Descriptor {
    Descriptor(
        schema: .concrete(schema),
        layout: MemoryLayout<[Element]>.phonLayout,
        access: .sequence(SequenceAccess(
            element: element,
            stride: MemoryLayout<Element>.stride,
            elemAlign: MemoryLayout<Element>.alignment,
            witness: arraySeqWitness(of: Element.self)
        ))
    )
}

private func dodecaRecordDesc<T>(_ schema: SchemaId, _ type: T.Type, fields: [FieldAccess]) -> Descriptor {
    Descriptor(
        schema: .concrete(schema),
        layout: MemoryLayout<T>.phonLayout,
        access: .record(RecordAccess(fields: fields, construct: .inPlace))
    )
}

private func dodecaRouteSetDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(DodecaBenchId.routeSet),
        layout: MemoryLayout<Set<String>>.phonLayout,
        access: .sequence(SequenceAccess(
            element: stringDesc(),
            stride: MemoryLayout<String>.stride,
            elemAlign: MemoryLayout<String>.alignment,
            witness: .setOf(String.self)
        ))
    )
}

private func dodecaOptionStringDesc() -> Descriptor {
    dodecaOptionDesc(DodecaBenchId.optionString, String.self, some: stringDesc())
}

private func dodecaResolvedDependencyDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.resolvedDependency, DodecaResolvedDependencyHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaResolvedDependencyHot>.offset(of: \DodecaResolvedDependencyHot.name)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaResolvedDependencyHot>.offset(of: \DodecaResolvedDependencyHot.version)!, descriptor: dodecaOptionStringDesc()),
    ])
}

private func dodecaResolvedDependencyListDesc() -> Descriptor {
    dodecaListDesc(DodecaBenchId.resolvedDependencyList, DodecaResolvedDependencyHot.self, element: dodecaResolvedDependencyDesc())
}

private func dodecaCodeExecutionMetadataDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.codeExecutionMetadata, DodecaCodeExecutionMetadataHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaCodeExecutionMetadataHot>.offset(of: \DodecaCodeExecutionMetadataHot.language)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaCodeExecutionMetadataHot>.offset(of: \DodecaCodeExecutionMetadataHot.dependencies)!, descriptor: dodecaResolvedDependencyListDesc()),
        FieldAccess(offset: MemoryLayout<DodecaCodeExecutionMetadataHot>.offset(of: \DodecaCodeExecutionMetadataHot.durationMs)!, descriptor: u64Desc()),
    ])
}

private func dodecaInjectionLocationDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(DodecaBenchId.injectionLocation),
        layout: MemoryLayout<DodecaInjectionLocationHot>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: { ptr in
                switch ptr.assumingMemoryBound(to: DodecaInjectionLocationHot.self).pointee {
                case .head: return 0
                case .body: return 1
                }
            },
            projectPayload: { _, _, _ in },
            inject: { slot, localIndex, _ in
                let value: DodecaInjectionLocationHot = localIndex == 0 ? .head : .body
                slot.assumingMemoryBound(to: DodecaInjectionLocationHot.self).initialize(to: value)
            },
            variants: [
                VariantAccess(wireIndex: 0, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
                VariantAccess(wireIndex: 1, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
            ]
        ))
    )
}

private func dodecaInjectionDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.injection, DodecaInjectionHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaInjectionHot>.offset(of: \DodecaInjectionHot.location)!, descriptor: dodecaInjectionLocationDesc()),
        FieldAccess(offset: MemoryLayout<DodecaInjectionHot>.offset(of: \DodecaInjectionHot.content)!, descriptor: stringDesc()),
    ])
}

private func dodecaInjectionListDesc() -> Descriptor {
    dodecaListDesc(DodecaBenchId.injectionList, DodecaInjectionHot.self, element: dodecaInjectionDesc())
}

private func dodecaStringU32TupleDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.stringU32Tuple, DodecaStringU32Hot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaStringU32Hot>.offset(of: \DodecaStringU32Hot.string)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaStringU32Hot>.offset(of: \DodecaStringU32Hot.value)!, descriptor: u32Desc()),
    ])
}

private func dodecaStringU32TupleListDesc() -> Descriptor {
    dodecaListDesc(DodecaBenchId.stringU32TupleList, DodecaStringU32Hot.self, element: dodecaStringU32TupleDesc())
}

private func dodecaResponsiveImageInfoDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.responsiveImageInfo, DodecaResponsiveImageInfoHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaResponsiveImageInfoHot>.offset(of: \DodecaResponsiveImageInfoHot.jxlSrcset)!, descriptor: dodecaStringU32TupleListDesc()),
        FieldAccess(offset: MemoryLayout<DodecaResponsiveImageInfoHot>.offset(of: \DodecaResponsiveImageInfoHot.webpSrcset)!, descriptor: dodecaStringU32TupleListDesc()),
    ])
}

private func dodecaMountLocalizationDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.mountLocalization, DodecaMountLocalizationHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaMountLocalizationHot>.offset(of: \DodecaMountLocalizationHot.segment)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaMountLocalizationHot>.offset(of: \DodecaMountLocalizationHot.routes)!, descriptor: dodecaRouteSetDesc()),
    ])
}

private func dodecaStringListDesc() -> Descriptor {
    dodecaListDesc(DodecaBenchId.stringList, String.self, element: stringDesc())
}

private func dodecaStringMapDesc<T>(_ schema: SchemaId, _ type: T.Type, value: Descriptor) -> Descriptor {
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

private func dodecaHtmlProcessInputDescriptor() -> (Descriptor, Registry) {
    let mapStringStringDesc = dodecaStringMapDesc(DodecaBenchId.mapStringString, String.self, value: stringDesc())
    let mapStringCodeMetadataDesc = dodecaStringMapDesc(
        DodecaBenchId.mapStringCodeMetadata,
        DodecaCodeExecutionMetadataHot.self,
        value: dodecaCodeExecutionMetadataDesc()
    )
    let mapStringResponsiveImageInfoDesc = dodecaStringMapDesc(
        DodecaBenchId.mapStringResponsiveImageInfo,
        DodecaResponsiveImageInfoHot.self,
        value: dodecaResponsiveImageInfoDesc()
    )
    let mapStringStringListDesc = dodecaStringMapDesc(
        DodecaBenchId.mapStringStringList,
        [String].self,
        value: dodecaStringListDesc()
    )
    let root = dodecaRecordDesc(DodecaBenchId.htmlProcessInput, DodecaHtmlProcessInputHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaHtmlProcessInputHot>.offset(of: \DodecaHtmlProcessInputHot.html)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaHtmlProcessInputHot>.offset(of: \DodecaHtmlProcessInputHot.pathMap)!, descriptor: dodecaOptionDesc(DodecaBenchId.optionMapStringString, [String: String].self, some: mapStringStringDesc)),
        FieldAccess(offset: MemoryLayout<DodecaHtmlProcessInputHot>.offset(of: \DodecaHtmlProcessInputHot.knownRoutes)!, descriptor: dodecaOptionDesc(DodecaBenchId.optionStringSet, Set<String>.self, some: dodecaRouteSetDesc())),
        FieldAccess(offset: MemoryLayout<DodecaHtmlProcessInputHot>.offset(of: \DodecaHtmlProcessInputHot.codeMetadata)!, descriptor: dodecaOptionDesc(DodecaBenchId.optionMapStringCodeMetadata, [String: DodecaCodeExecutionMetadataHot].self, some: mapStringCodeMetadataDesc)),
        FieldAccess(offset: MemoryLayout<DodecaHtmlProcessInputHot>.offset(of: \DodecaHtmlProcessInputHot.injections)!, descriptor: dodecaInjectionListDesc()),
        FieldAccess(offset: MemoryLayout<DodecaHtmlProcessInputHot>.offset(of: \DodecaHtmlProcessInputHot.imageVariants)!, descriptor: dodecaOptionDesc(DodecaBenchId.optionMapStringResponsiveImageInfo, [String: DodecaResponsiveImageInfoHot].self, some: mapStringResponsiveImageInfoDesc)),
        FieldAccess(offset: MemoryLayout<DodecaHtmlProcessInputHot>.offset(of: \DodecaHtmlProcessInputHot.viteCssMap)!, descriptor: dodecaOptionDesc(DodecaBenchId.optionMapStringStringList, [String: [String]].self, some: mapStringStringListDesc)),
        FieldAccess(offset: MemoryLayout<DodecaHtmlProcessInputHot>.offset(of: \DodecaHtmlProcessInputHot.mount)!, descriptor: dodecaOptionDesc(DodecaBenchId.optionMountLocalization, DodecaMountLocalizationHot.self, some: dodecaMountLocalizationDesc())),
    ])
    return (root, Registry(dodecaSchemas()))
}

private func dodecaDynamicDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(DodecaBenchId.dynamic),
        layout: MemoryLayout<Value>.phonLayout,
        access: .dynamic
    )
}

private func dodecaTemplateCallDescriptor() -> (Descriptor, Registry) {
    let dynamicList = dodecaListDesc(DodecaBenchId.dynamicList, Value.self, element: dodecaDynamicDesc())
    let stringValueTuple = dodecaRecordDesc(DodecaBenchId.stringValueTuple, DodecaStringValueHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaStringValueHot>.offset(of: \DodecaStringValueHot.key)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaStringValueHot>.offset(of: \DodecaStringValueHot.value)!, descriptor: dodecaDynamicDesc()),
    ])
    let kwargs = dodecaListDesc(DodecaBenchId.stringValueTupleList, DodecaStringValueHot.self, element: stringValueTuple)
    let root = dodecaRecordDesc(DodecaBenchId.templateCall, DodecaTemplateCallHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaTemplateCallHot>.offset(of: \DodecaTemplateCallHot.contextId)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaTemplateCallHot>.offset(of: \DodecaTemplateCallHot.name)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaTemplateCallHot>.offset(of: \DodecaTemplateCallHot.args)!, descriptor: dynamicList),
        FieldAccess(offset: MemoryLayout<DodecaTemplateCallHot>.offset(of: \DodecaTemplateCallHot.kwargs)!, descriptor: kwargs),
    ])
    return (root, Registry(dodecaSchemas()))
}

private func dodecaLoadDataResultDescriptor() -> (Descriptor, Registry) {
    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: DodecaLoadDataResultHot.self).pointee {
        case .success: return 0
        case .error: return 1
        }
    }
    let projectPayload: (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void = { value, _, scratch in
        switch value.assumingMemoryBound(to: DodecaLoadDataResultHot.self).pointee {
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
        let value: DodecaLoadDataResultHot
        switch localIndex {
        case 0:
            value = .success(value: scratch.assumingMemoryBound(to: Value.self).move())
        case 1:
            value = .error(message: scratch.assumingMemoryBound(to: String.self).move())
        default:
            fatalError("bad DodecaLoadDataResult variant index")
        }
        slot.assumingMemoryBound(to: DodecaLoadDataResultHot.self).initialize(to: value)
    }

    let root = Descriptor(
        schema: .concrete(DodecaBenchId.loadDataResult),
        layout: MemoryLayout<DodecaLoadDataResultHot>.phonLayout,
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
    return (root, Registry(dodecaSchemas()))
}

private func dodecaMarkdownHeadingDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.markdownHeading, DodecaMarkdownHeadingHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaMarkdownHeadingHot>.offset(of: \DodecaMarkdownHeadingHot.title)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaMarkdownHeadingHot>.offset(of: \DodecaMarkdownHeadingHot.id)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaMarkdownHeadingHot>.offset(of: \DodecaMarkdownHeadingHot.level)!, descriptor: u8Desc()),
    ])
}

private func dodecaReqDefinitionDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.reqDefinition, DodecaReqDefinitionHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaReqDefinitionHot>.offset(of: \DodecaReqDefinitionHot.id)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaReqDefinitionHot>.offset(of: \DodecaReqDefinitionHot.anchorId)!, descriptor: stringDesc()),
    ])
}

private func dodecaSourceKindDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(DodecaBenchId.sourceKind),
        layout: MemoryLayout<DodecaSourceKindHot>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: { ptr in
                switch ptr.assumingMemoryBound(to: DodecaSourceKindHot.self).pointee {
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
            projectPayload: { _, _, _ in },
            inject: { slot, localIndex, _ in
                let value: DodecaSourceKindHot
                switch localIndex {
                case 0: value = .heading
                case 1: value = .paragraph
                case 2: value = .blockQuote
                case 3: value = .list
                case 4: value = .listItem
                case 5: value = .definitionList
                case 6: value = .definitionListTitle
                case 7: value = .definitionListDefinition
                case 8: value = .thematicBreak
                case 9: value = .table
                case 10: value = .tableHead
                case 11: value = .tableRow
                case 12: value = .tableCell
                case 13: value = .image
                default: fatalError("bad DodecaSourceKind variant index")
                }
                slot.assumingMemoryBound(to: DodecaSourceKindHot.self).initialize(to: value)
            },
            variants: (0..<14).map { index in
                VariantAccess(wireIndex: UInt32(index), payloadFields: [], payloadLayout: Layout(size: 0, align: 1))
            }
        ))
    )
}

private func dodecaSourceMapEntryDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.sourceMapEntry, DodecaSourceMapEntryHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaSourceMapEntryHot>.offset(of: \DodecaSourceMapEntryHot.id)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaSourceMapEntryHot>.offset(of: \DodecaSourceMapEntryHot.kind)!, descriptor: dodecaSourceKindDesc()),
        FieldAccess(offset: MemoryLayout<DodecaSourceMapEntryHot>.offset(of: \DodecaSourceMapEntryHot.lineStart)!, descriptor: u32Desc()),
        FieldAccess(offset: MemoryLayout<DodecaSourceMapEntryHot>.offset(of: \DodecaSourceMapEntryHot.lineEnd)!, descriptor: u32Desc()),
        FieldAccess(offset: MemoryLayout<DodecaSourceMapEntryHot>.offset(of: \DodecaSourceMapEntryHot.byteStart)!, descriptor: u64Desc()),
        FieldAccess(offset: MemoryLayout<DodecaSourceMapEntryHot>.offset(of: \DodecaSourceMapEntryHot.byteEnd)!, descriptor: u64Desc()),
    ])
}

private func dodecaSourceMapDesc() -> Descriptor {
    let entries = dodecaListDesc(DodecaBenchId.sourceMapEntryList, DodecaSourceMapEntryHot.self, element: dodecaSourceMapEntryDesc())
    return dodecaRecordDesc(DodecaBenchId.sourceMap, DodecaSourceMapHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaSourceMapHot>.offset(of: \DodecaSourceMapHot.sourcePath)!, descriptor: dodecaOptionStringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaSourceMapHot>.offset(of: \DodecaSourceMapHot.entries)!, descriptor: entries),
    ])
}

private func dodecaFrontmatterDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.frontmatter, DodecaFrontmatterHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaFrontmatterHot>.offset(of: \DodecaFrontmatterHot.title)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaFrontmatterHot>.offset(of: \DodecaFrontmatterHot.weight)!, descriptor: i32Desc()),
        FieldAccess(offset: MemoryLayout<DodecaFrontmatterHot>.offset(of: \DodecaFrontmatterHot.description)!, descriptor: dodecaOptionStringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaFrontmatterHot>.offset(of: \DodecaFrontmatterHot.template)!, descriptor: dodecaOptionStringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaFrontmatterHot>.offset(of: \DodecaFrontmatterHot.extra)!, descriptor: dodecaDynamicDesc()),
    ])
}

private func dodecaParseResultDescriptor() -> (Descriptor, Registry) {
    let headings = dodecaListDesc(DodecaBenchId.markdownHeadingList, DodecaMarkdownHeadingHot.self, element: dodecaMarkdownHeadingDesc())
    let reqs = dodecaListDesc(DodecaBenchId.reqDefinitionList, DodecaReqDefinitionHot.self, element: dodecaReqDefinitionDesc())
    let successFrontmatterOffset = MemoryLayout<DodecaParseSuccessPayloadHot>.offset(of: \DodecaParseSuccessPayloadHot.frontmatter)!
    let successHtmlOffset = MemoryLayout<DodecaParseSuccessPayloadHot>.offset(of: \DodecaParseSuccessPayloadHot.html)!
    let successHeadingsOffset = MemoryLayout<DodecaParseSuccessPayloadHot>.offset(of: \DodecaParseSuccessPayloadHot.headings)!
    let successReqsOffset = MemoryLayout<DodecaParseSuccessPayloadHot>.offset(of: \DodecaParseSuccessPayloadHot.reqs)!
    let successHeadInjectionsOffset = MemoryLayout<DodecaParseSuccessPayloadHot>.offset(of: \DodecaParseSuccessPayloadHot.headInjections)!
    let successSourceMapOffset = MemoryLayout<DodecaParseSuccessPayloadHot>.offset(of: \DodecaParseSuccessPayloadHot.sourceMap)!
    let errorMessageOffset = MemoryLayout<DodecaParseErrorPayloadHot>.offset(of: \DodecaParseErrorPayloadHot.message)!

    let root = Descriptor(
        schema: .concrete(DodecaBenchId.parseResult),
        layout: MemoryLayout<DodecaParseResultHot>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: { ptr in
                switch ptr.assumingMemoryBound(to: DodecaParseResultHot.self).pointee {
                case .success: return 0
                case .error: return 1
                }
            },
            projectPayload: { value, _, scratch in
                switch value.assumingMemoryBound(to: DodecaParseResultHot.self).pointee {
                case .success(let payload):
                    scratch.assumingMemoryBound(to: DodecaParseSuccessPayloadHot.self).initialize(to: payload)
                case .error(let payload):
                    scratch.assumingMemoryBound(to: DodecaParseErrorPayloadHot.self).initialize(to: payload)
                }
            },
            destroyPayload: { scratch, localIndex in
                switch localIndex {
                case 0:
                    scratch.assumingMemoryBound(to: DodecaParseSuccessPayloadHot.self).deinitialize(count: 1)
                case 1:
                    scratch.assumingMemoryBound(to: DodecaParseErrorPayloadHot.self).deinitialize(count: 1)
                default:
                    break
                }
            },
            inject: { slot, localIndex, scratch in
                let value: DodecaParseResultHot
                switch localIndex {
                case 0:
                    value = .success(scratch.assumingMemoryBound(to: DodecaParseSuccessPayloadHot.self).move())
                case 1:
                    value = .error(scratch.assumingMemoryBound(to: DodecaParseErrorPayloadHot.self).move())
                default:
                    fatalError("bad DodecaParseResult variant index")
                }
                slot.assumingMemoryBound(to: DodecaParseResultHot.self).initialize(to: value)
            },
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [
                        FieldAccess(offset: successFrontmatterOffset, descriptor: dodecaFrontmatterDesc()),
                        FieldAccess(offset: successHtmlOffset, descriptor: stringDesc()),
                        FieldAccess(offset: successHeadingsOffset, descriptor: headings),
                        FieldAccess(offset: successReqsOffset, descriptor: reqs),
                        FieldAccess(offset: successHeadInjectionsOffset, descriptor: dodecaStringListDesc()),
                        FieldAccess(offset: successSourceMapOffset, descriptor: dodecaSourceMapDesc()),
                    ],
                    payloadLayout: MemoryLayout<DodecaParseSuccessPayloadHot>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [FieldAccess(offset: errorMessageOffset, descriptor: stringDesc())],
                    payloadLayout: MemoryLayout<DodecaParseErrorPayloadHot>.phonLayout
                ),
            ]
        ))
    )
    return (root, Registry(dodecaSchemas()))
}

private func dodecaDecodedImageDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.decodedImage, DodecaDecodedImageHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaDecodedImageHot>.offset(of: \DodecaDecodedImageHot.pixels)!, descriptor: bytesDesc()),
        FieldAccess(offset: MemoryLayout<DodecaDecodedImageHot>.offset(of: \DodecaDecodedImageHot.width)!, descriptor: u32Desc()),
        FieldAccess(offset: MemoryLayout<DodecaDecodedImageHot>.offset(of: \DodecaDecodedImageHot.height)!, descriptor: u32Desc()),
        FieldAccess(offset: MemoryLayout<DodecaDecodedImageHot>.offset(of: \DodecaDecodedImageHot.channels)!, descriptor: u8Desc()),
    ])
}

private func dodecaResizeInputDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.resizeInput, DodecaResizeInputHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaResizeInputHot>.offset(of: \DodecaResizeInputHot.pixels)!, descriptor: bytesDesc()),
        FieldAccess(offset: MemoryLayout<DodecaResizeInputHot>.offset(of: \DodecaResizeInputHot.width)!, descriptor: u32Desc()),
        FieldAccess(offset: MemoryLayout<DodecaResizeInputHot>.offset(of: \DodecaResizeInputHot.height)!, descriptor: u32Desc()),
        FieldAccess(offset: MemoryLayout<DodecaResizeInputHot>.offset(of: \DodecaResizeInputHot.channels)!, descriptor: u8Desc()),
        FieldAccess(offset: MemoryLayout<DodecaResizeInputHot>.offset(of: \DodecaResizeInputHot.targetWidth)!, descriptor: u32Desc()),
    ])
}

private func dodecaThumbhashInputDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.thumbhashInput, DodecaThumbhashInputHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaThumbhashInputHot>.offset(of: \DodecaThumbhashInputHot.pixels)!, descriptor: bytesDesc()),
        FieldAccess(offset: MemoryLayout<DodecaThumbhashInputHot>.offset(of: \DodecaThumbhashInputHot.width)!, descriptor: u32Desc()),
        FieldAccess(offset: MemoryLayout<DodecaThumbhashInputHot>.offset(of: \DodecaThumbhashInputHot.height)!, descriptor: u32Desc()),
    ])
}

private func dodecaImageResultDesc() -> Descriptor {
    let successImageOffset = MemoryLayout<DodecaImageSuccessPayloadHot>.offset(of: \DodecaImageSuccessPayloadHot.image)!
    let thumbhashDataUrlOffset = MemoryLayout<DodecaThumbhashSuccessPayloadHot>.offset(of: \DodecaThumbhashSuccessPayloadHot.dataUrl)!
    let errorMessageOffset = MemoryLayout<DodecaImageErrorPayloadHot>.offset(of: \DodecaImageErrorPayloadHot.message)!

    return Descriptor(
        schema: .concrete(DodecaBenchId.imageResult),
        layout: MemoryLayout<DodecaImageResultHot>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: { ptr in
                switch ptr.assumingMemoryBound(to: DodecaImageResultHot.self).pointee {
                case .success: return 0
                case .thumbhashSuccess: return 1
                case .error: return 2
                }
            },
            projectPayload: { value, _, scratch in
                switch value.assumingMemoryBound(to: DodecaImageResultHot.self).pointee {
                case .success(let payload):
                    scratch.assumingMemoryBound(to: DodecaImageSuccessPayloadHot.self).initialize(to: payload)
                case .thumbhashSuccess(let payload):
                    scratch.assumingMemoryBound(to: DodecaThumbhashSuccessPayloadHot.self).initialize(to: payload)
                case .error(let payload):
                    scratch.assumingMemoryBound(to: DodecaImageErrorPayloadHot.self).initialize(to: payload)
                }
            },
            destroyPayload: { scratch, localIndex in
                switch localIndex {
                case 0:
                    scratch.assumingMemoryBound(to: DodecaImageSuccessPayloadHot.self).deinitialize(count: 1)
                case 1:
                    scratch.assumingMemoryBound(to: DodecaThumbhashSuccessPayloadHot.self).deinitialize(count: 1)
                case 2:
                    scratch.assumingMemoryBound(to: DodecaImageErrorPayloadHot.self).deinitialize(count: 1)
                default:
                    break
                }
            },
            inject: { slot, localIndex, scratch in
                let value: DodecaImageResultHot
                switch localIndex {
                case 0:
                    value = .success(scratch.assumingMemoryBound(to: DodecaImageSuccessPayloadHot.self).move())
                case 1:
                    value = .thumbhashSuccess(scratch.assumingMemoryBound(to: DodecaThumbhashSuccessPayloadHot.self).move())
                case 2:
                    value = .error(scratch.assumingMemoryBound(to: DodecaImageErrorPayloadHot.self).move())
                default:
                    fatalError("bad DodecaImageResult variant index")
                }
                slot.assumingMemoryBound(to: DodecaImageResultHot.self).initialize(to: value)
            },
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [FieldAccess(offset: successImageOffset, descriptor: dodecaDecodedImageDesc())],
                    payloadLayout: MemoryLayout<DodecaImageSuccessPayloadHot>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [FieldAccess(offset: thumbhashDataUrlOffset, descriptor: stringDesc())],
                    payloadLayout: MemoryLayout<DodecaThumbhashSuccessPayloadHot>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 2,
                    payloadFields: [FieldAccess(offset: errorMessageOffset, descriptor: stringDesc())],
                    payloadLayout: MemoryLayout<DodecaImageErrorPayloadHot>.phonLayout
                ),
            ]
        ))
    )
}

private func dodecaImageProcessorFixtureDescriptor() -> (Descriptor, Registry) {
    let root = dodecaRecordDesc(DodecaBenchId.imageProcessorFixture, DodecaImageProcessorFixtureHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaImageProcessorFixtureHot>.offset(of: \DodecaImageProcessorFixtureHot.pngData)!, descriptor: bytesDesc()),
        FieldAccess(offset: MemoryLayout<DodecaImageProcessorFixtureHot>.offset(of: \DodecaImageProcessorFixtureHot.decodedResult)!, descriptor: dodecaImageResultDesc()),
        FieldAccess(offset: MemoryLayout<DodecaImageProcessorFixtureHot>.offset(of: \DodecaImageProcessorFixtureHot.resizeInput)!, descriptor: dodecaResizeInputDesc()),
        FieldAccess(offset: MemoryLayout<DodecaImageProcessorFixtureHot>.offset(of: \DodecaImageProcessorFixtureHot.resizeResult)!, descriptor: dodecaImageResultDesc()),
        FieldAccess(offset: MemoryLayout<DodecaImageProcessorFixtureHot>.offset(of: \DodecaImageProcessorFixtureHot.thumbhashInput)!, descriptor: dodecaThumbhashInputDesc()),
        FieldAccess(offset: MemoryLayout<DodecaImageProcessorFixtureHot>.offset(of: \DodecaImageProcessorFixtureHot.thumbhashResult)!, descriptor: dodecaImageResultDesc()),
        FieldAccess(offset: MemoryLayout<DodecaImageProcessorFixtureHot>.offset(of: \DodecaImageProcessorFixtureHot.errorResult)!, descriptor: dodecaImageResultDesc()),
    ])
    return (root, Registry(dodecaSchemas()))
}

private func dodecaSearchPageDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.searchPage, DodecaSearchPageHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaSearchPageHot>.offset(of: \DodecaSearchPageHot.url)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaSearchPageHot>.offset(of: \DodecaSearchPageHot.source)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaSearchPageHot>.offset(of: \DodecaSearchPageHot.html)!, descriptor: stringDesc()),
    ])
}

private func dodecaSearchFileDesc() -> Descriptor {
    dodecaRecordDesc(DodecaBenchId.searchFile, DodecaSearchFileHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaSearchFileHot>.offset(of: \DodecaSearchFileHot.path)!, descriptor: stringDesc()),
        FieldAccess(offset: MemoryLayout<DodecaSearchFileHot>.offset(of: \DodecaSearchFileHot.contents)!, descriptor: bytesDesc()),
    ])
}

private func dodecaSearchIndexResultDesc() -> Descriptor {
    let successFilesOffset = MemoryLayout<DodecaSearchSuccessPayloadHot>.offset(of: \DodecaSearchSuccessPayloadHot.files)!
    let errorMessageOffset = MemoryLayout<DodecaSearchErrorPayloadHot>.offset(of: \DodecaSearchErrorPayloadHot.message)!
    let fileList = dodecaListDesc(DodecaBenchId.searchFileList, DodecaSearchFileHot.self, element: dodecaSearchFileDesc())

    return Descriptor(
        schema: .concrete(DodecaBenchId.searchIndexResult),
        layout: MemoryLayout<DodecaSearchIndexResultHot>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: { ptr in
                switch ptr.assumingMemoryBound(to: DodecaSearchIndexResultHot.self).pointee {
                case .success: return 0
                case .error: return 1
                }
            },
            projectPayload: { value, _, scratch in
                switch value.assumingMemoryBound(to: DodecaSearchIndexResultHot.self).pointee {
                case .success(let payload):
                    scratch.assumingMemoryBound(to: DodecaSearchSuccessPayloadHot.self).initialize(to: payload)
                case .error(let payload):
                    scratch.assumingMemoryBound(to: DodecaSearchErrorPayloadHot.self).initialize(to: payload)
                }
            },
            destroyPayload: { scratch, localIndex in
                switch localIndex {
                case 0:
                    scratch.assumingMemoryBound(to: DodecaSearchSuccessPayloadHot.self).deinitialize(count: 1)
                case 1:
                    scratch.assumingMemoryBound(to: DodecaSearchErrorPayloadHot.self).deinitialize(count: 1)
                default:
                    break
                }
            },
            inject: { slot, localIndex, scratch in
                let value: DodecaSearchIndexResultHot
                switch localIndex {
                case 0:
                    value = .success(scratch.assumingMemoryBound(to: DodecaSearchSuccessPayloadHot.self).move())
                case 1:
                    value = .error(scratch.assumingMemoryBound(to: DodecaSearchErrorPayloadHot.self).move())
                default:
                    fatalError("bad DodecaSearchIndexResult variant index")
                }
                slot.assumingMemoryBound(to: DodecaSearchIndexResultHot.self).initialize(to: value)
            },
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [FieldAccess(offset: successFilesOffset, descriptor: fileList)],
                    payloadLayout: MemoryLayout<DodecaSearchSuccessPayloadHot>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [FieldAccess(offset: errorMessageOffset, descriptor: stringDesc())],
                    payloadLayout: MemoryLayout<DodecaSearchErrorPayloadHot>.phonLayout
                ),
            ]
        ))
    )
}

private func dodecaSearchIndexerFixtureDescriptor() -> (Descriptor, Registry) {
    let pages = dodecaListDesc(DodecaBenchId.searchPageList, DodecaSearchPageHot.self, element: dodecaSearchPageDesc())
    let root = dodecaRecordDesc(DodecaBenchId.searchIndexerFixture, DodecaSearchIndexerFixtureHot.self, fields: [
        FieldAccess(offset: MemoryLayout<DodecaSearchIndexerFixtureHot>.offset(of: \DodecaSearchIndexerFixtureHot.pages)!, descriptor: pages),
        FieldAccess(offset: MemoryLayout<DodecaSearchIndexerFixtureHot>.offset(of: \DodecaSearchIndexerFixtureHot.result)!, descriptor: dodecaSearchIndexResultDesc()),
        FieldAccess(offset: MemoryLayout<DodecaSearchIndexerFixtureHot>.offset(of: \DodecaSearchIndexerFixtureHot.errorResult)!, descriptor: dodecaSearchIndexResultDesc()),
    ])
    return (root, Registry(dodecaSchemas()))
}

private func dibsOptionU64Desc(_ id: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<UInt64?>.phonLayout,
        access: .option(OptionAccess(witness: .of(UInt64.self), some: u64Desc()))
    )
}

private func dibsSqlValueDesc(_ id: SchemaId) -> Descriptor {
    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: DibsSqlValueHot.self).pointee {
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
        switch value.assumingMemoryBound(to: DibsSqlValueHot.self).pointee {
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
        let value: DibsSqlValueHot
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
            fatalError("bad DibsSqlValueHot variant index")
        }
        slot.assumingMemoryBound(to: DibsSqlValueHot.self).initialize(to: value)
    }

    return Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<DibsSqlValueHot>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: projectPayload,
            destroyPayload: destroyPayload,
            inject: inject,
            variants: [
                VariantAccess(wireIndex: 0, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
                VariantAccess(wireIndex: 1, payloadFields: [FieldAccess(offset: 0, descriptor: boolDesc())], payloadLayout: MemoryLayout<Bool>.phonLayout),
                VariantAccess(wireIndex: 2, payloadFields: [FieldAccess(offset: 0, descriptor: i16Desc())], payloadLayout: MemoryLayout<Int16>.phonLayout),
                VariantAccess(wireIndex: 3, payloadFields: [FieldAccess(offset: 0, descriptor: i32Desc())], payloadLayout: MemoryLayout<Int32>.phonLayout),
                VariantAccess(wireIndex: 4, payloadFields: [FieldAccess(offset: 0, descriptor: i64Desc())], payloadLayout: MemoryLayout<Int64>.phonLayout),
                VariantAccess(wireIndex: 5, payloadFields: [FieldAccess(offset: 0, descriptor: f32Desc())], payloadLayout: MemoryLayout<Float>.phonLayout),
                VariantAccess(wireIndex: 6, payloadFields: [FieldAccess(offset: 0, descriptor: f64Desc())], payloadLayout: MemoryLayout<Double>.phonLayout),
                VariantAccess(wireIndex: 7, payloadFields: [FieldAccess(offset: 0, descriptor: stringDesc())], payloadLayout: MemoryLayout<String>.phonLayout),
                VariantAccess(wireIndex: 8, payloadFields: [FieldAccess(offset: 0, descriptor: bytesDesc())], payloadLayout: MemoryLayout<[UInt8]>.phonLayout),
            ]
        ))
    )
}

private func dibsRowFieldDesc(_ id: SchemaId, sqlValueId: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<DibsRowFieldHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<DibsRowFieldHot>.offset(of: \DibsRowFieldHot.name)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<DibsRowFieldHot>.offset(of: \DibsRowFieldHot.value)!, descriptor: dibsSqlValueDesc(sqlValueId)),
        ], construct: .inPlace))
    )
}

private func dibsRowFieldListDesc(_ id: SchemaId, rowFieldId: SchemaId, sqlValueId: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<[DibsRowFieldHot]>.phonLayout,
        access: .sequence(SequenceAccess(
            element: dibsRowFieldDesc(rowFieldId, sqlValueId: sqlValueId),
            stride: MemoryLayout<DibsRowFieldHot>.stride,
            elemAlign: MemoryLayout<DibsRowFieldHot>.alignment,
            witness: arraySeqWitness(of: DibsRowFieldHot.self)
        ))
    )
}

private func dibsRowFieldListListDesc(
    _ id: SchemaId,
    rowFieldListId: SchemaId,
    rowFieldId: SchemaId,
    sqlValueId: SchemaId
) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<[[DibsRowFieldHot]]>.phonLayout,
        access: .sequence(SequenceAccess(
            element: dibsRowFieldListDesc(rowFieldListId, rowFieldId: rowFieldId, sqlValueId: sqlValueId),
            stride: MemoryLayout<[DibsRowFieldHot]>.stride,
            elemAlign: MemoryLayout<[DibsRowFieldHot]>.alignment,
            witness: arraySeqWitness(of: [DibsRowFieldHot].self)
        ))
    )
}

private func dibsListResponseDescriptor() -> (Descriptor, Registry) {
    let responseId = SchemaId(1)
    let rowListListId = SchemaId(2)
    let rowListId = SchemaId(3)
    let rowFieldId = SchemaId(4)
    let sqlValueId = SchemaId(5)
    let optionU64Id = SchemaId(6)

    let schemas = [
        Schema(id: responseId, kind: .structure(name: "DibsListResponse", fields: [
            Field(name: "rows", schema: .concrete(rowListListId), required: true),
            Field(name: "total", schema: .concrete(optionU64Id), required: true),
        ])),
        Schema(id: rowListListId, kind: .list(element: .concrete(rowListId))),
        Schema(id: rowListId, kind: .list(element: .concrete(rowFieldId))),
        Schema(id: rowFieldId, kind: .structure(name: "RowField", fields: [
            Field(name: "name", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "value", schema: .concrete(sqlValueId), required: true),
        ])),
        Schema(id: sqlValueId, kind: .enumeration(name: "SqlValue", variants: [
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
        Schema(id: optionU64Id, kind: .option(element: .concrete(primitiveId(.u64)))),
    ]
    let desc = Descriptor(
        schema: .concrete(responseId),
        layout: MemoryLayout<DibsListResponseHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(
                offset: MemoryLayout<DibsListResponseHot>.offset(of: \DibsListResponseHot.rows)!,
                descriptor: dibsRowFieldListListDesc(
                    rowListListId,
                    rowFieldListId: rowListId,
                    rowFieldId: rowFieldId,
                    sqlValueId: sqlValueId
                )
            ),
            FieldAccess(offset: MemoryLayout<DibsListResponseHot>.offset(of: \DibsListResponseHot.total)!, descriptor: dibsOptionU64Desc(optionU64Id)),
        ], construct: .inPlace))
    )
    return (desc, Registry(schemas))
}

private func optionU32Desc(_ id: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<UInt32?>.phonLayout,
        access: .option(OptionAccess(witness: .of(UInt32.self), some: u32Desc()))
    )
}

private func stringListDesc(_ id: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<[String]>.phonLayout,
        access: .sequence(SequenceAccess(
            element: stringDesc(),
            stride: MemoryLayout<String>.stride,
            elemAlign: MemoryLayout<String>.alignment,
            witness: arraySeqWitness(of: String.self)
        ))
    )
}

private func offCpuBreakdownDesc(_ id: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<OffCpuBreakdownHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<OffCpuBreakdownHot>.offset(of: \OffCpuBreakdownHot.sleepNs)!, descriptor: u64Desc()),
            FieldAccess(offset: MemoryLayout<OffCpuBreakdownHot>.offset(of: \OffCpuBreakdownHot.ioNs)!, descriptor: u64Desc()),
            FieldAccess(offset: MemoryLayout<OffCpuBreakdownHot>.offset(of: \OffCpuBreakdownHot.mutexNs)!, descriptor: u64Desc()),
        ], construct: .inPlace))
    )
}

private func staxFlamegraphDescriptor() -> (root: Descriptor, registry: Registry, blocks: [SchemaId: Descriptor]) {
    let updateId = SchemaId(1)
    let optionU32Id = SchemaId(2)
    let offCpuId = SchemaId(3)
    let nodeId = SchemaId(4)
    let nodeListId = SchemaId(5)
    let stringListId = SchemaId(6)

    let schemas = [
        Schema(id: updateId, kind: .structure(name: "StaxFlamegraphUpdate", fields: [
            Field(name: "total_on_cpu_ns", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "strings", schema: .concrete(stringListId), required: true),
            Field(name: "root", schema: .concrete(nodeId), required: true),
        ])),
        Schema(id: optionU32Id, kind: .option(element: .concrete(primitiveId(.u32)))),
        Schema(id: offCpuId, kind: .structure(name: "OffCpuBreakdown", fields: [
            Field(name: "sleep_ns", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "io_ns", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "mutex_ns", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
        Schema(id: nodeId, kind: .structure(name: "FlameNode", fields: [
            Field(name: "address", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "function_name", schema: .concrete(optionU32Id), required: true),
            Field(name: "binary", schema: .concrete(optionU32Id), required: true),
            Field(name: "on_cpu_ns", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "off_cpu", schema: .concrete(offCpuId), required: true),
            Field(name: "children", schema: .concrete(nodeListId), required: true),
        ])),
        Schema(id: nodeListId, kind: .list(element: .concrete(nodeId))),
        Schema(id: stringListId, kind: .list(element: .concrete(primitiveId(.string)))),
    ]
    let recurseNode = Descriptor(
        schema: .concrete(nodeId),
        layout: MemoryLayout<FlameNodeHot>.phonLayout,
        access: .recurse
    )
    let recurseNodeList = Descriptor(
        schema: .concrete(nodeListId),
        layout: MemoryLayout<[FlameNodeHot]>.phonLayout,
        access: .recurse
    )
    let nodeBody = Descriptor(
        schema: .concrete(nodeId),
        layout: MemoryLayout<FlameNodeHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<FlameNodeHot>.offset(of: \FlameNodeHot.address)!, descriptor: u64Desc()),
            FieldAccess(offset: MemoryLayout<FlameNodeHot>.offset(of: \FlameNodeHot.functionName)!, descriptor: optionU32Desc(optionU32Id)),
            FieldAccess(offset: MemoryLayout<FlameNodeHot>.offset(of: \FlameNodeHot.binary)!, descriptor: optionU32Desc(optionU32Id)),
            FieldAccess(offset: MemoryLayout<FlameNodeHot>.offset(of: \FlameNodeHot.onCpuNs)!, descriptor: u64Desc()),
            FieldAccess(offset: MemoryLayout<FlameNodeHot>.offset(of: \FlameNodeHot.offCpu)!, descriptor: offCpuBreakdownDesc(offCpuId)),
            FieldAccess(offset: MemoryLayout<FlameNodeHot>.offset(of: \FlameNodeHot.children)!, descriptor: recurseNodeList),
        ], construct: .inPlace))
    )
    let nodeListBody = Descriptor(
        schema: .concrete(nodeListId),
        layout: MemoryLayout<[FlameNodeHot]>.phonLayout,
        access: .sequence(SequenceAccess(
            element: recurseNode,
            stride: MemoryLayout<FlameNodeHot>.stride,
            elemAlign: MemoryLayout<FlameNodeHot>.alignment,
            witness: arraySeqWitness(of: FlameNodeHot.self)
        ))
    )
    let root = Descriptor(
        schema: .concrete(updateId),
        layout: MemoryLayout<StaxFlamegraphUpdateHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<StaxFlamegraphUpdateHot>.offset(of: \StaxFlamegraphUpdateHot.totalOnCpuNs)!, descriptor: u64Desc()),
            FieldAccess(offset: MemoryLayout<StaxFlamegraphUpdateHot>.offset(of: \StaxFlamegraphUpdateHot.strings)!, descriptor: stringListDesc(stringListId)),
            FieldAccess(offset: MemoryLayout<StaxFlamegraphUpdateHot>.offset(of: \StaxFlamegraphUpdateHot.root)!, descriptor: recurseNode),
        ], construct: .inPlace))
    )

    return (root, Registry(schemas), [
        nodeId: nodeBody,
        nodeListId: nodeListBody,
    ])
}

private func staxLinuxBrokerControlDescriptor() -> (Descriptor, Registry) {
    let rootId = SchemaId(1)
    let configId = SchemaId(2)
    let statusId = SchemaId(3)
    let errorId = SchemaId(4)
    let errorListId = SchemaId(5)
    let offsetsId = SchemaId(6)
    let optionOffsetsId = SchemaId(7)

    let schemas = [
        Schema(id: rootId, kind: .structure(name: "StaxLinuxBrokerControlFixture", fields: [
            Field(name: "config", schema: .concrete(configId), required: true),
            Field(name: "status", schema: .concrete(statusId), required: true),
            Field(name: "errors", schema: .concrete(errorListId), required: true),
            Field(name: "waking_field_offsets", schema: .concrete(optionOffsetsId), required: true),
        ])),
        Schema(id: configId, kind: .structure(name: "StaxLinuxPerfSessionConfig", fields: [
            Field(name: "target_pid", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "frequency_hz", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "kernel_stacks", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "request_waking", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "request_pmu", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "request_dwarf_unwind", schema: .concrete(primitiveId(.bool)), required: true),
        ])),
        Schema(id: statusId, kind: .structure(name: "StaxLinuxDaemonStatus", fields: [
            Field(name: "version", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "host_arch", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "privileged", schema: .concrete(primitiveId(.bool)), required: true),
            Field(name: "perf_event_paranoid", schema: .concrete(primitiveId(.i32)), required: true),
        ])),
        Schema(id: errorId, kind: .enumeration(name: "StaxLinuxPerfSessionError", variants: [
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
        Schema(id: errorListId, kind: .list(element: .concrete(errorId))),
        Schema(id: offsetsId, kind: .structure(name: "StaxLinuxWakingFieldOffsets", fields: [
            Field(name: "wakee_pid_offset", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "wakee_pid_size", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: optionOffsetsId, kind: .option(element: .concrete(offsetsId))),
    ]

    func configDesc() -> Descriptor {
        Descriptor(
            schema: .concrete(configId),
            layout: MemoryLayout<StaxLinuxPerfSessionConfigHot>.phonLayout,
            access: .record(RecordAccess(fields: [
                FieldAccess(offset: MemoryLayout<StaxLinuxPerfSessionConfigHot>.offset(of: \StaxLinuxPerfSessionConfigHot.targetPid)!, descriptor: u32Desc()),
                FieldAccess(offset: MemoryLayout<StaxLinuxPerfSessionConfigHot>.offset(of: \StaxLinuxPerfSessionConfigHot.frequencyHz)!, descriptor: u32Desc()),
                FieldAccess(offset: MemoryLayout<StaxLinuxPerfSessionConfigHot>.offset(of: \StaxLinuxPerfSessionConfigHot.kernelStacks)!, descriptor: boolDesc()),
                FieldAccess(offset: MemoryLayout<StaxLinuxPerfSessionConfigHot>.offset(of: \StaxLinuxPerfSessionConfigHot.requestWaking)!, descriptor: boolDesc()),
                FieldAccess(offset: MemoryLayout<StaxLinuxPerfSessionConfigHot>.offset(of: \StaxLinuxPerfSessionConfigHot.requestPmu)!, descriptor: boolDesc()),
                FieldAccess(offset: MemoryLayout<StaxLinuxPerfSessionConfigHot>.offset(of: \StaxLinuxPerfSessionConfigHot.requestDwarfUnwind)!, descriptor: boolDesc()),
            ], construct: .inPlace))
        )
    }

    func statusDesc() -> Descriptor {
        Descriptor(
            schema: .concrete(statusId),
            layout: MemoryLayout<StaxLinuxDaemonStatusHot>.phonLayout,
            access: .record(RecordAccess(fields: [
                FieldAccess(offset: MemoryLayout<StaxLinuxDaemonStatusHot>.offset(of: \StaxLinuxDaemonStatusHot.version)!, descriptor: stringDesc()),
                FieldAccess(offset: MemoryLayout<StaxLinuxDaemonStatusHot>.offset(of: \StaxLinuxDaemonStatusHot.hostArch)!, descriptor: stringDesc()),
                FieldAccess(offset: MemoryLayout<StaxLinuxDaemonStatusHot>.offset(of: \StaxLinuxDaemonStatusHot.privileged)!, descriptor: boolDesc()),
                FieldAccess(offset: MemoryLayout<StaxLinuxDaemonStatusHot>.offset(of: \StaxLinuxDaemonStatusHot.perfEventParanoid)!, descriptor: i32Desc()),
            ], construct: .inPlace))
        )
    }

    func offsetsDesc() -> Descriptor {
        Descriptor(
            schema: .concrete(offsetsId),
            layout: MemoryLayout<StaxLinuxWakingFieldOffsetsHot>.phonLayout,
            access: .record(RecordAccess(fields: [
                FieldAccess(offset: MemoryLayout<StaxLinuxWakingFieldOffsetsHot>.offset(of: \StaxLinuxWakingFieldOffsetsHot.wakeePidOffset)!, descriptor: u32Desc()),
                FieldAccess(offset: MemoryLayout<StaxLinuxWakingFieldOffsetsHot>.offset(of: \StaxLinuxWakingFieldOffsetsHot.wakeePidSize)!, descriptor: u32Desc()),
            ], construct: .inPlace))
        )
    }

    func optionOffsetsDesc() -> Descriptor {
        Descriptor(
            schema: .concrete(optionOffsetsId),
            layout: MemoryLayout<StaxLinuxWakingFieldOffsetsHot?>.phonLayout,
            access: .option(OptionAccess(witness: .of(StaxLinuxWakingFieldOffsetsHot.self), some: offsetsDesc()))
        )
    }

    func errorDesc() -> Descriptor {
        let notPrivilegedDetailOffset = MemoryLayout<StaxNotPrivilegedPayloadHot>.offset(of: \StaxNotPrivilegedPayloadHot.detail)!
        let perfCpuOffset = MemoryLayout<StaxPerfEventOpenPayloadHot>.offset(of: \StaxPerfEventOpenPayloadHot.cpu)!
        let perfErrnoOffset = MemoryLayout<StaxPerfEventOpenPayloadHot>.offset(of: \StaxPerfEventOpenPayloadHot.errno)!
        let perfDetailOffset = MemoryLayout<StaxPerfEventOpenPayloadHot>.offset(of: \StaxPerfEventOpenPayloadHot.detail)!
        let callerUidOffset = MemoryLayout<StaxNotAuthorizedPayloadHot>.offset(of: \StaxNotAuthorizedPayloadHot.callerUid)!
        let targetUidOffset = MemoryLayout<StaxNotAuthorizedPayloadHot>.offset(of: \StaxNotAuthorizedPayloadHot.targetUid)!

        return Descriptor(
            schema: .concrete(errorId),
            layout: MemoryLayout<StaxLinuxPerfSessionErrorHot>.phonLayout,
            access: .enumeration(EnumAccess(
                tag: { ptr in
                    switch ptr.assumingMemoryBound(to: StaxLinuxPerfSessionErrorHot.self).pointee {
                    case .notPrivileged: return 0
                    case .perfEventOpen: return 1
                    case .noSuchTarget: return 2
                    case .notAuthorized: return 3
                    }
                },
                projectPayload: { value, _, scratch in
                    switch value.assumingMemoryBound(to: StaxLinuxPerfSessionErrorHot.self).pointee {
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
                },
                destroyPayload: { scratch, localIndex in
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
                },
                inject: { slot, localIndex, scratch in
                    let error: StaxLinuxPerfSessionErrorHot
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
                        fatalError("bad StaxLinuxPerfSessionError variant")
                    }
                    slot.assumingMemoryBound(to: StaxLinuxPerfSessionErrorHot.self).initialize(to: error)
                },
                variants: [
                    VariantAccess(
                        wireIndex: 0,
                        payloadFields: [FieldAccess(offset: notPrivilegedDetailOffset, descriptor: stringDesc())],
                        payloadLayout: MemoryLayout<StaxNotPrivilegedPayloadHot>.phonLayout
                    ),
                    VariantAccess(
                        wireIndex: 1,
                        payloadFields: [
                            FieldAccess(offset: perfCpuOffset, descriptor: u32Desc()),
                            FieldAccess(offset: perfErrnoOffset, descriptor: i32Desc()),
                            FieldAccess(offset: perfDetailOffset, descriptor: stringDesc()),
                        ],
                        payloadLayout: MemoryLayout<StaxPerfEventOpenPayloadHot>.phonLayout
                    ),
                    VariantAccess(
                        wireIndex: 2,
                        payloadFields: [FieldAccess(offset: 0, descriptor: u32Desc())],
                        payloadLayout: MemoryLayout<UInt32>.phonLayout
                    ),
                    VariantAccess(
                        wireIndex: 3,
                        payloadFields: [
                            FieldAccess(offset: callerUidOffset, descriptor: u32Desc()),
                            FieldAccess(offset: targetUidOffset, descriptor: u32Desc()),
                        ],
                        payloadLayout: MemoryLayout<StaxNotAuthorizedPayloadHot>.phonLayout
                    ),
                ]
            ))
        )
    }

    func errorListDesc() -> Descriptor {
        Descriptor(
            schema: .concrete(errorListId),
            layout: MemoryLayout<[StaxLinuxPerfSessionErrorHot]>.phonLayout,
            access: .sequence(SequenceAccess(
                element: errorDesc(),
                stride: MemoryLayout<StaxLinuxPerfSessionErrorHot>.stride,
                elemAlign: MemoryLayout<StaxLinuxPerfSessionErrorHot>.alignment,
                witness: arraySeqWitness(of: StaxLinuxPerfSessionErrorHot.self)
            ))
        )
    }

    let root = Descriptor(
        schema: .concrete(rootId),
        layout: MemoryLayout<StaxLinuxBrokerControlFixtureHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<StaxLinuxBrokerControlFixtureHot>.offset(of: \StaxLinuxBrokerControlFixtureHot.config)!, descriptor: configDesc()),
            FieldAccess(offset: MemoryLayout<StaxLinuxBrokerControlFixtureHot>.offset(of: \StaxLinuxBrokerControlFixtureHot.status)!, descriptor: statusDesc()),
            FieldAccess(offset: MemoryLayout<StaxLinuxBrokerControlFixtureHot>.offset(of: \StaxLinuxBrokerControlFixtureHot.errors)!, descriptor: errorListDesc()),
            FieldAccess(offset: MemoryLayout<StaxLinuxBrokerControlFixtureHot>.offset(of: \StaxLinuxBrokerControlFixtureHot.wakingFieldOffsets)!, descriptor: optionOffsetsDesc()),
        ], construct: .inPlace))
    )

    return (root, Registry(schemas))
}

private func dodecaByteChannelItemDescriptor() -> (Descriptor, Registry) {
    (bytesDesc(), Registry([]))
}

private func dodecaStringChannelItemDescriptor() -> (Descriptor, Registry) {
    (stringDesc(), Registry([]))
}

private func dibsMigrationLogDescriptor() -> (Descriptor, Registry) {
    let rootId = SchemaId(1)
    let schemas = [
        Schema(id: rootId, kind: .structure(name: "DibsMigrationLog", fields: [
            Field(name: "version", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "level", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "message", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "applied_count", schema: .concrete(primitiveId(.u32)), required: true),
            Field(name: "elapsed_ms", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
    ]
    let root = Descriptor(
        schema: .concrete(rootId),
        layout: MemoryLayout<DibsMigrationLogHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<DibsMigrationLogHot>.offset(of: \DibsMigrationLogHot.version)!, descriptor: u64Desc()),
            FieldAccess(offset: MemoryLayout<DibsMigrationLogHot>.offset(of: \DibsMigrationLogHot.level)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<DibsMigrationLogHot>.offset(of: \DibsMigrationLogHot.message)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<DibsMigrationLogHot>.offset(of: \DibsMigrationLogHot.appliedCount)!, descriptor: u32Desc()),
            FieldAccess(offset: MemoryLayout<DibsMigrationLogHot>.offset(of: \DibsMigrationLogHot.elapsedMs)!, descriptor: u64Desc()),
        ], construct: .inPlace))
    )
    return (root, Registry(schemas))
}

private func helixPulseAvailableDescriptor() -> (Descriptor, Registry) {
    let rootId = SchemaId(1)
    let schemas = [
        Schema(id: rootId, kind: .structure(name: "HelixPulseAvailable", fields: [
            Field(name: "pulse_id", schema: .concrete(primitiveId(.u64)), required: true),
        ])),
    ]
    let root = Descriptor(
        schema: .concrete(rootId),
        layout: MemoryLayout<HelixPulseAvailableHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<HelixPulseAvailableHot>.offset(of: \HelixPulseAvailableHot.pulseId)!, descriptor: u64Desc()),
        ], construct: .inPlace))
    )
    return (root, Registry(schemas))
}

private func traceyRuleIdDesc(_ id: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<TraceyRuleIdHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<TraceyRuleIdHot>.offset(of: \TraceyRuleIdHot.base)!, descriptor: stringDesc()),
            FieldAccess(offset: MemoryLayout<TraceyRuleIdHot>.offset(of: \TraceyRuleIdHot.version)!, descriptor: u32Desc()),
        ], construct: .inPlace))
    )
}

private func traceyRuleIdListDesc(_ id: SchemaId, ruleId: SchemaId) -> Descriptor {
    Descriptor(
        schema: .concrete(id),
        layout: MemoryLayout<[TraceyRuleIdHot]>.phonLayout,
        access: .sequence(SequenceAccess(
            element: traceyRuleIdDesc(ruleId),
            stride: MemoryLayout<TraceyRuleIdHot>.stride,
            elemAlign: MemoryLayout<TraceyRuleIdHot>.alignment,
            witness: arraySeqWitness(of: TraceyRuleIdHot.self)
        ))
    )
}

private func traceyDataUpdateDescriptor() -> (Descriptor, Registry) {
    let rootId = SchemaId(1)
    let ruleId = SchemaId(2)
    let ruleListId = SchemaId(3)
    let schemas = [
        Schema(id: rootId, kind: .structure(name: "TraceyDataUpdate", fields: [
            Field(name: "version", schema: .concrete(primitiveId(.u64)), required: true),
            Field(name: "newly_covered", schema: .concrete(ruleListId), required: true),
            Field(name: "newly_uncovered", schema: .concrete(ruleListId), required: true),
        ])),
        Schema(id: ruleId, kind: .structure(name: "TraceyRuleId", fields: [
            Field(name: "base", schema: .concrete(primitiveId(.string)), required: true),
            Field(name: "version", schema: .concrete(primitiveId(.u32)), required: true),
        ])),
        Schema(id: ruleListId, kind: .list(element: .concrete(ruleId))),
    ]
    let root = Descriptor(
        schema: .concrete(rootId),
        layout: MemoryLayout<TraceyDataUpdateHot>.phonLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<TraceyDataUpdateHot>.offset(of: \TraceyDataUpdateHot.version)!, descriptor: u64Desc()),
            FieldAccess(offset: MemoryLayout<TraceyDataUpdateHot>.offset(of: \TraceyDataUpdateHot.newlyCovered)!, descriptor: traceyRuleIdListDesc(ruleListId, ruleId: ruleId)),
            FieldAccess(offset: MemoryLayout<TraceyDataUpdateHot>.offset(of: \TraceyDataUpdateHot.newlyUncovered)!, descriptor: traceyRuleIdListDesc(ruleListId, ruleId: ruleId)),
        ], construct: .inPlace))
    )
    return (root, Registry(schemas))
}

private func makeFeedArgs() -> FeedArgsHot {
    FeedArgsHot(
        sessionId: "bee-session-hot-path",
        samples: (0..<4096).map { Float(Int($0 % 97) - 48) / 97.0 }
    )
}

private func makeFeedResponse() -> FeedResponseHot {
    let alignments = (0..<24).map { i in
        AlignedWordHot(
            word: "word-\(i)",
            start: Double(i) * 0.08,
            end: Double(i) * 0.08 + 0.07,
            confidence: ConfidenceHot(meanLp: -0.25, minLp: -0.9, meanM: 0.85, minM: 0.42)
        )
    }
    var edits: [CorrectionEditHot] = []
    edits.reserveCapacity(6)
    for i in 0..<6 {
        edits.append(CorrectionEditHot(
            editId: "edit-\(i)",
            spanStart: UInt32(i * 3),
            spanEnd: UInt32(i * 3 + 2),
            original: "raw phrase \(i)",
            replacement: "corrected phrase \(i)",
            term: "term-\(i)",
            aliasId: Int32(i),
            rankerProb: 0.72 + Double(i) * 0.01,
            gateProb: 0.81 + Double(i) * 0.01
        ))
    }
    return .ok(FeedResultHot(
        text: "bee hot path transcript with a handful of aligned words",
        committedUtf16Len: 54,
        alignments: alignments,
        isFinal: false,
        detectedLanguage: "en",
        correctionEdits: edits,
        correctionSessionId: "corr-session-0001"
    ))
}

private func makeDibsListResponse() -> DibsListResponseHot {
    var rows: [[DibsRowFieldHot]] = []
    rows.reserveCapacity(32)
    for i in 0..<32 {
        let row: [DibsRowFieldHot] = [
            DibsRowFieldHot(name: "id", value: .i64(Int64(1_000 + i))),
            DibsRowFieldHot(name: "enabled", value: .bool(i % 2 == 0)),
            DibsRowFieldHot(name: "score", value: .f64(Double(i) * 0.125)),
            DibsRowFieldHot(name: "label", value: .string("product-\(i)")),
            DibsRowFieldHot(name: "payload", value: .bytes([1, 2, 3, 5, 8, UInt8(i & 0xff)])),
        ]
        rows.append(row)
    }
    return DibsListResponseHot(rows: rows, total: UInt64(rows.count))
}

private func makeDodecaRoutes() -> DodecaRoutesHot {
    var routes = Set<String>()
    routes.reserveCapacity(96)
    routes.insert("/")
    routes.insert("/guide/")
    for i in 0..<94 {
        routes.insert("/guide/topic-\(i)/")
    }
    return DodecaRoutesHot(routes: routes)
}

private func makeDodecaDynamicObject() -> Value {
    .object([
        .init(key: "route", value: .string("/guide/")),
        .init(key: "fresh", value: .bool(true)),
        .init(key: "count", value: .number(.canonical(unsigned: 12))),
        .init(key: "items", value: .array([.string("docs"), .string("search"), .null])),
    ])
}

private func makeDodecaTemplateCall() -> DodecaTemplateCallHot {
    DodecaTemplateCallHot(
        contextId: "ctx-1",
        name: "get_section",
        args: [
            makeDodecaDynamicObject(),
            .string("docs"),
            .array((0..<8).map { .string("arg-\($0)") }),
        ],
        kwargs: [
            DodecaStringValueHot(key: "path", value: .string("/guide/")),
            DodecaStringValueHot(key: "meta", value: makeDodecaDynamicObject()),
            DodecaStringValueHot(key: "limit", value: .number(.canonical(unsigned: 64))),
        ]
    )
}

private func makeDodecaLoadDataResult() -> DodecaLoadDataResultHot {
    .success(
        value: .object([
            .init(key: "title", value: .string("Phon")),
            .init(key: "sidebar", value: .bool(true)),
            .init(key: "count", value: .number(.canonical(unsigned: 42))),
        ])
    )
}

private func makeDodecaFrontmatterExtra() -> Value {
    .object([
        .init(key: "sidebar", value: .bool(true)),
        .init(key: "icon", value: .string("book")),
        .init(key: "custom_value", value: .number(.canonical(unsigned: 42))),
    ])
}

private func makeDodecaSourceKind(_ i: Int) -> DodecaSourceKindHot {
    switch i % 14 {
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
    default: return .image
    }
}

private func makeDodecaParseResult() -> DodecaParseResultHot {
    let entries = (0..<64).map { i in
        DodecaSourceMapEntryHot(
            id: "node-\(i)",
            kind: makeDodecaSourceKind(i),
            lineStart: UInt32(5 + i),
            lineEnd: UInt32(5 + i),
            byteStart: UInt64(i * 24),
            byteEnd: UInt64(i * 24 + 18)
        )
    }

    return .success(DodecaParseSuccessPayloadHot(
        frontmatter: DodecaFrontmatterHot(
            title: "Phon migration",
            weight: 10,
            description: "Generated fixture for Dodeca markdown",
            template: "page.html",
            extra: makeDodecaFrontmatterExtra()
        ),
        html: String(repeating: "<h1 data-sid=\"h1\">Intro</h1><p data-sid=\"p1\">Generated fixture</p>", count: 12),
        headings: (0..<16).map { i in
            DodecaMarkdownHeadingHot(
                title: "Heading \(i)",
                id: "heading-\(i)",
                level: UInt8(i % 4 + 1)
            )
        },
        reqs: (0..<8).map { i in
            DodecaReqDefinitionHot(
                id: "vox.dodeca.markdown.\(i)",
                anchorId: "r-vox-dodeca-markdown-\(i)"
            )
        },
        headInjections: [
            "<link rel=\"stylesheet\" href=\"/assets/arborium.css\">",
            "<script type=\"module\" src=\"/assets/search.js\"></script>",
        ],
        sourceMap: DodecaSourceMapHot(
            sourcePath: "content/guide.md",
            entries: entries
        )
    ))
}

private func makeDodecaDecodedImage(seed: UInt8, width: UInt32, height: UInt32) -> DodecaDecodedImageHot {
    let pixelCount = Int(width) * Int(height) * 4
    return DodecaDecodedImageHot(
        pixels: (0..<pixelCount).map { UInt8(($0 + Int(seed)) & 0xff) },
        width: width,
        height: height,
        channels: 4
    )
}

private func makeDodecaImageProcessorFixture() -> DodecaImageProcessorFixtureHot {
    let decoded = makeDodecaDecodedImage(seed: 0x20, width: 96, height: 64)
    let resized = makeDodecaDecodedImage(seed: 0x80, width: 48, height: 32)

    return DodecaImageProcessorFixtureHot(
        pngData: (0..<16_384).map { UInt8($0 & 0xff) },
        decodedResult: .success(DodecaImageSuccessPayloadHot(image: decoded)),
        resizeInput: DodecaResizeInputHot(
            pixels: decoded.pixels,
            width: decoded.width,
            height: decoded.height,
            channels: decoded.channels,
            targetWidth: resized.width
        ),
        resizeResult: .success(DodecaImageSuccessPayloadHot(image: resized)),
        thumbhashInput: DodecaThumbhashInputHot(
            pixels: decoded.pixels,
            width: decoded.width,
            height: decoded.height
        ),
        thumbhashResult: .thumbhashSuccess(DodecaThumbhashSuccessPayloadHot(
            dataUrl: "data:image/thumbhash;base64,BwgJCgsMDQ4PEA=="
        )),
        errorResult: .error(DodecaImageErrorPayloadHot(
            message: "unsupported color profile in source image"
        ))
    )
}

private func makeDodecaSearchIndexerFixture() -> DodecaSearchIndexerFixtureHot {
    let pages = (0..<32).map { i in
        DodecaSearchPageHot(
            url: "/guide/topic-\(i)/",
            source: "content/guide/topic-\(i).md",
            html: "<article><h1>Topic \(i)</h1><p>Search body \(i)</p></article>"
        )
    }
    let files = (0..<8).map { i in
        DodecaSearchFileHot(
            path: "public/search/chunk-\(i).json",
            contents: (0..<1_024).map { UInt8(($0 + i * 17) & 0xff) }
        )
    }

    return DodecaSearchIndexerFixtureHot(
        pages: pages,
        result: .success(DodecaSearchSuccessPayloadHot(files: files)),
        errorResult: .error(DodecaSearchErrorPayloadHot(
            message: "search index could not write public/search/index.json"
        ))
    )
}

private func makeDodecaHtmlProcessInput() -> DodecaHtmlProcessInputHot {
    var pathMap: [String: String] = [:]
    var codeMetadata: [String: DodecaCodeExecutionMetadataHot] = [:]
    var imageVariants: [String: DodecaResponsiveImageInfoHot] = [:]
    var viteCssMap: [String: [String]] = [:]

    for i in 0..<16 {
        pathMap["/old-\(i).css"] = "/assets/new-\(i).css"
        codeMetadata["sample-\(i).rs"] = DodecaCodeExecutionMetadataHot(
            language: "rust",
            dependencies: [
                DodecaResolvedDependencyHot(name: "facet", version: "0.29"),
                DodecaResolvedDependencyHot(name: "phon", version: nil),
            ],
            durationMs: UInt64(12 + i)
        )
        imageVariants["/hero-\(i).png"] = DodecaResponsiveImageInfoHot(
            jxlSrcset: [
                DodecaStringU32Hot(string: "/hero-\(i)-640.jxl", value: 640),
                DodecaStringU32Hot(string: "/hero-\(i)-1280.jxl", value: 1_280),
            ],
            webpSrcset: [
                DodecaStringU32Hot(string: "/hero-\(i)-640.webp", value: 640),
                DodecaStringU32Hot(string: "/hero-\(i)-1280.webp", value: 1_280),
            ]
        )
        viteCssMap["/entry-\(i).ts"] = ["/assets/entry-\(i).css", "/assets/chunk-\(i).css"]
    }

    return DodecaHtmlProcessInputHot(
        html: "<main>" + (0..<16).map { "<img src=\"/hero-\($0).png\">" }.joined() + "</main>",
        pathMap: pathMap,
        knownRoutes: Set((0..<96).map { "/guide/topic-\($0)/" } + ["/", "/guide/"]),
        codeMetadata: codeMetadata,
        injections: [
            DodecaInjectionHot(location: .head, content: "<meta charset=\"utf-8\">"),
            DodecaInjectionHot(location: .body, content: "<script type=\"module\" src=\"/entry.ts\"></script>"),
        ],
        imageVariants: imageVariants,
        viteCssMap: viteCssMap,
        mount: DodecaMountLocalizationHot(
            segment: "wiki",
            routes: Set((0..<32).map { "/wiki/section-\($0)/" } + ["/wiki/"])
        )
    )
}

private func makeStaxFlamegraphUpdate() -> StaxFlamegraphUpdateHot {
    var leaves: [FlameNodeHot] = []
    leaves.reserveCapacity(12)
    for i in 0..<12 {
        let node = FlameNodeHot(
            address: UInt64(0x2000 + i * 0x40),
            functionName: UInt32((i + 1) % 8),
            binary: 2,
            onCpuNs: UInt64(1_000 + i * 37),
            offCpu: OffCpuBreakdownHot(
                sleepNs: UInt64(10 + i),
                ioNs: UInt64(20 + i),
                mutexNs: UInt64(30 + i)
            ),
            children: []
        )
        leaves.append(node)
    }
    return StaxFlamegraphUpdateHot(
        totalOnCpuNs: 25_000,
        strings: ["main", "poll", "libbee.dylib", "stax", "tokio", "dispatch", "worker", "jit"],
        root: FlameNodeHot(
            address: 0x1000,
            functionName: 0,
            binary: 2,
            onCpuNs: 25_000,
            offCpu: OffCpuBreakdownHot(sleepNs: 100, ioNs: 200, mutexNs: 300),
            children: leaves
        )
    )
}

private func makeStaxLinuxBrokerControlFixture() -> StaxLinuxBrokerControlFixtureHot {
    StaxLinuxBrokerControlFixtureHot(
        config: StaxLinuxPerfSessionConfigHot(
            targetPid: 42_424,
            frequencyHz: 997,
            kernelStacks: true,
            requestWaking: true,
            requestPmu: true,
            requestDwarfUnwind: false
        ),
        status: StaxLinuxDaemonStatusHot(
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
        wakingFieldOffsets: StaxLinuxWakingFieldOffsetsHot(
            wakeePidOffset: 16,
            wakeePidSize: 4
        )
    )
}

private func makeDodecaByteChannelItem() -> [UInt8] {
    (0..<4096).map { UInt8(($0 * 31 + 7) & 0xff) }
}

private func makeDodecaStringChannelItem() -> String {
    (0..<64).map { "diagnostic-\($0): route /guide/topic-\($0)/ refreshed" }.joined(separator: "\n")
}

private func makeDibsMigrationLog() -> DibsMigrationLogHot {
    DibsMigrationLogHot(
        version: 42,
        level: "info",
        message: "applied migration 202606050701_add_trace_indexes with 18 statements",
        appliedCount: 18,
        elapsedMs: 1_237
    )
}

private func makeHelixPulseAvailable() -> HelixPulseAvailableHot {
    HelixPulseAvailableHot(pulseId: 9_001)
}

private func makeTraceyDataUpdate() -> TraceyDataUpdateHot {
    let covered = (0..<32).map { i in
        TraceyRuleIdHot(base: "compat.rule-\(i)", version: 1)
    }
    let uncovered = (0..<8).map { i in
        TraceyRuleIdHot(base: "channel.rule-\(i)", version: 2)
    }
    return TraceyDataUpdateHot(version: 42, newlyCovered: covered, newlyUncovered: uncovered)
}

private func main() throws {
    print("Bee/Vox Swift typed codec steady-state throughput\n")
    try benchCase(
        "feed(args): String + [Float; 4096]",
        method: "feed",
        phase: "args",
        setup: feedArgsDescriptor(),
        value: makeFeedArgs(),
        iters: 20_000
    )
    try benchCase(
        "feed(response): Result<Option<FeedResult>, BeeError>",
        method: "feed",
        phase: "response",
        setup: feedResponseDescriptor(),
        value: makeFeedResponse(),
        iters: 12_000
    )
    try benchCase(
        "setMarkedText(args)",
        method: "setMarkedText",
        phase: "args",
        setup: markedTextArgsDescriptor(),
        value: MarkedTextArgs(text: "partial dictated text", animationBudgetMs: 90),
        iters: 50_000
    )
    try benchCase(
        "advanceTranscript(args)",
        method: "advanceTranscript",
        phase: "args",
        setup: advanceTranscriptArgsDescriptor(),
        value: AdvanceTranscriptArgs(
            text: "committed prefix plus live dictated suffix",
            committedLen: 17,
            animationBudgetMs: 120
        ),
        iters: 50_000
    )
    try benchCase(
        "imeKeyEvent(args)",
        method: "imeKeyEvent",
        phase: "args",
        setup: imeKeyEventArgsDescriptor(),
        value: ImeKeyEventArgs(eventType: "keyDown", keyCode: 49, characters: " "),
        iters: 50_000
    )
    print("Compat Swift typed decode steady-state throughput\n")
    let compatSkip = compatWriterOnlySkipFixture()
    try benchDecodeCase(
        "compat(field skip): u32",
        method: "compat.field_skip",
        phase: "decode",
        writerRoot: compatSkip.writerRoot,
        reader: compatSkip.reader,
        registry: compatSkip.registry,
        bytes: compatSkip.bytes,
        expected: compatSkip.expected,
        iters: 80_000
    )
    let compatDefault = compatReaderOnlyDefaultFixture()
    try benchDecodeCase(
        "compat(default): u32?",
        method: "compat.default",
        phase: "decode",
        writerRoot: compatDefault.writerRoot,
        reader: compatDefault.reader,
        registry: compatDefault.registry,
        bytes: compatDefault.bytes,
        expected: compatDefault.expected,
        iters: 80_000
    )
    let compatEnumPayloadDrift = compatEnumPayloadDriftFixture()
    try benchDecodeCase(
        "compat(enum payload drift)",
        method: "compat.enum_payload_drift",
        phase: "decode",
        writerRoot: compatEnumPayloadDrift.writerRoot,
        reader: compatEnumPayloadDrift.reader,
        registry: compatEnumPayloadDrift.registry,
        bytes: compatEnumPayloadDrift.bytes,
        expected: compatEnumPayloadDrift.expected,
        iters: 80_000
    )
    print("Ecosystem Swift typed codec steady-state throughput\n")
    try benchCase(
        "dodeca(routes): Set<String>",
        method: "dodeca.routes",
        phase: "response",
        setup: dodecaRoutesDescriptor(),
        value: makeDodecaRoutes(),
        iters: 25_000
    )
    try benchCase(
        "dodeca(template): dynamic Value",
        method: "dodeca.template_call",
        phase: "fixture",
        setup: dodecaTemplateCallDescriptor(),
        value: makeDodecaTemplateCall(),
        iters: 15_000
    )
    try benchCase(
        "dodeca(load data): dynamic enum",
        method: "dodeca.load_data",
        phase: "response",
        setup: dodecaLoadDataResultDescriptor(),
        value: makeDodecaLoadDataResult(),
        iters: 40_000
    )
    try benchCase(
        "dodeca(parse): source map",
        method: "dodeca.parse_and_render",
        phase: "response",
        setup: dodecaParseResultDescriptor(),
        value: makeDodecaParseResult(),
        iters: 8_000
    )
    try benchCase(
        "dodeca(image): pixels/results",
        method: "dodeca.image.process",
        phase: "fixture",
        setup: dodecaImageProcessorFixtureDescriptor(),
        value: makeDodecaImageProcessorFixture(),
        iters: 4_000
    )
    try benchCase(
        "dodeca(search): pages/files",
        method: "dodeca.search.build_index",
        phase: "fixture",
        setup: dodecaSearchIndexerFixtureDescriptor(),
        value: makeDodecaSearchIndexerFixture(),
        iters: 6_000
    )
    try benchCase(
        "dodeca(html): maps/sets/tuples",
        method: "dodeca.html.process",
        phase: "fixture",
        setup: dodecaHtmlProcessInputDescriptor(),
        value: makeDodecaHtmlProcessInput(),
        iters: 8_000
    )
    try benchCase(
        "dibs(list response): SQL rows",
        method: "dibs.list",
        phase: "response",
        setup: dibsListResponseDescriptor(),
        value: makeDibsListResponse(),
        iters: 12_000
    )
    try benchCase(
        "dibs(squel service): generated roots",
        method: "dibs.squel",
        phase: "fixture",
        setup: dibsSquelServiceDescriptor(),
        value: sampleDibsSquelServiceFixture(),
        iters: 4_000
    )
    try benchCase(
        "dibs(migration service): status/migrate roots",
        method: "dibs.migration",
        phase: "fixture",
        setup: dibsMigrationServiceDescriptor(),
        value: sampleDibsMigrationServiceFixture(),
        iters: 8_000
    )
    let styxValueSetup = styxDescriptor()
    try benchCase(
        "styx(value): recursive",
        method: "styx.value",
        phase: "response",
        setup: (styxValueSetup.root, styxValueSetup.registry),
        value: sampleStyxValue(),
        iters: 4_000,
        blocks: styxValueSetup.blocks
    )
    let styxLspSetup = styxLspSurfaceDescriptor()
    try benchCase(
        "styx(lsp surface): aggregate",
        method: "styx.lsp.surface",
        phase: "response",
        setup: (styxLspSetup.root, styxLspSetup.registry),
        value: sampleStyxLspSurfaceFixture(),
        iters: 1_000,
        blocks: styxLspSetup.blocks
    )
    let staxSetup = staxFlamegraphDescriptor()
    try benchCase(
        "stax(flamegraph): recursive",
        method: "stax.flamegraph",
        phase: "update",
        setup: (staxSetup.root, staxSetup.registry),
        value: makeStaxFlamegraphUpdate(),
        iters: 4_000,
        blocks: staxSetup.blocks
    )
    try benchCase(
        "stax(linux broker): control DTOs",
        method: "stax.linux.broker_control",
        phase: "fixture",
        setup: staxLinuxBrokerControlDescriptor(),
        value: makeStaxLinuxBrokerControlFixture(),
        iters: 12_000
    )
    let helixTraceServiceSetup = helixTraceServiceSurfaceDescriptor()
    try benchCase(
        "helix(trace service): aggregate",
        method: "helix.trace_service_surface",
        phase: "fixture",
        setup: (helixTraceServiceSetup.root, helixTraceServiceSetup.registry),
        value: sampleHelixTraceServiceSurface(),
        iters: 1_000
    )
    print("Channel item Swift typed codec steady-state throughput\n")
    try benchCase(
        "dodeca(byte tunnel): [UInt8]",
        method: "dodeca.byte_tunnel",
        phase: "channel-item",
        setup: dodecaByteChannelItemDescriptor(),
        value: makeDodecaByteChannelItem(),
        iters: 20_000
    )
    try benchCase(
        "dodeca(lsp): String",
        method: "dodeca.devtools_lsp",
        phase: "channel-item",
        setup: dodecaStringChannelItemDescriptor(),
        value: makeDodecaStringChannelItem(),
        iters: 20_000
    )
    try benchCase(
        "dibs(migration log): DTO",
        method: "dibs.migrate",
        phase: "channel-item",
        setup: dibsMigrationLogDescriptor(),
        value: makeDibsMigrationLog(),
        iters: 20_000
    )
    try benchCase(
        "helix(pulse available): DTO",
        method: "helix.subscribe_pulses",
        phase: "channel-item",
        setup: helixPulseAvailableDescriptor(),
        value: makeHelixPulseAvailable(),
        iters: 50_000
    )
    try benchCase(
        "tracey(data update): DTO",
        method: "tracey.subscribe_updates",
        phase: "channel-item",
        setup: traceyDataUpdateDescriptor(),
        value: makeTraceyDataUpdate(),
        iters: 8_000
    )
    blackHole(blackHoleSink)
}

try main()

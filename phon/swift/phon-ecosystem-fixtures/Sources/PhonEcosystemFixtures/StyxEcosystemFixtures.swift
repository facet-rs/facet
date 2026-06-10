import PhonEngine
import PhonIR
import PhonSchema

public struct StyxValue: Equatable {
    var tag: StyxTag?
    var payload: StyxPayload?
    var span: StyxSpan?
}

public struct StyxTag: Equatable {
    var name: String
    var span: StyxSpan?
}

public indirect enum StyxPayload: Equatable {
    case scalar(StyxScalar)
    case sequence(StyxSequence)
    case object(StyxObject)
}

public struct StyxScalar: Equatable {
    var text: String
    var kind: StyxScalarKind
    var span: StyxSpan?
}

public enum StyxScalarKind: Equatable {
    case bare
    case quoted
    case raw
    case heredoc
}

public struct StyxSequence: Equatable {
    var items: [StyxValue]
    var span: StyxSpan?
}

public struct StyxEntry: Equatable {
    var key: StyxValue
    var value: StyxValue
    var docComment: String?
}

public struct StyxObject: Equatable {
    var entries: [StyxEntry]
    var span: StyxSpan?
}

public struct StyxSpan: Equatable {
    var start: UInt32
    var end: UInt32
}

public struct StyxLspPosition: Equatable {
    var line: UInt32
    var character: UInt32
}

public struct StyxLspRange: Equatable {
    var start: StyxLspPosition
    var end: StyxLspPosition
}

public struct StyxLspCursor: Equatable {
    var line: UInt32
    var character: UInt32
    var offset: UInt32
}

public enum StyxLspCapability: Equatable {
    case completions
    case hover
    case diagnostics
    case codeActions
    case definition
}

public struct StyxLspInitializeParams: Equatable {
    var styxVersion: String
    var documentUri: String
    var schemaId: String
}

public struct StyxLspInitializeResult: Equatable {
    var name: String
    var version: String
    var capabilities: [StyxLspCapability]
}

public struct StyxLspCompletionParams: Equatable {
    var documentUri: String
    var cursor: StyxLspCursor
    var path: [String]
    var prefix: String
    var context: StyxValue?
    var taggedContext: StyxValue?
}

public enum StyxLspCompletionKind: Equatable {
    case field
    case type
    case function
    case keyword
}

public struct StyxLspCompletionItem: Equatable {
    var label: String
    var detail: String?
    var documentation: String?
    var kind: StyxLspCompletionKind?
    var sortText: String?
    var insertText: String?
}

public struct StyxLspHoverParams: Equatable {
    var documentUri: String
    var cursor: StyxLspCursor
    var path: [String]
    var context: StyxValue?
    var taggedContext: StyxValue?
}

public struct StyxLspHoverResult: Equatable {
    var contents: String
    var range: StyxLspRange?
}

public struct StyxLspInlayHintParams: Equatable {
    var documentUri: String
    var range: StyxLspRange
    var context: StyxValue?
}

public enum StyxLspInlayHintKind: Equatable {
    case type
    case parameter
}

public struct StyxLspInlayHint: Equatable {
    var position: StyxLspPosition
    var label: String
    var kind: StyxLspInlayHintKind?
    var paddingLeft: Bool
    var paddingRight: Bool
}

public enum StyxLspDiagnosticSeverity: Equatable {
    case error
    case warning
    case information
    case hint
}

public struct StyxLspDiagnostic: Equatable {
    var span: StyxSpan
    var severity: StyxLspDiagnosticSeverity
    var message: String
    var source: String?
    var code: String?
    var data: StyxValue?
}

public struct StyxLspDiagnosticParams: Equatable {
    var documentUri: String
    var tree: StyxValue
    var content: String
}

public struct StyxLspCodeActionParams: Equatable {
    var documentUri: String
    var span: StyxSpan
    var diagnostics: [StyxLspDiagnostic]
}

public enum StyxLspCodeActionKind: Equatable {
    case quickFix
    case refactor
}

public struct StyxLspWorkspaceEdit: Equatable {
    var changes: [StyxLspDocumentEdit]
}

public struct StyxLspDocumentEdit: Equatable {
    var uri: String
    var edits: [StyxLspTextEdit]
}

public struct StyxLspTextEdit: Equatable {
    var span: StyxSpan
    var newText: String
}

public struct StyxLspCodeAction: Equatable {
    var title: String
    var kind: StyxLspCodeActionKind?
    var edit: StyxLspWorkspaceEdit?
    var isPreferred: Bool
}

public struct StyxLspDefinitionParams: Equatable {
    var documentUri: String
    var cursor: StyxLspCursor
    var path: [String]
    var context: StyxValue?
    var taggedContext: StyxValue?
}

public struct StyxLspLocation: Equatable {
    var uri: String
    var span: StyxSpan
}

public struct StyxLspSchemaInfo: Equatable {
    var source: String
    var uri: String
}

public struct StyxLspGetSubtreeParams: Equatable {
    var documentUri: String
    var path: [String]
}

public struct StyxLspGetDocumentParams: Equatable {
    var documentUri: String
}

public struct StyxLspGetSourceParams: Equatable {
    var documentUri: String
}

public struct StyxLspGetSchemaParams: Equatable {
    var documentUri: String
}

public struct StyxLspOffsetToPositionParams: Equatable {
    var documentUri: String
    var offset: UInt32
}

public struct StyxLspPositionToOffsetParams: Equatable {
    var documentUri: String
    var position: StyxLspPosition
}

public struct StyxLspSurfaceFixture: Equatable {
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
    let size = fixtureFixedSize(p)!
    return Descriptor(
        schema: .concrete(primitiveId(p)),
        layout: Layout(size: size, align: fixtureAlignment(p)),
        access: .scalar
    )
}

private func fixtureFixedSize(_ p: Primitive) -> Int? {
    switch p {
    case .unit: return 0
    case .bool, .u8, .i8: return 1
    case .u16, .i16: return 2
    case .u32, .i32, .f32, .char: return 4
    case .u64, .i64, .f64: return 8
    case .u128, .i128: return 16
    case .string, .bytes, .never, .datetime, .uuid, .qname: return nil
    }
}

private func fixtureAlignment(_ p: Primitive) -> Int {
    switch p {
    case .u16, .i16: return 2
    case .u32, .i32, .f32, .char: return 4
    case .u64, .i64, .f64: return 8
    case .u128, .i128: return 16
    default: return 1
    }
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

public func styxDescriptor() -> (root: Descriptor, registry: Registry, blocks: [SchemaId: Descriptor]) {
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

public func styxLspSurfaceDescriptor() -> (root: Descriptor, registry: Registry, blocks: [SchemaId: Descriptor]) {
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

public func sampleStyxValue() -> StyxValue {
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

public func sampleStyxLspSurfaceFixture() -> StyxLspSurfaceFixture {
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

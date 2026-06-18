import PhonEngine
import PhonIR
import PhonSchema

public struct HelixAudioTokenRange: Equatable {
    var start: UInt32
    var end: UInt32
}

public struct HelixAudioRepresentationSpan: Equatable {
    var audio: HelixAudioTokenRange
    var audioRepresentationVersion: UInt32
}

public struct HelixStreamMeta: Equatable {
    var schemaVersion: UInt32
    var pulseIds: [UInt64]
    var timelineEventCount: UInt64
    var attentionBatchCount: UInt64
}

public struct HelixVerifyOutcome: Equatable {
    var rewindK: UInt64
    var acceptedPrefixLen: UInt64?
    var divergenceRow: UInt64?
    var discardedSpeculativeTokens: UInt64?
}

public struct HelixPulseRollup: Equatable {
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

public struct HelixTextTokenSnapshot: Equatable {
    var textTokenId: UInt32
    var text: String?
    var textBefore: String?
    var inVerifyBatch: Bool
    var decodedThisPulse: Bool
}

public struct HelixPromptLayout: Equatable {
    var pulseId: UInt64
    var firstAudioTokenId: UInt32
    var residentAudioFrames: UInt64
    var changedAudioSpans: [HelixAudioRepresentationSpan]
    var textTokenStart: UInt32
    var textTokenEnd: UInt32
    var textTokens: [HelixTextTokenSnapshot]
}

public struct HelixPulseAttentionHeatmap: Equatable {
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

public struct HelixStreamMetrics: Equatable {
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

public struct HelixPulseAvailable: Equatable {
    var pulseId: UInt64
}

public struct HelixRunInfo: Equatable {
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

public struct HelixMelFrameRange: Equatable {
    var start: UInt32
    var end: UInt32
}

public struct HelixNoMergePayload: Equatable {
    var preMergeAudioTokenId: UInt32
}

public struct HelixMergedPayload: Equatable {
    var preMerge: HelixAudioTokenRange
}

public enum HelixAudioTokenMergeProvenance: Equatable {
    case noMerge(HelixNoMergePayload)
    case merged(HelixMergedPayload)
}

public struct HelixAdmitAllPayload: Equatable {
    var admissionSegment: UInt32
}

public enum HelixAudioTokenAdmissionProvenance: Equatable {
    case admitAll(HelixAdmitAllPayload)
}

public struct HelixAudioTokenProvenance: Equatable {
    var audioTokenId: UInt32
    var audioRepresentationVersion: UInt32
    var melFrames: [HelixMelFrameRange]
    var nativeWindow: UInt32
    var convStemChunk: UInt32
    var postMergeAudioTokenId: UInt32
    var merge: HelixAudioTokenMergeProvenance
    var admission: HelixAudioTokenAdmissionProvenance
    var cosineToPrevious: Float?
}

public struct HelixTextAttendanceRow: Equatable {
    var textTokenId: UInt32
    var decoderLayerIndex: UInt32
    var headIndex: UInt32
    var dominantAudioMass: Float
    var totalAudioMass: Float
    var observedAudio: HelixAudioTokenRange
    var dominantAudio: HelixAudioTokenRange
    var audioWeights: [Float]
    var queriedAudioWeight: Float
}

public struct HelixAudioAttendanceRow: Equatable {
    var decoderLayerIndex: UInt32
    var headIndex: UInt32
    var dominantAudioMass: Float
    var totalAudioMass: Float
    var centerAudioToken: Float?
    var widthAudioTokens: Float?
    var observedAudio: HelixAudioTokenRange
    var dominantAudio: HelixAudioTokenRange
    var audioWeights: [Float]
}

public struct HelixRefreshAttendanceRow: Equatable {
    var queryPosition: UInt32
    var decoderLayerIndex: UInt32
    var headIndex: UInt32
    var dominantAudioMass: Float
    var totalAudioMass: Float
    var centerAudioToken: Float?
    var widthAudioTokens: Float?
    var observedAudio: HelixAudioTokenRange
    var dominantAudio: HelixAudioTokenRange
    var audioWeights: [Float]
}

public struct HelixAudioSelfAttentionRow: Equatable {
    var encoderLayerIndex: UInt32
    var headIndex: UInt32
    var audioRepresentationVersion: UInt32
    var dominantAudioMass: Float
    var totalAudioMass: Float
    var centerAudioToken: Float?
    var widthAudioTokens: Float?
    var observedAudio: HelixAudioTokenRange
    var dominantAudio: HelixAudioTokenRange
    var frontierDebt: Float
}

public struct HelixTranscriptToken: Equatable {
    var textTokenId: UInt32
    var decodedInPulse: UInt64
    var text: String
    var committed: Bool
}

public struct HelixAudioClip: Equatable {
    var sampleRate: UInt32
    var firstSample: UInt64
    var samples: [Float]
}

public struct HelixMelClip: Equatable {
    var numMelBins: UInt32
    var firstMelFrame: UInt32
    var numMelFrames: UInt32
    var values: [Float]
    var minValue: Float
    var maxValue: Float
    var corpusMinValue: Float
    var corpusMaxValue: Float
}

public struct HelixAttentionSupportSummary: Equatable {
    var totalAudioMass: Float
    var observedAudio: HelixAudioTokenRange
    var dominantAudio: HelixAudioTokenRange
    var dominantAudioMass: Float
    var centerAudioToken: Float?
    var widthAudioTokens: Float?
}

public struct HelixTextAttentionSupportRecord: Equatable {
    var textTokenId: UInt32
    var queryPosition: UInt32
    var decoderLayerIndex: UInt32
    var headIndex: UInt32
    var support: HelixAttentionSupportSummary
    var audioWeights: [Float]
}

public struct HelixAudioEncoderSupportRecord: Equatable {
    var audioTokenId: UInt32
    var audioRepresentationVersion: UInt32
    var encoderLayerIndex: UInt32
    var headIndex: UInt32
    var support: HelixAttentionSupportSummary
    var frontierDebt: Float
}

public struct HelixDecodeEvidencePayload: Equatable {
    var inputTokenId: UInt32
}

public struct HelixVerifyPredictionEvidencePayload: Equatable {
    var verifiedDraftIndex: UInt32
    var draftTokenId: UInt32
    var queryRow: UInt32
    var maxLogit: Float
    var draftLogit: Float
}

public struct HelixVerifySeedEvidencePayload: Equatable {
    var queryRow: UInt32
    var nextTokenSeed: UInt32
    var maxLogit: Float
}

public enum HelixDecoderEvidenceKind: Equatable {
    case decode(HelixDecodeEvidencePayload)
    case verifyPrediction(HelixVerifyPredictionEvidencePayload)
    case verifySeed(HelixVerifySeedEvidencePayload)
    case promptPrefill
}

public struct HelixDecoderEvidenceRecord: Equatable {
    var textTokenId: UInt32?
    var queryPosition: UInt32
    var expectedObservedAudio: HelixAudioTokenRange
    var records: [HelixTextAttentionSupportRecord]
    var kind: HelixDecoderEvidenceKind
}

public struct HelixQueryRowAttentionRecord: Equatable {
    var queryPosition: UInt32
    var decoderLayerIndex: UInt32
    var headIndex: UInt32
    var support: HelixAttentionSupportSummary
    var audioWeights: [Float]
}

public struct HelixAttentionSummaryBatch: Equatable {
    var schemaVersion: UInt32
    var pulseId: UInt64
    var audioContextId: UInt64
    var textContextId: UInt64
    var audioRepresentationSpans: [HelixAudioRepresentationSpan]
    var changedAudioRepresentationSpans: [HelixAudioRepresentationSpan]
    var textSupport: [HelixTextAttentionSupportRecord]
    var headerTextSupport: [HelixQueryRowAttentionRecord]
    var audioEncoderSupport: [HelixAudioEncoderSupportRecord]
    var decoderEvidence: [HelixDecoderEvidenceRecord]
}

public enum HelixVerifyDraftStatus: Equatable {
    case accepted
    case divergent
    case discardedAfterDivergence
}

public struct HelixVerifyDraftRow: Equatable {
    var draftIndex: UInt32
    var draftTokenId: UInt32
    var verifiedTextTokenId: UInt32
    var text: String
    var status: HelixVerifyDraftStatus
    var expectedObservedAudio: HelixAudioTokenRange
    var maxDominantAudioMass: Float
    var recordCount: UInt32
    var maxLogit: Float
    var draftLogit: Float
}

public struct HelixVerifySeedRow: Equatable {
    var queryRow: UInt32
    var nextTokenSeed: UInt32
    var expectedObservedAudio: HelixAudioTokenRange
    var maxDominantAudioMass: Float
    var recordCount: UInt32
    var maxLogit: Float
}

public struct HelixVerifyEvidenceDigest: Equatable {
    var pulseId: UInt64
    var rewindK: UInt64
    var acceptedPrefixLen: UInt64?
    var divergenceRow: UInt64?
    var drafts: [HelixVerifyDraftRow]
    var seed: HelixVerifySeedRow?
}

public struct HelixDecodeFact: Equatable {
    var textTokenId: UInt32
    var queryPosition: UInt32
    var inputTokenId: UInt32
    var observedAudio: HelixAudioTokenRange
}

public struct HelixVerifyPredictionFact: Equatable {
    var verifiedTextTokenId: UInt32
    var verifiedDraftIndex: UInt32
    var draftTokenId: UInt32
    var queryRow: UInt32
    var queryPosition: UInt32
    var observedAudio: HelixAudioTokenRange
}

public struct HelixVerifySeedFact: Equatable {
    var queryRow: UInt32
    var queryPosition: UInt32
    var nextTokenSeed: UInt32
    var observedAudio: HelixAudioTokenRange
}

public struct HelixPromptPrefillFact: Equatable {
    var queryPosition: UInt32
    var observedAudio: HelixAudioTokenRange
}

public struct HelixDecoderEvidenceFactCounts: Equatable {
    var decode: UInt32
    var verifyPrediction: UInt32
    var verifySeed: UInt32
    var promptPrefill: UInt32
}

public struct HelixEncoderFactsSnapshot: Equatable {
    var refreshedAudio: HelixAudioTokenRange
    var audioRepresentationVersion: UInt32
    var provenance: [HelixAudioTokenProvenance]
}

public struct HelixPulseEvidenceSnapshot: Equatable {
    var pulseId: UInt64
    var encoder: HelixEncoderFactsSnapshot?
    var counts: HelixDecoderEvidenceFactCounts
    var decode: [HelixDecodeFact]
    var verifyPrediction: [HelixVerifyPredictionFact]
    var verifySeed: [HelixVerifySeedFact]
    var promptPrefill: [HelixPromptPrefillFact]
}

public enum HelixEncoderProvenanceViolationKind: Equatable {
    case missingProvenance
    case versionMismatch
    case emptyMelFrames
    case nonFiniteFrontierDebt
}

public struct HelixEncoderProvenanceViolation: Equatable {
    var audioTokenId: UInt32
    var encoderLayerIndex: UInt32
    var headIndex: UInt32
    var observedAudioTokenId: UInt32?
    var kind: HelixEncoderProvenanceViolationKind
    var message: String
}

public struct HelixEncoderProvenanceReport: Equatable {
    var pulseId: UInt64
    var recordsChecked: UInt64
    var violations: [HelixEncoderProvenanceViolation]
}

public struct HelixDecoderEvidenceVariantCounts: Equatable {
    var decode: UInt64
    var verifyPrediction: UInt64
    var verifySeed: UInt64
    var promptPrefill: UInt64
}

public struct HelixDecoderEvidenceReport: Equatable {
    var totalBatches: UInt64
    var batchesWithoutDecoderEvidence: UInt64
    var pulsesWithoutDecoderEvidence: [UInt64]
    var variantEvidenceCounts: HelixDecoderEvidenceVariantCounts
    var variantRecordCounts: HelixDecoderEvidenceVariantCounts
    var observedDecoderLayerIndices: [UInt32]
    var observedDecoderHeadIndices: [UInt32]
}

public struct HelixEncoderFrontierPoint: Equatable {
    var audioTokenId: UInt32
    var meanFrontierDebt: Float
    var headCount: UInt32
}

public struct HelixEncoderFrontierLayer: Equatable {
    var encoderLayerIndex: UInt32
    var points: [HelixEncoderFrontierPoint]
}

public struct HelixEncoderFrontierSeries: Equatable {
    var pulseId: UInt64
    var layers: [HelixEncoderFrontierLayer]
    var minAudioTokenId: UInt32
    var maxAudioTokenId: UInt32
    var minFrontierDebt: Float
    var maxFrontierDebt: Float
}

public struct HelixTracePositionSpan: Equatable {
    var logicalStart: UInt64
    var rows: UInt64
    var physicalStart: UInt64
}

public enum HelixArDecodeEarlyExitReason: Equatable {
    case budgetExhausted
    case noBudget
    case seedWasEos
    case producedEos
}

public enum HelixVerifySkippedReason: Equatable {
    case rewindGuardFailed
    case preCommitFullRewind
}

public struct HelixPulseTracePayload: Equatable {
    var startUs: UInt64
    var durationUs: UInt64
    var pulseId: UInt64
    var previousConsumedMelFrames: UInt64
    var consumedMelFrames: UInt64
    var pulseMelFrames: UInt64
    var committedTextLenStart: UInt64
    var speculativeLenStart: UInt64
    var committedTokens: UInt64
    var retainedSpeculativeTokens: UInt64
    var residentCommittedTokens: UInt64
    var evictedAudioTokens: UInt64
    var evictedCommittedTokens: UInt64
}

public struct HelixRefreshPromptTracePayload: Equatable {
    var startUs: UInt64
    var durationUs: UInt64
    var pulseId: UInt64
    var firstAudioTokenId: UInt64
    var residentAudioFrames: UInt64
    var committedTextLen: UInt64
    var residentCommittedLen: UInt64
    var residentTextLen: UInt64
    var logicalStart: UInt64
    var logicalEnd: UInt64
    var textTokenStart: UInt64
    var textTokenEnd: UInt64
    var spans: [HelixTracePositionSpan]
}

public struct HelixVerifyTracePayload: Equatable {
    var startUs: UInt64
    var durationUs: UInt64
    var pulseId: UInt64
    var rewindK: UInt64
    var postRewindTextLen: UInt64
    var textTokenStart: UInt64
    var textTokenEnd: UInt64
    var logicalStart: UInt64
    var logicalEnd: UInt64
    var spans: [HelixTracePositionSpan]
    var acceptedPrefixLen: UInt64?
    var divergenceRow: UInt64?
    var nextTokenSeed: UInt64?
    var discardedSpeculativeTokens: UInt64?
    var invalidatedSpeculativeSlots: UInt64?
}

public struct HelixArDecodeTracePayload: Equatable {
    var startUs: UInt64
    var durationUs: UInt64
    var pulseId: UInt64
    var decodeSteps: UInt64
    var decodedTokens: UInt64
    var speculativeLenEntering: UInt64
    var liveSpeculativeTokens: UInt64
    var hitEos: Bool
    var seedTokenId: UInt64
    var seedTokenText: String
    var earlyExitReason: HelixArDecodeEarlyExitReason
    var nextAfterTail: UInt64
}

public struct HelixArTokenTracePayload: Equatable {
    var startUs: UInt64
    var durationUs: UInt64
    var pulseId: UInt64
    var stepIndex: UInt64
    var inputTokenId: UInt64
    var inputText: String
    var textTokenId: UInt64
    var queryPosition: UInt64
    var physicalStart: UInt64
    var summaryRecords: UInt64
    var nextTokenId: UInt64
    var nextText: String
}

public struct HelixCommitTracePayload: Equatable {
    var startUs: UInt64
    var durationUs: UInt64
    var pulseId: UInt64
    var speculativeLenPre: UInt64
    var revisableTailTarget: UInt64
    var committedTokens: UInt64
    var retainedSpeculativeTokens: UInt64
    var committedTextLen: UInt64
    var nextAfterCommitted: UInt64
}

public struct HelixVerifySkippedTracePayload: Equatable {
    var timestampUs: UInt64
    var pulseId: UInt64
    var reason: HelixVerifySkippedReason
    var rewindK: UInt64
    var residentCommittedLen: UInt64
    var speculativeLen: UInt64
}

public enum HelixStreamingTraceEvent: Equatable {
    case pulse(HelixPulseTracePayload)
    case refreshPrompt(HelixRefreshPromptTracePayload)
    case verify(HelixVerifyTracePayload)
    case arDecode(HelixArDecodeTracePayload)
    case arToken(HelixArTokenTracePayload)
    case commit(HelixCommitTracePayload)
    case verifySkipped(HelixVerifySkippedTracePayload)
}

public struct HelixChromeTraceEvent: Equatable {
    var name: String
    var cat: String
    var ph: String
    var ts: Double
    var dur: Double?
    var pid: UInt32
    var tid: UInt32
    var s: String?
    var args: [String: Value]
}

public struct HelixPulseBundleFields: Equatable {
    var promptLayout: Bool
    var audioProvenance: Bool
    var attentionHeatmap: Bool
    var encoderFrontier: Bool
    var encoderProvenance: Bool
    var audioClip: Bool
    var melClip: Bool
    var pulseRollup: Bool
    var timeline: Bool
    var gpuChromeEvents: Bool
    var verifyEvidence: Bool
    var schedulerSnapshot: Bool
}

public struct HelixPulseBundle: Equatable {
    var pulseId: UInt64
    var schemaVersion: UInt32
    var promptLayout: HelixPromptLayout?
    var audioProvenance: [HelixAudioTokenProvenance]?
    var attentionHeatmap: HelixPulseAttentionHeatmap?
    var encoderFrontier: HelixEncoderFrontierSeries?
    var encoderProvenance: HelixEncoderProvenanceReport?
    var audioClip: HelixAudioClip?
    var melClip: HelixMelClip?
    var pulseRollup: HelixPulseRollup?
    var timeline: [HelixStreamingTraceEvent]?
    var gpuChromeEvents: [HelixChromeTraceEvent]?
    var verifyEvidence: HelixVerifyEvidenceDigest?
    var schedulerSnapshot: HelixPulseEvidenceSnapshot?
}

public struct HelixPieceEvalSnapshot: Equatable {
    var audioNowMs: Double
    var referenceWordsAvailable: UInt32
    var hypothesisWords: UInt32
    var substitutions: UInt32
    var deletions: UInt32
    var insertions: UInt32
    var rollingWer: Double
    var s2dMatchedWords: UInt32
    var s2dNewWords: UInt32
    var s2dP50Ms: Double?
    var s2dP90Ms: Double?
    var s2dP100Ms: Double?
    var s2dAvgMs: Double?
    var audioFrontier: UInt32
    var displayedFrontier: UInt32
    var committedFrontier: UInt32
    var lagMs: Double
}

public struct HelixPieceEvalReference: Equatable {
    var piece: String
    var language: String
    var words: [String]
}

public struct HelixTraceServiceSurface: Equatable {
    var meta: HelixStreamMeta
    var pulseRollup: HelixPulseRollup?
    var timeline: [HelixStreamingTraceEvent]
    var attentionBatch: HelixAttentionSummaryBatch?
    var promptLayout: HelixPromptLayout?
    var audioAttendedBy: [HelixTextAttendanceRow]
    var textAttendsTo: [HelixAudioAttendanceRow]
    var refreshAttendsTo: [HelixRefreshAttendanceRow]
    var audioTokenProvenance: HelixAudioTokenProvenance?
    var audioProvenanceForPulse: [HelixAudioTokenProvenance]
    var audioTokensForMelFrame: [UInt32]
    var audioClipForAudioToken: HelixAudioClip?
    var audioClipForPrompt: HelixAudioClip?
    var audioClipForAudioRange: HelixAudioClip?
    var melClipForPrompt: HelixMelClip?
    var audioSelfAttention: [HelixAudioSelfAttentionRow]
    var transcript: [HelixTranscriptToken]
    var pulseAttentionHeatmap: HelixPulseAttentionHeatmap?
    var encoderFrontier: HelixEncoderFrontierSeries?
    var streamMetrics: HelixStreamMetrics
    var verifyEvidence: HelixVerifyEvidenceDigest?
    var decoderEvidenceReport: HelixDecoderEvidenceReport
    var pulseEvidenceSnapshot: HelixPulseEvidenceSnapshot?
    var gpuChromeEventsForPulse: [HelixChromeTraceEvent]
    var runInfo: HelixRunInfo?
    var pieceEvalReference: HelixPieceEvalReference?
    var pieceEvalForPulse: HelixPieceEvalSnapshot?
    var encoderProvenanceReport: HelixEncoderProvenanceReport?
    var pulseBundleFields: HelixPulseBundleFields
    var pulseBundle: HelixPulseBundle
    var pulseAvailable: HelixPulseAvailable
}

private enum HelixSchema {
    static let optionU64 = SchemaId(701)
    static let optionString = SchemaId(702)
    static let optionF32 = SchemaId(703)
    static let optionF64 = SchemaId(704)
    static let optionTextTokenId = SchemaId(705)
    static let optionAudioTokenId = SchemaId(706)
    static let u32List = SchemaId(707)
    static let u64List = SchemaId(708)
    static let f32List = SchemaId(709)
    static let f64List = SchemaId(710)
    static let stringList = SchemaId(711)
    static let dynamic = SchemaId(712)
    static let mapStringDynamic = SchemaId(713)
    static let audioTokenRange = SchemaId(714)
    static let audioRepresentationSpan = SchemaId(715)
    static let audioRepresentationSpanList = SchemaId(716)
    static let streamMeta = SchemaId(717)
    static let verifyOutcome = SchemaId(718)
    static let optionVerifyOutcome = SchemaId(719)
    static let pulseRollup = SchemaId(720)
    static let optionPulseRollup = SchemaId(721)
    static let textTokenSnapshot = SchemaId(722)
    static let textTokenSnapshotList = SchemaId(723)
    static let promptLayout = SchemaId(724)
    static let optionPromptLayout = SchemaId(725)
    static let attentionHeatmap = SchemaId(726)
    static let optionAttentionHeatmap = SchemaId(727)
    static let streamMetrics = SchemaId(728)
    static let pulseAvailable = SchemaId(729)
    static let runInfo = SchemaId(730)
    static let optionRunInfo = SchemaId(731)
    static let melFrameRange = SchemaId(732)
    static let melFrameRangeList = SchemaId(733)
    static let mergeProvenance = SchemaId(734)
    static let admissionProvenance = SchemaId(735)
    static let audioTokenProvenance = SchemaId(736)
    static let audioTokenProvenanceList = SchemaId(737)
    static let optionAudioTokenProvenance = SchemaId(738)
    static let optionAudioTokenProvenanceList = SchemaId(739)
    static let textAttendanceRow = SchemaId(740)
    static let textAttendanceRowList = SchemaId(741)
    static let audioAttendanceRow = SchemaId(742)
    static let audioAttendanceRowList = SchemaId(743)
    static let refreshAttendanceRow = SchemaId(744)
    static let refreshAttendanceRowList = SchemaId(745)
    static let audioSelfAttentionRow = SchemaId(746)
    static let audioSelfAttentionRowList = SchemaId(747)
    static let transcriptToken = SchemaId(748)
    static let transcriptTokenList = SchemaId(749)
    static let audioClip = SchemaId(750)
    static let optionAudioClip = SchemaId(751)
    static let melClip = SchemaId(752)
    static let optionMelClip = SchemaId(753)
    static let supportSummary = SchemaId(754)
    static let textSupportRecord = SchemaId(755)
    static let textSupportRecordList = SchemaId(756)
    static let audioEncoderSupportRecord = SchemaId(757)
    static let audioEncoderSupportRecordList = SchemaId(758)
    static let decoderEvidenceKind = SchemaId(759)
    static let decoderEvidenceRecord = SchemaId(760)
    static let decoderEvidenceRecordList = SchemaId(761)
    static let queryRowAttentionRecord = SchemaId(762)
    static let queryRowAttentionRecordList = SchemaId(763)
    static let attentionSummaryBatch = SchemaId(764)
    static let optionAttentionSummaryBatch = SchemaId(765)
    static let verifyDraftStatus = SchemaId(766)
    static let verifyDraftRow = SchemaId(767)
    static let verifyDraftRowList = SchemaId(768)
    static let verifySeedRow = SchemaId(769)
    static let optionVerifySeedRow = SchemaId(770)
    static let verifyEvidenceDigest = SchemaId(771)
    static let optionVerifyEvidenceDigest = SchemaId(772)
    static let decodeFact = SchemaId(773)
    static let decodeFactList = SchemaId(774)
    static let verifyPredictionFact = SchemaId(775)
    static let verifyPredictionFactList = SchemaId(776)
    static let verifySeedFact = SchemaId(777)
    static let verifySeedFactList = SchemaId(778)
    static let promptPrefillFact = SchemaId(779)
    static let promptPrefillFactList = SchemaId(780)
    static let factCounts = SchemaId(781)
    static let encoderFactsSnapshot = SchemaId(782)
    static let optionEncoderFactsSnapshot = SchemaId(783)
    static let pulseEvidenceSnapshot = SchemaId(784)
    static let optionPulseEvidenceSnapshot = SchemaId(785)
    static let provenanceViolationKind = SchemaId(786)
    static let provenanceViolation = SchemaId(787)
    static let provenanceViolationList = SchemaId(788)
    static let encoderProvenanceReport = SchemaId(789)
    static let optionEncoderProvenanceReport = SchemaId(790)
    static let variantCounts = SchemaId(791)
    static let decoderEvidenceReport = SchemaId(792)
    static let frontierPoint = SchemaId(793)
    static let frontierPointList = SchemaId(794)
    static let frontierLayer = SchemaId(795)
    static let frontierLayerList = SchemaId(796)
    static let frontierSeries = SchemaId(797)
    static let optionFrontierSeries = SchemaId(798)
    static let tracePositionSpan = SchemaId(799)
    static let tracePositionSpanList = SchemaId(800)
    static let arDecodeEarlyExitReason = SchemaId(801)
    static let verifySkippedReason = SchemaId(802)
    static let streamingTraceEvent = SchemaId(803)
    static let streamingTraceEventList = SchemaId(804)
    static let optionStreamingTraceEventList = SchemaId(805)
    static let chromeTraceEvent = SchemaId(806)
    static let chromeTraceEventList = SchemaId(807)
    static let optionChromeTraceEventList = SchemaId(808)
    static let pulseBundleFields = SchemaId(809)
    static let pulseBundle = SchemaId(810)
    static let pieceEvalSnapshot = SchemaId(811)
    static let optionPieceEvalSnapshot = SchemaId(812)
    static let pieceEvalReference = SchemaId(813)
    static let optionPieceEvalReference = SchemaId(814)
    static let traceServiceSurface = SchemaId(815)
}

private func schemaRef(_ id: SchemaId) -> SchemaRef {
    .concrete(id)
}

private func primitiveRef(_ primitive: Primitive) -> SchemaRef {
    .concrete(primitiveId(primitive))
}

private func field(_ name: String, _ schema: SchemaRef) -> Field {
    Field(name: name, schema: schema, required: true)
}

private func helixSchemas() -> [Schema] {
    [
        Schema(id: HelixSchema.optionU64, kind: .option(element: primitiveRef(.u64))),
        Schema(id: HelixSchema.optionString, kind: .option(element: primitiveRef(.string))),
        Schema(id: HelixSchema.optionF32, kind: .option(element: primitiveRef(.f32))),
        Schema(id: HelixSchema.optionF64, kind: .option(element: primitiveRef(.f64))),
        Schema(id: HelixSchema.optionTextTokenId, kind: .option(element: primitiveRef(.u32))),
        Schema(id: HelixSchema.optionAudioTokenId, kind: .option(element: primitiveRef(.u32))),
        Schema(id: HelixSchema.u32List, kind: .list(element: primitiveRef(.u32))),
        Schema(id: HelixSchema.u64List, kind: .list(element: primitiveRef(.u64))),
        Schema(id: HelixSchema.f32List, kind: .list(element: primitiveRef(.f32))),
        Schema(id: HelixSchema.f64List, kind: .list(element: primitiveRef(.f64))),
        Schema(id: HelixSchema.stringList, kind: .list(element: primitiveRef(.string))),
        Schema(id: HelixSchema.dynamic, kind: .dynamic),
        Schema(id: HelixSchema.mapStringDynamic, kind: .map(key: primitiveRef(.string), value: schemaRef(HelixSchema.dynamic))),
        Schema(id: HelixSchema.audioTokenRange, kind: .structure(name: "HelixAudioTokenRange", fields: [
            field("start", primitiveRef(.u32)),
            field("end", primitiveRef(.u32)),
        ])),
        Schema(id: HelixSchema.audioRepresentationSpan, kind: .structure(name: "HelixAudioRepresentationSpan", fields: [
            field("audio", schemaRef(HelixSchema.audioTokenRange)),
            field("audio_representation_version", primitiveRef(.u32)),
        ])),
        Schema(id: HelixSchema.audioRepresentationSpanList, kind: .list(element: schemaRef(HelixSchema.audioRepresentationSpan))),
        Schema(id: HelixSchema.streamMeta, kind: .structure(name: "HelixStreamMeta", fields: [
            field("schema_version", primitiveRef(.u32)),
            field("pulse_ids", schemaRef(HelixSchema.u64List)),
            field("timeline_event_count", primitiveRef(.u64)),
            field("attention_batch_count", primitiveRef(.u64)),
        ])),
        Schema(id: HelixSchema.verifyOutcome, kind: .structure(name: "HelixVerifyOutcome", fields: [
            field("rewind_k", primitiveRef(.u64)),
            field("accepted_prefix_len", schemaRef(HelixSchema.optionU64)),
            field("divergence_row", schemaRef(HelixSchema.optionU64)),
            field("discarded_speculative_tokens", schemaRef(HelixSchema.optionU64)),
        ])),
        Schema(id: HelixSchema.optionVerifyOutcome, kind: .option(element: schemaRef(HelixSchema.verifyOutcome))),
        Schema(id: HelixSchema.pulseRollup, kind: .structure(name: "HelixPulseRollup", fields: [
            field("pulse_id", primitiveRef(.u64)),
            field("pulse_start_us", schemaRef(HelixSchema.optionU64)),
            field("pulse_duration_us", schemaRef(HelixSchema.optionU64)),
            field("encoder_duration_us", schemaRef(HelixSchema.optionU64)),
            field("refresh_duration_us", schemaRef(HelixSchema.optionU64)),
            field("verify_duration_us", schemaRef(HelixSchema.optionU64)),
            field("decode_duration_us", schemaRef(HelixSchema.optionU64)),
            field("commit_duration_us", schemaRef(HelixSchema.optionU64)),
            field("pulse_mel_frames", primitiveRef(.u64)),
            field("committed_tokens", primitiveRef(.u64)),
            field("retained_speculative_tokens", primitiveRef(.u64)),
            field("resident_committed_tokens", primitiveRef(.u64)),
            field("evicted_audio_tokens", primitiveRef(.u64)),
            field("evicted_committed_tokens", primitiveRef(.u64)),
            field("decoded_tokens", primitiveRef(.u64)),
            field("hit_eos", primitiveRef(.bool)),
            field("verify", schemaRef(HelixSchema.optionVerifyOutcome)),
            field("has_attention_batch", primitiveRef(.bool)),
            field("ar_token_count", primitiveRef(.u64)),
        ])),
        Schema(id: HelixSchema.optionPulseRollup, kind: .option(element: schemaRef(HelixSchema.pulseRollup))),
        Schema(id: HelixSchema.textTokenSnapshot, kind: .structure(name: "HelixTextTokenSnapshot", fields: [
            field("text_token_id", primitiveRef(.u32)),
            field("text", schemaRef(HelixSchema.optionString)),
            field("text_before", schemaRef(HelixSchema.optionString)),
            field("in_verify_batch", primitiveRef(.bool)),
            field("decoded_this_pulse", primitiveRef(.bool)),
        ])),
        Schema(id: HelixSchema.textTokenSnapshotList, kind: .list(element: schemaRef(HelixSchema.textTokenSnapshot))),
        Schema(id: HelixSchema.promptLayout, kind: .structure(name: "HelixPromptLayout", fields: [
            field("pulse_id", primitiveRef(.u64)),
            field("first_audio_token_id", primitiveRef(.u32)),
            field("resident_audio_frames", primitiveRef(.u64)),
            field("changed_audio_spans", schemaRef(HelixSchema.audioRepresentationSpanList)),
            field("text_token_start", primitiveRef(.u32)),
            field("text_token_end", primitiveRef(.u32)),
            field("text_tokens", schemaRef(HelixSchema.textTokenSnapshotList)),
        ])),
        Schema(id: HelixSchema.optionPromptLayout, kind: .option(element: schemaRef(HelixSchema.promptLayout))),
        Schema(id: HelixSchema.attentionHeatmap, kind: .structure(name: "HelixPulseAttentionHeatmap", fields: [
            field("pulse_id", primitiveRef(.u64)),
            field("first_audio_token_id", primitiveRef(.u32)),
            field("audio_token_count", primitiveRef(.u32)),
            field("text_token_start", primitiveRef(.u32)),
            field("text_token_count", primitiveRef(.u32)),
            field("record_count", primitiveRef(.u32)),
            field("max_value", primitiveRef(.f32)),
            field("mean_audio_mass", schemaRef(HelixSchema.f32List)),
            field("text_token_glyphs", schemaRef(HelixSchema.stringList)),
        ])),
        Schema(id: HelixSchema.optionAttentionHeatmap, kind: .option(element: schemaRef(HelixSchema.attentionHeatmap))),
        Schema(id: HelixSchema.streamMetrics, kind: .structure(name: "HelixStreamMetrics", fields: [
            field("pulse_ids", schemaRef(HelixSchema.u64List)),
            field("pulse_duration_us", schemaRef(HelixSchema.u64List)),
            field("decoded_tokens", schemaRef(HelixSchema.u64List)),
            field("committed_tokens", schemaRef(HelixSchema.u64List)),
            field("retained_speculative_tokens", schemaRef(HelixSchema.u64List)),
            field("evicted_audio_tokens", schemaRef(HelixSchema.u64List)),
            field("evicted_committed_tokens", schemaRef(HelixSchema.u64List)),
            field("rewind_k", schemaRef(HelixSchema.u64List)),
            field("ar_token_count", schemaRef(HelixSchema.u64List)),
            field("rolling_wer", schemaRef(HelixSchema.f64List)),
            field("s2d_p50_ms", schemaRef(HelixSchema.f64List)),
        ])),
        Schema(id: HelixSchema.pulseAvailable, kind: .structure(name: "HelixPulseAvailable", fields: [
            field("pulse_id", primitiveRef(.u64)),
        ])),
        Schema(id: HelixSchema.runInfo, kind: .structure(name: "HelixRunInfo", fields: [
            field("backend", primitiveRef(.string)),
            field("model_dir", primitiveRef(.string)),
            field("input", primitiveRef(.string)),
            field("piece", schemaRef(HelixSchema.optionString)),
            field("pulse_ms", primitiveRef(.u32)),
            field("audio_ring_capacity", primitiveRef(.u32)),
            field("text_ring_capacity", primitiveRef(.u32)),
            field("commit_revisable_tail_text_tokens", primitiveRef(.u32)),
            field("revise_logit_margin", primitiveRef(.f32)),
            field("sample_rate", primitiveRef(.u32)),
            field("mel_hop_samples", primitiveRef(.u32)),
            field("num_mel_bins", primitiveRef(.u32)),
            field("num_mel_frames", primitiveRef(.u32)),
            field("audio_tokens_per_chunk", primitiveRef(.u32)),
            field("native_window_tokens", primitiveRef(.u32)),
            field("realtime_pacing", primitiveRef(.bool)),
            field("profile_phases", primitiveRef(.bool)),
            field("attention_trace_schema_version", primitiveRef(.u32)),
            field("trace_server_schema_version", primitiveRef(.u32)),
        ])),
        Schema(id: HelixSchema.optionRunInfo, kind: .option(element: schemaRef(HelixSchema.runInfo))),
        Schema(id: HelixSchema.melFrameRange, kind: .structure(name: "HelixMelFrameRange", fields: [
            field("start", primitiveRef(.u32)),
            field("end", primitiveRef(.u32)),
        ])),
        Schema(id: HelixSchema.melFrameRangeList, kind: .list(element: schemaRef(HelixSchema.melFrameRange))),
        Schema(id: HelixSchema.mergeProvenance, kind: .enumeration(name: "HelixAudioTokenMergeProvenance", variants: [
            Variant(name: "NoMerge", index: 0, payload: .structure([
                field("pre_merge_audio_token_id", primitiveRef(.u32)),
            ])),
            Variant(name: "Merged", index: 1, payload: .structure([
                field("pre_merge", schemaRef(HelixSchema.audioTokenRange)),
            ])),
        ])),
        Schema(id: HelixSchema.admissionProvenance, kind: .enumeration(name: "HelixAudioTokenAdmissionProvenance", variants: [
            Variant(name: "AdmitAll", index: 0, payload: .structure([
                field("admission_segment", primitiveRef(.u32)),
            ])),
        ])),
        Schema(id: HelixSchema.audioTokenProvenance, kind: .structure(name: "HelixAudioTokenProvenance", fields: [
            field("audio_token_id", primitiveRef(.u32)),
            field("audio_representation_version", primitiveRef(.u32)),
            field("mel_frames", schemaRef(HelixSchema.melFrameRangeList)),
            field("native_window", primitiveRef(.u32)),
            field("conv_stem_chunk", primitiveRef(.u32)),
            field("post_merge_audio_token_id", primitiveRef(.u32)),
            field("merge", schemaRef(HelixSchema.mergeProvenance)),
            field("admission", schemaRef(HelixSchema.admissionProvenance)),
            field("cosine_to_previous", schemaRef(HelixSchema.optionF32)),
        ])),
        Schema(id: HelixSchema.audioTokenProvenanceList, kind: .list(element: schemaRef(HelixSchema.audioTokenProvenance))),
        Schema(id: HelixSchema.optionAudioTokenProvenance, kind: .option(element: schemaRef(HelixSchema.audioTokenProvenance))),
        Schema(id: HelixSchema.optionAudioTokenProvenanceList, kind: .option(element: schemaRef(HelixSchema.audioTokenProvenanceList))),
        Schema(id: HelixSchema.textAttendanceRow, kind: .structure(name: "HelixTextAttendanceRow", fields: [
            field("text_token_id", primitiveRef(.u32)),
            field("decoder_layer_index", primitiveRef(.u32)),
            field("head_index", primitiveRef(.u32)),
            field("dominant_audio_mass", primitiveRef(.f32)),
            field("total_audio_mass", primitiveRef(.f32)),
            field("observed_audio", schemaRef(HelixSchema.audioTokenRange)),
            field("dominant_audio", schemaRef(HelixSchema.audioTokenRange)),
            field("audio_weights", schemaRef(HelixSchema.f32List)),
            field("queried_audio_weight", primitiveRef(.f32)),
        ])),
        Schema(id: HelixSchema.textAttendanceRowList, kind: .list(element: schemaRef(HelixSchema.textAttendanceRow))),
        Schema(id: HelixSchema.audioAttendanceRow, kind: .structure(name: "HelixAudioAttendanceRow", fields: [
            field("decoder_layer_index", primitiveRef(.u32)),
            field("head_index", primitiveRef(.u32)),
            field("dominant_audio_mass", primitiveRef(.f32)),
            field("total_audio_mass", primitiveRef(.f32)),
            field("center_audio_token", schemaRef(HelixSchema.optionF32)),
            field("width_audio_tokens", schemaRef(HelixSchema.optionF32)),
            field("observed_audio", schemaRef(HelixSchema.audioTokenRange)),
            field("dominant_audio", schemaRef(HelixSchema.audioTokenRange)),
            field("audio_weights", schemaRef(HelixSchema.f32List)),
        ])),
        Schema(id: HelixSchema.audioAttendanceRowList, kind: .list(element: schemaRef(HelixSchema.audioAttendanceRow))),
        Schema(id: HelixSchema.refreshAttendanceRow, kind: .structure(name: "HelixRefreshAttendanceRow", fields: [
            field("query_position", primitiveRef(.u32)),
            field("decoder_layer_index", primitiveRef(.u32)),
            field("head_index", primitiveRef(.u32)),
            field("dominant_audio_mass", primitiveRef(.f32)),
            field("total_audio_mass", primitiveRef(.f32)),
            field("center_audio_token", schemaRef(HelixSchema.optionF32)),
            field("width_audio_tokens", schemaRef(HelixSchema.optionF32)),
            field("observed_audio", schemaRef(HelixSchema.audioTokenRange)),
            field("dominant_audio", schemaRef(HelixSchema.audioTokenRange)),
            field("audio_weights", schemaRef(HelixSchema.f32List)),
        ])),
        Schema(id: HelixSchema.refreshAttendanceRowList, kind: .list(element: schemaRef(HelixSchema.refreshAttendanceRow))),
        Schema(id: HelixSchema.audioSelfAttentionRow, kind: .structure(name: "HelixAudioSelfAttentionRow", fields: [
            field("encoder_layer_index", primitiveRef(.u32)),
            field("head_index", primitiveRef(.u32)),
            field("audio_representation_version", primitiveRef(.u32)),
            field("dominant_audio_mass", primitiveRef(.f32)),
            field("total_audio_mass", primitiveRef(.f32)),
            field("center_audio_token", schemaRef(HelixSchema.optionF32)),
            field("width_audio_tokens", schemaRef(HelixSchema.optionF32)),
            field("observed_audio", schemaRef(HelixSchema.audioTokenRange)),
            field("dominant_audio", schemaRef(HelixSchema.audioTokenRange)),
            field("frontier_debt", primitiveRef(.f32)),
        ])),
        Schema(id: HelixSchema.audioSelfAttentionRowList, kind: .list(element: schemaRef(HelixSchema.audioSelfAttentionRow))),
        Schema(id: HelixSchema.transcriptToken, kind: .structure(name: "HelixTranscriptToken", fields: [
            field("text_token_id", primitiveRef(.u32)),
            field("decoded_in_pulse", primitiveRef(.u64)),
            field("text", primitiveRef(.string)),
            field("committed", primitiveRef(.bool)),
        ])),
        Schema(id: HelixSchema.transcriptTokenList, kind: .list(element: schemaRef(HelixSchema.transcriptToken))),
        Schema(id: HelixSchema.audioClip, kind: .structure(name: "HelixAudioClip", fields: [
            field("sample_rate", primitiveRef(.u32)),
            field("first_sample", primitiveRef(.u64)),
            field("samples", schemaRef(HelixSchema.f32List)),
        ])),
        Schema(id: HelixSchema.optionAudioClip, kind: .option(element: schemaRef(HelixSchema.audioClip))),
        Schema(id: HelixSchema.melClip, kind: .structure(name: "HelixMelClip", fields: [
            field("num_mel_bins", primitiveRef(.u32)),
            field("first_mel_frame", primitiveRef(.u32)),
            field("num_mel_frames", primitiveRef(.u32)),
            field("values", schemaRef(HelixSchema.f32List)),
            field("min_value", primitiveRef(.f32)),
            field("max_value", primitiveRef(.f32)),
            field("corpus_min_value", primitiveRef(.f32)),
            field("corpus_max_value", primitiveRef(.f32)),
        ])),
        Schema(id: HelixSchema.optionMelClip, kind: .option(element: schemaRef(HelixSchema.melClip))),
        Schema(id: HelixSchema.supportSummary, kind: .structure(name: "HelixAttentionSupportSummary", fields: [
            field("total_audio_mass", primitiveRef(.f32)),
            field("observed_audio", schemaRef(HelixSchema.audioTokenRange)),
            field("dominant_audio", schemaRef(HelixSchema.audioTokenRange)),
            field("dominant_audio_mass", primitiveRef(.f32)),
            field("center_audio_token", schemaRef(HelixSchema.optionF32)),
            field("width_audio_tokens", schemaRef(HelixSchema.optionF32)),
        ])),
        Schema(id: HelixSchema.textSupportRecord, kind: .structure(name: "HelixTextAttentionSupportRecord", fields: [
            field("text_token_id", primitiveRef(.u32)),
            field("query_position", primitiveRef(.u32)),
            field("decoder_layer_index", primitiveRef(.u32)),
            field("head_index", primitiveRef(.u32)),
            field("support", schemaRef(HelixSchema.supportSummary)),
            field("audio_weights", schemaRef(HelixSchema.f32List)),
        ])),
        Schema(id: HelixSchema.textSupportRecordList, kind: .list(element: schemaRef(HelixSchema.textSupportRecord))),
        Schema(id: HelixSchema.audioEncoderSupportRecord, kind: .structure(name: "HelixAudioEncoderSupportRecord", fields: [
            field("audio_token_id", primitiveRef(.u32)),
            field("audio_representation_version", primitiveRef(.u32)),
            field("encoder_layer_index", primitiveRef(.u32)),
            field("head_index", primitiveRef(.u32)),
            field("support", schemaRef(HelixSchema.supportSummary)),
            field("frontier_debt", primitiveRef(.f32)),
        ])),
        Schema(id: HelixSchema.audioEncoderSupportRecordList, kind: .list(element: schemaRef(HelixSchema.audioEncoderSupportRecord))),
        Schema(id: HelixSchema.decoderEvidenceKind, kind: .enumeration(name: "HelixDecoderEvidenceKind", variants: [
            Variant(name: "Decode", index: 0, payload: .structure([
                field("input_token_id", primitiveRef(.u32)),
            ])),
            Variant(name: "VerifyPrediction", index: 1, payload: .structure([
                field("verified_draft_index", primitiveRef(.u32)),
                field("draft_token_id", primitiveRef(.u32)),
                field("query_row", primitiveRef(.u32)),
                field("max_logit", primitiveRef(.f32)),
                field("draft_logit", primitiveRef(.f32)),
            ])),
            Variant(name: "VerifySeed", index: 2, payload: .structure([
                field("query_row", primitiveRef(.u32)),
                field("next_token_seed", primitiveRef(.u32)),
                field("max_logit", primitiveRef(.f32)),
            ])),
            Variant(name: "PromptPrefill", index: 3, payload: .unit),
        ])),
        Schema(id: HelixSchema.decoderEvidenceRecord, kind: .structure(name: "HelixDecoderEvidenceRecord", fields: [
            field("text_token_id", schemaRef(HelixSchema.optionTextTokenId)),
            field("query_position", primitiveRef(.u32)),
            field("expected_observed_audio", schemaRef(HelixSchema.audioTokenRange)),
            field("records", schemaRef(HelixSchema.textSupportRecordList)),
            field("kind", schemaRef(HelixSchema.decoderEvidenceKind)),
        ])),
        Schema(id: HelixSchema.decoderEvidenceRecordList, kind: .list(element: schemaRef(HelixSchema.decoderEvidenceRecord))),
        Schema(id: HelixSchema.queryRowAttentionRecord, kind: .structure(name: "HelixQueryRowAttentionRecord", fields: [
            field("query_position", primitiveRef(.u32)),
            field("decoder_layer_index", primitiveRef(.u32)),
            field("head_index", primitiveRef(.u32)),
            field("support", schemaRef(HelixSchema.supportSummary)),
            field("audio_weights", schemaRef(HelixSchema.f32List)),
        ])),
        Schema(id: HelixSchema.queryRowAttentionRecordList, kind: .list(element: schemaRef(HelixSchema.queryRowAttentionRecord))),
        Schema(id: HelixSchema.attentionSummaryBatch, kind: .structure(name: "HelixAttentionSummaryBatch", fields: [
            field("schema_version", primitiveRef(.u32)),
            field("pulse_id", primitiveRef(.u64)),
            field("audio_context_id", primitiveRef(.u64)),
            field("text_context_id", primitiveRef(.u64)),
            field("audio_representation_spans", schemaRef(HelixSchema.audioRepresentationSpanList)),
            field("changed_audio_representation_spans", schemaRef(HelixSchema.audioRepresentationSpanList)),
            field("text_support", schemaRef(HelixSchema.textSupportRecordList)),
            field("header_text_support", schemaRef(HelixSchema.queryRowAttentionRecordList)),
            field("audio_encoder_support", schemaRef(HelixSchema.audioEncoderSupportRecordList)),
            field("decoder_evidence", schemaRef(HelixSchema.decoderEvidenceRecordList)),
        ])),
        Schema(id: HelixSchema.optionAttentionSummaryBatch, kind: .option(element: schemaRef(HelixSchema.attentionSummaryBatch))),
        Schema(id: HelixSchema.verifyDraftStatus, kind: .enumeration(name: "HelixVerifyDraftStatus", variants: [
            Variant(name: "Accepted", index: 0, payload: .unit),
            Variant(name: "Divergent", index: 1, payload: .unit),
            Variant(name: "DiscardedAfterDivergence", index: 2, payload: .unit),
        ])),
        Schema(id: HelixSchema.verifyDraftRow, kind: .structure(name: "HelixVerifyDraftRow", fields: [
            field("draft_index", primitiveRef(.u32)),
            field("draft_token_id", primitiveRef(.u32)),
            field("verified_text_token_id", primitiveRef(.u32)),
            field("text", primitiveRef(.string)),
            field("status", schemaRef(HelixSchema.verifyDraftStatus)),
            field("expected_observed_audio", schemaRef(HelixSchema.audioTokenRange)),
            field("max_dominant_audio_mass", primitiveRef(.f32)),
            field("record_count", primitiveRef(.u32)),
            field("max_logit", primitiveRef(.f32)),
            field("draft_logit", primitiveRef(.f32)),
        ])),
        Schema(id: HelixSchema.verifyDraftRowList, kind: .list(element: schemaRef(HelixSchema.verifyDraftRow))),
        Schema(id: HelixSchema.verifySeedRow, kind: .structure(name: "HelixVerifySeedRow", fields: [
            field("query_row", primitiveRef(.u32)),
            field("next_token_seed", primitiveRef(.u32)),
            field("expected_observed_audio", schemaRef(HelixSchema.audioTokenRange)),
            field("max_dominant_audio_mass", primitiveRef(.f32)),
            field("record_count", primitiveRef(.u32)),
            field("max_logit", primitiveRef(.f32)),
        ])),
        Schema(id: HelixSchema.optionVerifySeedRow, kind: .option(element: schemaRef(HelixSchema.verifySeedRow))),
        Schema(id: HelixSchema.verifyEvidenceDigest, kind: .structure(name: "HelixVerifyEvidenceDigest", fields: [
            field("pulse_id", primitiveRef(.u64)),
            field("rewind_k", primitiveRef(.u64)),
            field("accepted_prefix_len", schemaRef(HelixSchema.optionU64)),
            field("divergence_row", schemaRef(HelixSchema.optionU64)),
            field("drafts", schemaRef(HelixSchema.verifyDraftRowList)),
            field("seed", schemaRef(HelixSchema.optionVerifySeedRow)),
        ])),
        Schema(id: HelixSchema.optionVerifyEvidenceDigest, kind: .option(element: schemaRef(HelixSchema.verifyEvidenceDigest))),
        Schema(id: HelixSchema.decodeFact, kind: .structure(name: "HelixDecodeFact", fields: [
            field("text_token_id", primitiveRef(.u32)),
            field("query_position", primitiveRef(.u32)),
            field("input_token_id", primitiveRef(.u32)),
            field("observed_audio", schemaRef(HelixSchema.audioTokenRange)),
        ])),
        Schema(id: HelixSchema.decodeFactList, kind: .list(element: schemaRef(HelixSchema.decodeFact))),
        Schema(id: HelixSchema.verifyPredictionFact, kind: .structure(name: "HelixVerifyPredictionFact", fields: [
            field("verified_text_token_id", primitiveRef(.u32)),
            field("verified_draft_index", primitiveRef(.u32)),
            field("draft_token_id", primitiveRef(.u32)),
            field("query_row", primitiveRef(.u32)),
            field("query_position", primitiveRef(.u32)),
            field("observed_audio", schemaRef(HelixSchema.audioTokenRange)),
        ])),
        Schema(id: HelixSchema.verifyPredictionFactList, kind: .list(element: schemaRef(HelixSchema.verifyPredictionFact))),
        Schema(id: HelixSchema.verifySeedFact, kind: .structure(name: "HelixVerifySeedFact", fields: [
            field("query_row", primitiveRef(.u32)),
            field("query_position", primitiveRef(.u32)),
            field("next_token_seed", primitiveRef(.u32)),
            field("observed_audio", schemaRef(HelixSchema.audioTokenRange)),
        ])),
        Schema(id: HelixSchema.verifySeedFactList, kind: .list(element: schemaRef(HelixSchema.verifySeedFact))),
        Schema(id: HelixSchema.promptPrefillFact, kind: .structure(name: "HelixPromptPrefillFact", fields: [
            field("query_position", primitiveRef(.u32)),
            field("observed_audio", schemaRef(HelixSchema.audioTokenRange)),
        ])),
        Schema(id: HelixSchema.promptPrefillFactList, kind: .list(element: schemaRef(HelixSchema.promptPrefillFact))),
        Schema(id: HelixSchema.factCounts, kind: .structure(name: "HelixDecoderEvidenceFactCounts", fields: [
            field("decode", primitiveRef(.u32)),
            field("verify_prediction", primitiveRef(.u32)),
            field("verify_seed", primitiveRef(.u32)),
            field("prompt_prefill", primitiveRef(.u32)),
        ])),
        Schema(id: HelixSchema.encoderFactsSnapshot, kind: .structure(name: "HelixEncoderFactsSnapshot", fields: [
            field("refreshed_audio", schemaRef(HelixSchema.audioTokenRange)),
            field("audio_representation_version", primitiveRef(.u32)),
            field("provenance", schemaRef(HelixSchema.audioTokenProvenanceList)),
        ])),
        Schema(id: HelixSchema.optionEncoderFactsSnapshot, kind: .option(element: schemaRef(HelixSchema.encoderFactsSnapshot))),
        Schema(id: HelixSchema.pulseEvidenceSnapshot, kind: .structure(name: "HelixPulseEvidenceSnapshot", fields: [
            field("pulse_id", primitiveRef(.u64)),
            field("encoder", schemaRef(HelixSchema.optionEncoderFactsSnapshot)),
            field("counts", schemaRef(HelixSchema.factCounts)),
            field("decode", schemaRef(HelixSchema.decodeFactList)),
            field("verify_prediction", schemaRef(HelixSchema.verifyPredictionFactList)),
            field("verify_seed", schemaRef(HelixSchema.verifySeedFactList)),
            field("prompt_prefill", schemaRef(HelixSchema.promptPrefillFactList)),
        ])),
        Schema(id: HelixSchema.optionPulseEvidenceSnapshot, kind: .option(element: schemaRef(HelixSchema.pulseEvidenceSnapshot))),
        Schema(id: HelixSchema.provenanceViolationKind, kind: .enumeration(name: "HelixEncoderProvenanceViolationKind", variants: [
            Variant(name: "MissingProvenance", index: 0, payload: .unit),
            Variant(name: "VersionMismatch", index: 1, payload: .unit),
            Variant(name: "EmptyMelFrames", index: 2, payload: .unit),
            Variant(name: "NonFiniteFrontierDebt", index: 3, payload: .unit),
        ])),
        Schema(id: HelixSchema.provenanceViolation, kind: .structure(name: "HelixEncoderProvenanceViolation", fields: [
            field("audio_token_id", primitiveRef(.u32)),
            field("encoder_layer_index", primitiveRef(.u32)),
            field("head_index", primitiveRef(.u32)),
            field("observed_audio_token_id", schemaRef(HelixSchema.optionAudioTokenId)),
            field("kind", schemaRef(HelixSchema.provenanceViolationKind)),
            field("message", primitiveRef(.string)),
        ])),
        Schema(id: HelixSchema.provenanceViolationList, kind: .list(element: schemaRef(HelixSchema.provenanceViolation))),
        Schema(id: HelixSchema.encoderProvenanceReport, kind: .structure(name: "HelixEncoderProvenanceReport", fields: [
            field("pulse_id", primitiveRef(.u64)),
            field("records_checked", primitiveRef(.u64)),
            field("violations", schemaRef(HelixSchema.provenanceViolationList)),
        ])),
        Schema(id: HelixSchema.optionEncoderProvenanceReport, kind: .option(element: schemaRef(HelixSchema.encoderProvenanceReport))),
        Schema(id: HelixSchema.variantCounts, kind: .structure(name: "HelixDecoderEvidenceVariantCounts", fields: [
            field("decode", primitiveRef(.u64)),
            field("verify_prediction", primitiveRef(.u64)),
            field("verify_seed", primitiveRef(.u64)),
            field("prompt_prefill", primitiveRef(.u64)),
        ])),
        Schema(id: HelixSchema.decoderEvidenceReport, kind: .structure(name: "HelixDecoderEvidenceReport", fields: [
            field("total_batches", primitiveRef(.u64)),
            field("batches_without_decoder_evidence", primitiveRef(.u64)),
            field("pulses_without_decoder_evidence", schemaRef(HelixSchema.u64List)),
            field("variant_evidence_counts", schemaRef(HelixSchema.variantCounts)),
            field("variant_record_counts", schemaRef(HelixSchema.variantCounts)),
            field("observed_decoder_layer_indices", schemaRef(HelixSchema.u32List)),
            field("observed_decoder_head_indices", schemaRef(HelixSchema.u32List)),
        ])),
        Schema(id: HelixSchema.frontierPoint, kind: .structure(name: "HelixEncoderFrontierPoint", fields: [
            field("audio_token_id", primitiveRef(.u32)),
            field("mean_frontier_debt", primitiveRef(.f32)),
            field("head_count", primitiveRef(.u32)),
        ])),
        Schema(id: HelixSchema.frontierPointList, kind: .list(element: schemaRef(HelixSchema.frontierPoint))),
        Schema(id: HelixSchema.frontierLayer, kind: .structure(name: "HelixEncoderFrontierLayer", fields: [
            field("encoder_layer_index", primitiveRef(.u32)),
            field("points", schemaRef(HelixSchema.frontierPointList)),
        ])),
        Schema(id: HelixSchema.frontierLayerList, kind: .list(element: schemaRef(HelixSchema.frontierLayer))),
        Schema(id: HelixSchema.frontierSeries, kind: .structure(name: "HelixEncoderFrontierSeries", fields: [
            field("pulse_id", primitiveRef(.u64)),
            field("layers", schemaRef(HelixSchema.frontierLayerList)),
            field("min_audio_token_id", primitiveRef(.u32)),
            field("max_audio_token_id", primitiveRef(.u32)),
            field("min_frontier_debt", primitiveRef(.f32)),
            field("max_frontier_debt", primitiveRef(.f32)),
        ])),
        Schema(id: HelixSchema.optionFrontierSeries, kind: .option(element: schemaRef(HelixSchema.frontierSeries))),
        Schema(id: HelixSchema.tracePositionSpan, kind: .structure(name: "HelixTracePositionSpan", fields: [
            field("logical_start", primitiveRef(.u64)),
            field("rows", primitiveRef(.u64)),
            field("physical_start", primitiveRef(.u64)),
        ])),
        Schema(id: HelixSchema.tracePositionSpanList, kind: .list(element: schemaRef(HelixSchema.tracePositionSpan))),
        Schema(id: HelixSchema.arDecodeEarlyExitReason, kind: .enumeration(name: "HelixArDecodeEarlyExitReason", variants: [
            Variant(name: "BudgetExhausted", index: 0, payload: .unit),
            Variant(name: "NoBudget", index: 1, payload: .unit),
            Variant(name: "SeedWasEos", index: 2, payload: .unit),
            Variant(name: "ProducedEos", index: 3, payload: .unit),
        ])),
        Schema(id: HelixSchema.verifySkippedReason, kind: .enumeration(name: "HelixVerifySkippedReason", variants: [
            Variant(name: "RewindGuardFailed", index: 0, payload: .unit),
            Variant(name: "PreCommitFullRewind", index: 1, payload: .unit),
        ])),
        Schema(id: HelixSchema.streamingTraceEvent, kind: .enumeration(name: "HelixStreamingTraceEvent", variants: [
            Variant(name: "Pulse", index: 0, payload: .structure(helixPulseTraceFields())),
            Variant(name: "RefreshPrompt", index: 1, payload: .structure(helixRefreshPromptTraceFields())),
            Variant(name: "Verify", index: 2, payload: .structure(helixVerifyTraceFields())),
            Variant(name: "ArDecode", index: 3, payload: .structure(helixArDecodeTraceFields())),
            Variant(name: "ArToken", index: 4, payload: .structure(helixArTokenTraceFields())),
            Variant(name: "Commit", index: 5, payload: .structure(helixCommitTraceFields())),
            Variant(name: "VerifySkipped", index: 6, payload: .structure(helixVerifySkippedTraceFields())),
        ])),
        Schema(id: HelixSchema.streamingTraceEventList, kind: .list(element: schemaRef(HelixSchema.streamingTraceEvent))),
        Schema(id: HelixSchema.optionStreamingTraceEventList, kind: .option(element: schemaRef(HelixSchema.streamingTraceEventList))),
        Schema(id: HelixSchema.chromeTraceEvent, kind: .structure(name: "HelixChromeTraceEvent", fields: [
            field("name", primitiveRef(.string)),
            field("cat", primitiveRef(.string)),
            field("ph", primitiveRef(.string)),
            field("ts", primitiveRef(.f64)),
            field("dur", schemaRef(HelixSchema.optionF64)),
            field("pid", primitiveRef(.u32)),
            field("tid", primitiveRef(.u32)),
            field("s", schemaRef(HelixSchema.optionString)),
            field("args", schemaRef(HelixSchema.mapStringDynamic)),
        ])),
        Schema(id: HelixSchema.chromeTraceEventList, kind: .list(element: schemaRef(HelixSchema.chromeTraceEvent))),
        Schema(id: HelixSchema.optionChromeTraceEventList, kind: .option(element: schemaRef(HelixSchema.chromeTraceEventList))),
        Schema(id: HelixSchema.pulseBundleFields, kind: .structure(name: "HelixPulseBundleFields", fields: [
            field("prompt_layout", primitiveRef(.bool)),
            field("audio_provenance", primitiveRef(.bool)),
            field("attention_heatmap", primitiveRef(.bool)),
            field("encoder_frontier", primitiveRef(.bool)),
            field("encoder_provenance", primitiveRef(.bool)),
            field("audio_clip", primitiveRef(.bool)),
            field("mel_clip", primitiveRef(.bool)),
            field("pulse_rollup", primitiveRef(.bool)),
            field("timeline", primitiveRef(.bool)),
            field("gpu_chrome_events", primitiveRef(.bool)),
            field("verify_evidence", primitiveRef(.bool)),
            field("scheduler_snapshot", primitiveRef(.bool)),
        ])),
        Schema(id: HelixSchema.pulseBundle, kind: .structure(name: "HelixPulseBundle", fields: [
            field("pulse_id", primitiveRef(.u64)),
            field("schema_version", primitiveRef(.u32)),
            field("prompt_layout", schemaRef(HelixSchema.optionPromptLayout)),
            field("audio_provenance", schemaRef(HelixSchema.optionAudioTokenProvenanceList)),
            field("attention_heatmap", schemaRef(HelixSchema.optionAttentionHeatmap)),
            field("encoder_frontier", schemaRef(HelixSchema.optionFrontierSeries)),
            field("encoder_provenance", schemaRef(HelixSchema.optionEncoderProvenanceReport)),
            field("audio_clip", schemaRef(HelixSchema.optionAudioClip)),
            field("mel_clip", schemaRef(HelixSchema.optionMelClip)),
            field("pulse_rollup", schemaRef(HelixSchema.optionPulseRollup)),
            field("timeline", schemaRef(HelixSchema.optionStreamingTraceEventList)),
            field("gpu_chrome_events", schemaRef(HelixSchema.optionChromeTraceEventList)),
            field("verify_evidence", schemaRef(HelixSchema.optionVerifyEvidenceDigest)),
            field("scheduler_snapshot", schemaRef(HelixSchema.optionPulseEvidenceSnapshot)),
        ])),
        Schema(id: HelixSchema.pieceEvalSnapshot, kind: .structure(name: "HelixPieceEvalSnapshot", fields: [
            field("audio_now_ms", primitiveRef(.f64)),
            field("reference_words_available", primitiveRef(.u32)),
            field("hypothesis_words", primitiveRef(.u32)),
            field("substitutions", primitiveRef(.u32)),
            field("deletions", primitiveRef(.u32)),
            field("insertions", primitiveRef(.u32)),
            field("rolling_wer", primitiveRef(.f64)),
            field("s2d_matched_words", primitiveRef(.u32)),
            field("s2d_new_words", primitiveRef(.u32)),
            field("s2d_p50_ms", schemaRef(HelixSchema.optionF64)),
            field("s2d_p90_ms", schemaRef(HelixSchema.optionF64)),
            field("s2d_p100_ms", schemaRef(HelixSchema.optionF64)),
            field("s2d_avg_ms", schemaRef(HelixSchema.optionF64)),
            field("audio_frontier", primitiveRef(.u32)),
            field("displayed_frontier", primitiveRef(.u32)),
            field("committed_frontier", primitiveRef(.u32)),
            field("lag_ms", primitiveRef(.f64)),
        ])),
        Schema(id: HelixSchema.optionPieceEvalSnapshot, kind: .option(element: schemaRef(HelixSchema.pieceEvalSnapshot))),
        Schema(id: HelixSchema.pieceEvalReference, kind: .structure(name: "HelixPieceEvalReference", fields: [
            field("piece", primitiveRef(.string)),
            field("language", primitiveRef(.string)),
            field("words", schemaRef(HelixSchema.stringList)),
        ])),
        Schema(id: HelixSchema.optionPieceEvalReference, kind: .option(element: schemaRef(HelixSchema.pieceEvalReference))),
        Schema(id: HelixSchema.traceServiceSurface, kind: .structure(name: "HelixTraceServiceSurface", fields: [
            field("meta", schemaRef(HelixSchema.streamMeta)),
            field("pulse_rollup", schemaRef(HelixSchema.optionPulseRollup)),
            field("timeline", schemaRef(HelixSchema.streamingTraceEventList)),
            field("attention_batch", schemaRef(HelixSchema.optionAttentionSummaryBatch)),
            field("prompt_layout", schemaRef(HelixSchema.optionPromptLayout)),
            field("audio_attended_by", schemaRef(HelixSchema.textAttendanceRowList)),
            field("text_attends_to", schemaRef(HelixSchema.audioAttendanceRowList)),
            field("refresh_attends_to", schemaRef(HelixSchema.refreshAttendanceRowList)),
            field("audio_token_provenance", schemaRef(HelixSchema.optionAudioTokenProvenance)),
            field("audio_provenance_for_pulse", schemaRef(HelixSchema.audioTokenProvenanceList)),
            field("audio_tokens_for_mel_frame", schemaRef(HelixSchema.u32List)),
            field("audio_clip_for_audio_token", schemaRef(HelixSchema.optionAudioClip)),
            field("audio_clip_for_prompt", schemaRef(HelixSchema.optionAudioClip)),
            field("audio_clip_for_audio_range", schemaRef(HelixSchema.optionAudioClip)),
            field("mel_clip_for_prompt", schemaRef(HelixSchema.optionMelClip)),
            field("audio_self_attention", schemaRef(HelixSchema.audioSelfAttentionRowList)),
            field("transcript", schemaRef(HelixSchema.transcriptTokenList)),
            field("pulse_attention_heatmap", schemaRef(HelixSchema.optionAttentionHeatmap)),
            field("encoder_frontier", schemaRef(HelixSchema.optionFrontierSeries)),
            field("stream_metrics", schemaRef(HelixSchema.streamMetrics)),
            field("verify_evidence", schemaRef(HelixSchema.optionVerifyEvidenceDigest)),
            field("decoder_evidence_report", schemaRef(HelixSchema.decoderEvidenceReport)),
            field("pulse_evidence_snapshot", schemaRef(HelixSchema.optionPulseEvidenceSnapshot)),
            field("gpu_chrome_events_for_pulse", schemaRef(HelixSchema.chromeTraceEventList)),
            field("run_info", schemaRef(HelixSchema.optionRunInfo)),
            field("piece_eval_reference", schemaRef(HelixSchema.optionPieceEvalReference)),
            field("piece_eval_for_pulse", schemaRef(HelixSchema.optionPieceEvalSnapshot)),
            field("encoder_provenance_report", schemaRef(HelixSchema.optionEncoderProvenanceReport)),
            field("pulse_bundle_fields", schemaRef(HelixSchema.pulseBundleFields)),
            field("pulse_bundle", schemaRef(HelixSchema.pulseBundle)),
            field("pulse_available", schemaRef(HelixSchema.pulseAvailable)),
        ])),
    ]
}

private func helixPulseTraceFields() -> [Field] {
    [
        field("start_us", primitiveRef(.u64)),
        field("duration_us", primitiveRef(.u64)),
        field("pulse_id", primitiveRef(.u64)),
        field("previous_consumed_mel_frames", primitiveRef(.u64)),
        field("consumed_mel_frames", primitiveRef(.u64)),
        field("pulse_mel_frames", primitiveRef(.u64)),
        field("committed_text_len_start", primitiveRef(.u64)),
        field("speculative_len_start", primitiveRef(.u64)),
        field("committed_tokens", primitiveRef(.u64)),
        field("retained_speculative_tokens", primitiveRef(.u64)),
        field("resident_committed_tokens", primitiveRef(.u64)),
        field("evicted_audio_tokens", primitiveRef(.u64)),
        field("evicted_committed_tokens", primitiveRef(.u64)),
    ]
}

private func helixRefreshPromptTraceFields() -> [Field] {
    [
        field("start_us", primitiveRef(.u64)),
        field("duration_us", primitiveRef(.u64)),
        field("pulse_id", primitiveRef(.u64)),
        field("first_audio_token_id", primitiveRef(.u64)),
        field("resident_audio_frames", primitiveRef(.u64)),
        field("committed_text_len", primitiveRef(.u64)),
        field("resident_committed_len", primitiveRef(.u64)),
        field("resident_text_len", primitiveRef(.u64)),
        field("logical_start", primitiveRef(.u64)),
        field("logical_end", primitiveRef(.u64)),
        field("text_token_start", primitiveRef(.u64)),
        field("text_token_end", primitiveRef(.u64)),
        field("spans", schemaRef(HelixSchema.tracePositionSpanList)),
    ]
}

private func helixVerifyTraceFields() -> [Field] {
    [
        field("start_us", primitiveRef(.u64)),
        field("duration_us", primitiveRef(.u64)),
        field("pulse_id", primitiveRef(.u64)),
        field("rewind_k", primitiveRef(.u64)),
        field("post_rewind_text_len", primitiveRef(.u64)),
        field("text_token_start", primitiveRef(.u64)),
        field("text_token_end", primitiveRef(.u64)),
        field("logical_start", primitiveRef(.u64)),
        field("logical_end", primitiveRef(.u64)),
        field("spans", schemaRef(HelixSchema.tracePositionSpanList)),
        field("accepted_prefix_len", schemaRef(HelixSchema.optionU64)),
        field("divergence_row", schemaRef(HelixSchema.optionU64)),
        field("next_token_seed", schemaRef(HelixSchema.optionU64)),
        field("discarded_speculative_tokens", schemaRef(HelixSchema.optionU64)),
        field("invalidated_speculative_slots", schemaRef(HelixSchema.optionU64)),
    ]
}

private func helixArDecodeTraceFields() -> [Field] {
    [
        field("start_us", primitiveRef(.u64)),
        field("duration_us", primitiveRef(.u64)),
        field("pulse_id", primitiveRef(.u64)),
        field("decode_steps", primitiveRef(.u64)),
        field("decoded_tokens", primitiveRef(.u64)),
        field("speculative_len_entering", primitiveRef(.u64)),
        field("live_speculative_tokens", primitiveRef(.u64)),
        field("hit_eos", primitiveRef(.bool)),
        field("seed_token_id", primitiveRef(.u64)),
        field("seed_token_text", primitiveRef(.string)),
        field("early_exit_reason", schemaRef(HelixSchema.arDecodeEarlyExitReason)),
        field("next_after_tail", primitiveRef(.u64)),
    ]
}

private func helixArTokenTraceFields() -> [Field] {
    [
        field("start_us", primitiveRef(.u64)),
        field("duration_us", primitiveRef(.u64)),
        field("pulse_id", primitiveRef(.u64)),
        field("step_index", primitiveRef(.u64)),
        field("input_token_id", primitiveRef(.u64)),
        field("input_text", primitiveRef(.string)),
        field("text_token_id", primitiveRef(.u64)),
        field("query_position", primitiveRef(.u64)),
        field("physical_start", primitiveRef(.u64)),
        field("summary_records", primitiveRef(.u64)),
        field("next_token_id", primitiveRef(.u64)),
        field("next_text", primitiveRef(.string)),
    ]
}

private func helixCommitTraceFields() -> [Field] {
    [
        field("start_us", primitiveRef(.u64)),
        field("duration_us", primitiveRef(.u64)),
        field("pulse_id", primitiveRef(.u64)),
        field("speculative_len_pre", primitiveRef(.u64)),
        field("revisable_tail_target", primitiveRef(.u64)),
        field("committed_tokens", primitiveRef(.u64)),
        field("retained_speculative_tokens", primitiveRef(.u64)),
        field("committed_text_len", primitiveRef(.u64)),
        field("next_after_committed", primitiveRef(.u64)),
    ]
}

private func helixVerifySkippedTraceFields() -> [Field] {
    [
        field("timestamp_us", primitiveRef(.u64)),
        field("pulse_id", primitiveRef(.u64)),
        field("reason", schemaRef(HelixSchema.verifySkippedReason)),
        field("rewind_k", primitiveRef(.u64)),
        field("resident_committed_len", primitiveRef(.u64)),
        field("speculative_len", primitiveRef(.u64)),
    ]
}

private func scalarDesc(_ p: Primitive) -> Descriptor {
    let size = helixFixedSize(p)!
    return Descriptor(
        schema: .concrete(primitiveId(p)),
        layout: Layout(size: size, align: helixAlignment(p)),
        access: .scalar
    )
}

private func helixFixedSize(_ p: Primitive) -> Int? {
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

private func helixAlignment(_ p: Primitive) -> Int {
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

private func dynamicDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(HelixSchema.dynamic),
        layout: MemoryLayout<Value>.phonLayout,
        access: .dynamic
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

private func stringValueMapDesc() -> Descriptor {
    Descriptor(
        schema: .concrete(HelixSchema.mapStringDynamic),
        layout: MemoryLayout<[String: Value]>.phonLayout,
        access: .map(MapAccess(
            key: stringDesc(),
            value: dynamicDesc(),
            keyStride: MemoryLayout<String>.stride,
            keyAlign: MemoryLayout<String>.alignment,
            valueStride: MemoryLayout<Value>.stride,
            valueAlign: MemoryLayout<Value>.alignment,
            witness: .stringKeyed(Value.self)
        ))
    )
}

private func optionU64Desc() -> Descriptor {
    optionDesc(HelixSchema.optionU64, UInt64.self, some: scalarDesc(.u64))
}

private func optionStringDesc() -> Descriptor {
    optionDesc(HelixSchema.optionString, String.self, some: stringDesc())
}

private func optionF32Desc() -> Descriptor {
    optionDesc(HelixSchema.optionF32, Float.self, some: scalarDesc(.f32))
}

private func optionF64Desc() -> Descriptor {
    optionDesc(HelixSchema.optionF64, Double.self, some: scalarDesc(.f64))
}

private func optionTextTokenIdDesc() -> Descriptor {
    optionDesc(HelixSchema.optionTextTokenId, UInt32.self, some: scalarDesc(.u32))
}

private func optionAudioTokenIdDesc() -> Descriptor {
    optionDesc(HelixSchema.optionAudioTokenId, UInt32.self, some: scalarDesc(.u32))
}

private func u32ListDesc() -> Descriptor {
    listDesc(HelixSchema.u32List, UInt32.self, element: scalarDesc(.u32))
}

private func u64ListDesc() -> Descriptor {
    listDesc(HelixSchema.u64List, UInt64.self, element: scalarDesc(.u64))
}

private func f32ListDesc() -> Descriptor {
    listDesc(HelixSchema.f32List, Float.self, element: scalarDesc(.f32))
}

private func f64ListDesc() -> Descriptor {
    listDesc(HelixSchema.f64List, Double.self, element: scalarDesc(.f64))
}

private func stringListDesc() -> Descriptor {
    listDesc(HelixSchema.stringList, String.self, element: stringDesc())
}

private func audioTokenRangeDesc() -> Descriptor {
    recordDesc(HelixSchema.audioTokenRange, HelixAudioTokenRange.self, fields: [
        fieldAccess(\HelixAudioTokenRange.start, scalarDesc(.u32)),
        fieldAccess(\HelixAudioTokenRange.end, scalarDesc(.u32)),
    ])
}

private func audioRepresentationSpanDesc() -> Descriptor {
    recordDesc(HelixSchema.audioRepresentationSpan, HelixAudioRepresentationSpan.self, fields: [
        fieldAccess(\HelixAudioRepresentationSpan.audio, audioTokenRangeDesc()),
        fieldAccess(\HelixAudioRepresentationSpan.audioRepresentationVersion, scalarDesc(.u32)),
    ])
}

private func audioRepresentationSpanListDesc() -> Descriptor {
    listDesc(HelixSchema.audioRepresentationSpanList, HelixAudioRepresentationSpan.self, element: audioRepresentationSpanDesc())
}

private func streamMetaDesc() -> Descriptor {
    recordDesc(HelixSchema.streamMeta, HelixStreamMeta.self, fields: [
        fieldAccess(\HelixStreamMeta.schemaVersion, scalarDesc(.u32)),
        fieldAccess(\HelixStreamMeta.pulseIds, u64ListDesc()),
        fieldAccess(\HelixStreamMeta.timelineEventCount, scalarDesc(.u64)),
        fieldAccess(\HelixStreamMeta.attentionBatchCount, scalarDesc(.u64)),
    ])
}

private func verifyOutcomeDesc() -> Descriptor {
    recordDesc(HelixSchema.verifyOutcome, HelixVerifyOutcome.self, fields: [
        fieldAccess(\HelixVerifyOutcome.rewindK, scalarDesc(.u64)),
        fieldAccess(\HelixVerifyOutcome.acceptedPrefixLen, optionU64Desc()),
        fieldAccess(\HelixVerifyOutcome.divergenceRow, optionU64Desc()),
        fieldAccess(\HelixVerifyOutcome.discardedSpeculativeTokens, optionU64Desc()),
    ])
}

private func optionVerifyOutcomeDesc() -> Descriptor {
    optionDesc(HelixSchema.optionVerifyOutcome, HelixVerifyOutcome.self, some: verifyOutcomeDesc())
}

private func pulseRollupDesc() -> Descriptor {
    recordDesc(HelixSchema.pulseRollup, HelixPulseRollup.self, fields: [
        fieldAccess(\HelixPulseRollup.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.pulseStartUs, optionU64Desc()),
        fieldAccess(\HelixPulseRollup.pulseDurationUs, optionU64Desc()),
        fieldAccess(\HelixPulseRollup.encoderDurationUs, optionU64Desc()),
        fieldAccess(\HelixPulseRollup.refreshDurationUs, optionU64Desc()),
        fieldAccess(\HelixPulseRollup.verifyDurationUs, optionU64Desc()),
        fieldAccess(\HelixPulseRollup.decodeDurationUs, optionU64Desc()),
        fieldAccess(\HelixPulseRollup.commitDurationUs, optionU64Desc()),
        fieldAccess(\HelixPulseRollup.pulseMelFrames, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.committedTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.retainedSpeculativeTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.residentCommittedTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.evictedAudioTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.evictedCommittedTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.decodedTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseRollup.hitEos, scalarDesc(.bool)),
        fieldAccess(\HelixPulseRollup.verify, optionVerifyOutcomeDesc()),
        fieldAccess(\HelixPulseRollup.hasAttentionBatch, scalarDesc(.bool)),
        fieldAccess(\HelixPulseRollup.arTokenCount, scalarDesc(.u64)),
    ])
}

private func optionPulseRollupDesc() -> Descriptor {
    optionDesc(HelixSchema.optionPulseRollup, HelixPulseRollup.self, some: pulseRollupDesc())
}

private func textTokenSnapshotDesc() -> Descriptor {
    recordDesc(HelixSchema.textTokenSnapshot, HelixTextTokenSnapshot.self, fields: [
        fieldAccess(\HelixTextTokenSnapshot.textTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixTextTokenSnapshot.text, optionStringDesc()),
        fieldAccess(\HelixTextTokenSnapshot.textBefore, optionStringDesc()),
        fieldAccess(\HelixTextTokenSnapshot.inVerifyBatch, scalarDesc(.bool)),
        fieldAccess(\HelixTextTokenSnapshot.decodedThisPulse, scalarDesc(.bool)),
    ])
}

private func textTokenSnapshotListDesc() -> Descriptor {
    listDesc(HelixSchema.textTokenSnapshotList, HelixTextTokenSnapshot.self, element: textTokenSnapshotDesc())
}

private func promptLayoutDesc() -> Descriptor {
    recordDesc(HelixSchema.promptLayout, HelixPromptLayout.self, fields: [
        fieldAccess(\HelixPromptLayout.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixPromptLayout.firstAudioTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixPromptLayout.residentAudioFrames, scalarDesc(.u64)),
        fieldAccess(\HelixPromptLayout.changedAudioSpans, audioRepresentationSpanListDesc()),
        fieldAccess(\HelixPromptLayout.textTokenStart, scalarDesc(.u32)),
        fieldAccess(\HelixPromptLayout.textTokenEnd, scalarDesc(.u32)),
        fieldAccess(\HelixPromptLayout.textTokens, textTokenSnapshotListDesc()),
    ])
}

private func optionPromptLayoutDesc() -> Descriptor {
    optionDesc(HelixSchema.optionPromptLayout, HelixPromptLayout.self, some: promptLayoutDesc())
}

private func attentionHeatmapDesc() -> Descriptor {
    recordDesc(HelixSchema.attentionHeatmap, HelixPulseAttentionHeatmap.self, fields: [
        fieldAccess(\HelixPulseAttentionHeatmap.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixPulseAttentionHeatmap.firstAudioTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixPulseAttentionHeatmap.audioTokenCount, scalarDesc(.u32)),
        fieldAccess(\HelixPulseAttentionHeatmap.textTokenStart, scalarDesc(.u32)),
        fieldAccess(\HelixPulseAttentionHeatmap.textTokenCount, scalarDesc(.u32)),
        fieldAccess(\HelixPulseAttentionHeatmap.recordCount, scalarDesc(.u32)),
        fieldAccess(\HelixPulseAttentionHeatmap.maxValue, scalarDesc(.f32)),
        fieldAccess(\HelixPulseAttentionHeatmap.meanAudioMass, f32ListDesc()),
        fieldAccess(\HelixPulseAttentionHeatmap.textTokenGlyphs, stringListDesc()),
    ])
}

private func optionAttentionHeatmapDesc() -> Descriptor {
    optionDesc(HelixSchema.optionAttentionHeatmap, HelixPulseAttentionHeatmap.self, some: attentionHeatmapDesc())
}

private func streamMetricsDesc() -> Descriptor {
    recordDesc(HelixSchema.streamMetrics, HelixStreamMetrics.self, fields: [
        fieldAccess(\HelixStreamMetrics.pulseIds, u64ListDesc()),
        fieldAccess(\HelixStreamMetrics.pulseDurationUs, u64ListDesc()),
        fieldAccess(\HelixStreamMetrics.decodedTokens, u64ListDesc()),
        fieldAccess(\HelixStreamMetrics.committedTokens, u64ListDesc()),
        fieldAccess(\HelixStreamMetrics.retainedSpeculativeTokens, u64ListDesc()),
        fieldAccess(\HelixStreamMetrics.evictedAudioTokens, u64ListDesc()),
        fieldAccess(\HelixStreamMetrics.evictedCommittedTokens, u64ListDesc()),
        fieldAccess(\HelixStreamMetrics.rewindK, u64ListDesc()),
        fieldAccess(\HelixStreamMetrics.arTokenCount, u64ListDesc()),
        fieldAccess(\HelixStreamMetrics.rollingWer, f64ListDesc()),
        fieldAccess(\HelixStreamMetrics.s2dP50Ms, f64ListDesc()),
    ])
}

private func pulseAvailableDesc() -> Descriptor {
    recordDesc(HelixSchema.pulseAvailable, HelixPulseAvailable.self, fields: [
        fieldAccess(\HelixPulseAvailable.pulseId, scalarDesc(.u64)),
    ])
}

private func runInfoDesc() -> Descriptor {
    recordDesc(HelixSchema.runInfo, HelixRunInfo.self, fields: [
        fieldAccess(\HelixRunInfo.backend, stringDesc()),
        fieldAccess(\HelixRunInfo.modelDir, stringDesc()),
        fieldAccess(\HelixRunInfo.input, stringDesc()),
        fieldAccess(\HelixRunInfo.piece, optionStringDesc()),
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

private func optionRunInfoDesc() -> Descriptor {
    optionDesc(HelixSchema.optionRunInfo, HelixRunInfo.self, some: runInfoDesc())
}

private func melFrameRangeDesc() -> Descriptor {
    recordDesc(HelixSchema.melFrameRange, HelixMelFrameRange.self, fields: [
        fieldAccess(\HelixMelFrameRange.start, scalarDesc(.u32)),
        fieldAccess(\HelixMelFrameRange.end, scalarDesc(.u32)),
    ])
}

private func melFrameRangeListDesc() -> Descriptor {
    listDesc(HelixSchema.melFrameRangeList, HelixMelFrameRange.self, element: melFrameRangeDesc())
}

private func noMergePayloadDesc() -> Descriptor {
    recordDesc(HelixSchema.mergeProvenance, HelixNoMergePayload.self, fields: [
        fieldAccess(\HelixNoMergePayload.preMergeAudioTokenId, scalarDesc(.u32)),
    ])
}

private func mergedPayloadDesc() -> Descriptor {
    recordDesc(HelixSchema.mergeProvenance, HelixMergedPayload.self, fields: [
        fieldAccess(\HelixMergedPayload.preMerge, audioTokenRangeDesc()),
    ])
}

private func admitAllPayloadDesc() -> Descriptor {
    recordDesc(HelixSchema.admissionProvenance, HelixAdmitAllPayload.self, fields: [
        fieldAccess(\HelixAdmitAllPayload.admissionSegment, scalarDesc(.u32)),
    ])
}

private func mergeProvenanceDesc() -> Descriptor {
    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: HelixAudioTokenMergeProvenance.self).pointee {
        case .noMerge: return 0
        case .merged: return 1
        }
    }
    let projectPayload: (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void = { value, _, scratch in
        switch value.assumingMemoryBound(to: HelixAudioTokenMergeProvenance.self).pointee {
        case .noMerge(let payload):
            scratch.assumingMemoryBound(to: HelixNoMergePayload.self).initialize(to: payload)
        case .merged(let payload):
            scratch.assumingMemoryBound(to: HelixMergedPayload.self).initialize(to: payload)
        }
    }
    let destroyPayload: (UnsafeMutableRawPointer, Int) -> Void = { scratch, localIndex in
        if localIndex == 0 {
            scratch.assumingMemoryBound(to: HelixNoMergePayload.self).deinitialize(count: 1)
        } else {
            scratch.assumingMemoryBound(to: HelixMergedPayload.self).deinitialize(count: 1)
        }
    }
    let inject: (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void = { slot, localIndex, scratch in
        let value: HelixAudioTokenMergeProvenance
        if localIndex == 0 {
            value = .noMerge(scratch.assumingMemoryBound(to: HelixNoMergePayload.self).move())
        } else {
            value = .merged(scratch.assumingMemoryBound(to: HelixMergedPayload.self).move())
        }
        slot.assumingMemoryBound(to: HelixAudioTokenMergeProvenance.self).initialize(to: value)
    }
    return Descriptor(
        schema: .concrete(HelixSchema.mergeProvenance),
        layout: MemoryLayout<HelixAudioTokenMergeProvenance>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: projectPayload,
            destroyPayload: destroyPayload,
            inject: inject,
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [fieldAccess(\HelixNoMergePayload.preMergeAudioTokenId, scalarDesc(.u32))],
                    payloadLayout: MemoryLayout<HelixNoMergePayload>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [fieldAccess(\HelixMergedPayload.preMerge, audioTokenRangeDesc())],
                    payloadLayout: MemoryLayout<HelixMergedPayload>.phonLayout
                ),
            ]
        ))
    )
}

private func admissionProvenanceDesc() -> Descriptor {
    let tag: (UnsafeRawPointer) -> Int = { _ in 0 }
    let projectPayload: (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void = { value, _, scratch in
        switch value.assumingMemoryBound(to: HelixAudioTokenAdmissionProvenance.self).pointee {
        case .admitAll(let payload):
            scratch.assumingMemoryBound(to: HelixAdmitAllPayload.self).initialize(to: payload)
        }
    }
    let destroyPayload: (UnsafeMutableRawPointer, Int) -> Void = { scratch, _ in
        scratch.assumingMemoryBound(to: HelixAdmitAllPayload.self).deinitialize(count: 1)
    }
    let inject: (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void = { slot, _, scratch in
        slot.assumingMemoryBound(to: HelixAudioTokenAdmissionProvenance.self)
            .initialize(to: .admitAll(scratch.assumingMemoryBound(to: HelixAdmitAllPayload.self).move()))
    }
    return Descriptor(
        schema: .concrete(HelixSchema.admissionProvenance),
        layout: MemoryLayout<HelixAudioTokenAdmissionProvenance>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: projectPayload,
            destroyPayload: destroyPayload,
            inject: inject,
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [fieldAccess(\HelixAdmitAllPayload.admissionSegment, scalarDesc(.u32))],
                    payloadLayout: MemoryLayout<HelixAdmitAllPayload>.phonLayout
                ),
            ]
        ))
    )
}

private func audioTokenProvenanceDesc() -> Descriptor {
    recordDesc(HelixSchema.audioTokenProvenance, HelixAudioTokenProvenance.self, fields: [
        fieldAccess(\HelixAudioTokenProvenance.audioTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixAudioTokenProvenance.audioRepresentationVersion, scalarDesc(.u32)),
        fieldAccess(\HelixAudioTokenProvenance.melFrames, melFrameRangeListDesc()),
        fieldAccess(\HelixAudioTokenProvenance.nativeWindow, scalarDesc(.u32)),
        fieldAccess(\HelixAudioTokenProvenance.convStemChunk, scalarDesc(.u32)),
        fieldAccess(\HelixAudioTokenProvenance.postMergeAudioTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixAudioTokenProvenance.merge, mergeProvenanceDesc()),
        fieldAccess(\HelixAudioTokenProvenance.admission, admissionProvenanceDesc()),
        fieldAccess(\HelixAudioTokenProvenance.cosineToPrevious, optionF32Desc()),
    ])
}

private func audioTokenProvenanceListDesc() -> Descriptor {
    listDesc(HelixSchema.audioTokenProvenanceList, HelixAudioTokenProvenance.self, element: audioTokenProvenanceDesc())
}

private func optionAudioTokenProvenanceDesc() -> Descriptor {
    optionDesc(HelixSchema.optionAudioTokenProvenance, HelixAudioTokenProvenance.self, some: audioTokenProvenanceDesc())
}

private func optionAudioTokenProvenanceListDesc() -> Descriptor {
    optionDesc(HelixSchema.optionAudioTokenProvenanceList, [HelixAudioTokenProvenance].self, some: audioTokenProvenanceListDesc())
}

private func textAttendanceRowDesc() -> Descriptor {
    recordDesc(HelixSchema.textAttendanceRow, HelixTextAttendanceRow.self, fields: [
        fieldAccess(\HelixTextAttendanceRow.textTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixTextAttendanceRow.decoderLayerIndex, scalarDesc(.u32)),
        fieldAccess(\HelixTextAttendanceRow.headIndex, scalarDesc(.u32)),
        fieldAccess(\HelixTextAttendanceRow.dominantAudioMass, scalarDesc(.f32)),
        fieldAccess(\HelixTextAttendanceRow.totalAudioMass, scalarDesc(.f32)),
        fieldAccess(\HelixTextAttendanceRow.observedAudio, audioTokenRangeDesc()),
        fieldAccess(\HelixTextAttendanceRow.dominantAudio, audioTokenRangeDesc()),
        fieldAccess(\HelixTextAttendanceRow.audioWeights, f32ListDesc()),
        fieldAccess(\HelixTextAttendanceRow.queriedAudioWeight, scalarDesc(.f32)),
    ])
}

private func textAttendanceRowListDesc() -> Descriptor {
    listDesc(HelixSchema.textAttendanceRowList, HelixTextAttendanceRow.self, element: textAttendanceRowDesc())
}

private func audioAttendanceRowDesc() -> Descriptor {
    recordDesc(HelixSchema.audioAttendanceRow, HelixAudioAttendanceRow.self, fields: [
        fieldAccess(\HelixAudioAttendanceRow.decoderLayerIndex, scalarDesc(.u32)),
        fieldAccess(\HelixAudioAttendanceRow.headIndex, scalarDesc(.u32)),
        fieldAccess(\HelixAudioAttendanceRow.dominantAudioMass, scalarDesc(.f32)),
        fieldAccess(\HelixAudioAttendanceRow.totalAudioMass, scalarDesc(.f32)),
        fieldAccess(\HelixAudioAttendanceRow.centerAudioToken, optionF32Desc()),
        fieldAccess(\HelixAudioAttendanceRow.widthAudioTokens, optionF32Desc()),
        fieldAccess(\HelixAudioAttendanceRow.observedAudio, audioTokenRangeDesc()),
        fieldAccess(\HelixAudioAttendanceRow.dominantAudio, audioTokenRangeDesc()),
        fieldAccess(\HelixAudioAttendanceRow.audioWeights, f32ListDesc()),
    ])
}

private func audioAttendanceRowListDesc() -> Descriptor {
    listDesc(HelixSchema.audioAttendanceRowList, HelixAudioAttendanceRow.self, element: audioAttendanceRowDesc())
}

private func refreshAttendanceRowDesc() -> Descriptor {
    recordDesc(HelixSchema.refreshAttendanceRow, HelixRefreshAttendanceRow.self, fields: [
        fieldAccess(\HelixRefreshAttendanceRow.queryPosition, scalarDesc(.u32)),
        fieldAccess(\HelixRefreshAttendanceRow.decoderLayerIndex, scalarDesc(.u32)),
        fieldAccess(\HelixRefreshAttendanceRow.headIndex, scalarDesc(.u32)),
        fieldAccess(\HelixRefreshAttendanceRow.dominantAudioMass, scalarDesc(.f32)),
        fieldAccess(\HelixRefreshAttendanceRow.totalAudioMass, scalarDesc(.f32)),
        fieldAccess(\HelixRefreshAttendanceRow.centerAudioToken, optionF32Desc()),
        fieldAccess(\HelixRefreshAttendanceRow.widthAudioTokens, optionF32Desc()),
        fieldAccess(\HelixRefreshAttendanceRow.observedAudio, audioTokenRangeDesc()),
        fieldAccess(\HelixRefreshAttendanceRow.dominantAudio, audioTokenRangeDesc()),
        fieldAccess(\HelixRefreshAttendanceRow.audioWeights, f32ListDesc()),
    ])
}

private func refreshAttendanceRowListDesc() -> Descriptor {
    listDesc(HelixSchema.refreshAttendanceRowList, HelixRefreshAttendanceRow.self, element: refreshAttendanceRowDesc())
}

private func audioSelfAttentionRowDesc() -> Descriptor {
    recordDesc(HelixSchema.audioSelfAttentionRow, HelixAudioSelfAttentionRow.self, fields: [
        fieldAccess(\HelixAudioSelfAttentionRow.encoderLayerIndex, scalarDesc(.u32)),
        fieldAccess(\HelixAudioSelfAttentionRow.headIndex, scalarDesc(.u32)),
        fieldAccess(\HelixAudioSelfAttentionRow.audioRepresentationVersion, scalarDesc(.u32)),
        fieldAccess(\HelixAudioSelfAttentionRow.dominantAudioMass, scalarDesc(.f32)),
        fieldAccess(\HelixAudioSelfAttentionRow.totalAudioMass, scalarDesc(.f32)),
        fieldAccess(\HelixAudioSelfAttentionRow.centerAudioToken, optionF32Desc()),
        fieldAccess(\HelixAudioSelfAttentionRow.widthAudioTokens, optionF32Desc()),
        fieldAccess(\HelixAudioSelfAttentionRow.observedAudio, audioTokenRangeDesc()),
        fieldAccess(\HelixAudioSelfAttentionRow.dominantAudio, audioTokenRangeDesc()),
        fieldAccess(\HelixAudioSelfAttentionRow.frontierDebt, scalarDesc(.f32)),
    ])
}

private func audioSelfAttentionRowListDesc() -> Descriptor {
    listDesc(HelixSchema.audioSelfAttentionRowList, HelixAudioSelfAttentionRow.self, element: audioSelfAttentionRowDesc())
}

private func transcriptTokenDesc() -> Descriptor {
    recordDesc(HelixSchema.transcriptToken, HelixTranscriptToken.self, fields: [
        fieldAccess(\HelixTranscriptToken.textTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixTranscriptToken.decodedInPulse, scalarDesc(.u64)),
        fieldAccess(\HelixTranscriptToken.text, stringDesc()),
        fieldAccess(\HelixTranscriptToken.committed, scalarDesc(.bool)),
    ])
}

private func transcriptTokenListDesc() -> Descriptor {
    listDesc(HelixSchema.transcriptTokenList, HelixTranscriptToken.self, element: transcriptTokenDesc())
}

private func audioClipDesc() -> Descriptor {
    recordDesc(HelixSchema.audioClip, HelixAudioClip.self, fields: [
        fieldAccess(\HelixAudioClip.sampleRate, scalarDesc(.u32)),
        fieldAccess(\HelixAudioClip.firstSample, scalarDesc(.u64)),
        fieldAccess(\HelixAudioClip.samples, f32ListDesc()),
    ])
}

private func optionAudioClipDesc() -> Descriptor {
    optionDesc(HelixSchema.optionAudioClip, HelixAudioClip.self, some: audioClipDesc())
}

private func melClipDesc() -> Descriptor {
    recordDesc(HelixSchema.melClip, HelixMelClip.self, fields: [
        fieldAccess(\HelixMelClip.numMelBins, scalarDesc(.u32)),
        fieldAccess(\HelixMelClip.firstMelFrame, scalarDesc(.u32)),
        fieldAccess(\HelixMelClip.numMelFrames, scalarDesc(.u32)),
        fieldAccess(\HelixMelClip.values, f32ListDesc()),
        fieldAccess(\HelixMelClip.minValue, scalarDesc(.f32)),
        fieldAccess(\HelixMelClip.maxValue, scalarDesc(.f32)),
        fieldAccess(\HelixMelClip.corpusMinValue, scalarDesc(.f32)),
        fieldAccess(\HelixMelClip.corpusMaxValue, scalarDesc(.f32)),
    ])
}

private func optionMelClipDesc() -> Descriptor {
    optionDesc(HelixSchema.optionMelClip, HelixMelClip.self, some: melClipDesc())
}

private func supportSummaryDesc() -> Descriptor {
    recordDesc(HelixSchema.supportSummary, HelixAttentionSupportSummary.self, fields: [
        fieldAccess(\HelixAttentionSupportSummary.totalAudioMass, scalarDesc(.f32)),
        fieldAccess(\HelixAttentionSupportSummary.observedAudio, audioTokenRangeDesc()),
        fieldAccess(\HelixAttentionSupportSummary.dominantAudio, audioTokenRangeDesc()),
        fieldAccess(\HelixAttentionSupportSummary.dominantAudioMass, scalarDesc(.f32)),
        fieldAccess(\HelixAttentionSupportSummary.centerAudioToken, optionF32Desc()),
        fieldAccess(\HelixAttentionSupportSummary.widthAudioTokens, optionF32Desc()),
    ])
}

private func textSupportRecordDesc() -> Descriptor {
    recordDesc(HelixSchema.textSupportRecord, HelixTextAttentionSupportRecord.self, fields: [
        fieldAccess(\HelixTextAttentionSupportRecord.textTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixTextAttentionSupportRecord.queryPosition, scalarDesc(.u32)),
        fieldAccess(\HelixTextAttentionSupportRecord.decoderLayerIndex, scalarDesc(.u32)),
        fieldAccess(\HelixTextAttentionSupportRecord.headIndex, scalarDesc(.u32)),
        fieldAccess(\HelixTextAttentionSupportRecord.support, supportSummaryDesc()),
        fieldAccess(\HelixTextAttentionSupportRecord.audioWeights, f32ListDesc()),
    ])
}

private func textSupportRecordListDesc() -> Descriptor {
    listDesc(HelixSchema.textSupportRecordList, HelixTextAttentionSupportRecord.self, element: textSupportRecordDesc())
}

private func audioEncoderSupportRecordDesc() -> Descriptor {
    recordDesc(HelixSchema.audioEncoderSupportRecord, HelixAudioEncoderSupportRecord.self, fields: [
        fieldAccess(\HelixAudioEncoderSupportRecord.audioTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixAudioEncoderSupportRecord.audioRepresentationVersion, scalarDesc(.u32)),
        fieldAccess(\HelixAudioEncoderSupportRecord.encoderLayerIndex, scalarDesc(.u32)),
        fieldAccess(\HelixAudioEncoderSupportRecord.headIndex, scalarDesc(.u32)),
        fieldAccess(\HelixAudioEncoderSupportRecord.support, supportSummaryDesc()),
        fieldAccess(\HelixAudioEncoderSupportRecord.frontierDebt, scalarDesc(.f32)),
    ])
}

private func audioEncoderSupportRecordListDesc() -> Descriptor {
    listDesc(HelixSchema.audioEncoderSupportRecordList, HelixAudioEncoderSupportRecord.self, element: audioEncoderSupportRecordDesc())
}

private func decoderEvidenceKindDesc() -> Descriptor {
    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: HelixDecoderEvidenceKind.self).pointee {
        case .decode: return 0
        case .verifyPrediction: return 1
        case .verifySeed: return 2
        case .promptPrefill: return 3
        }
    }
    let projectPayload: (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void = { value, _, scratch in
        switch value.assumingMemoryBound(to: HelixDecoderEvidenceKind.self).pointee {
        case .decode(let payload):
            scratch.assumingMemoryBound(to: HelixDecodeEvidencePayload.self).initialize(to: payload)
        case .verifyPrediction(let payload):
            scratch.assumingMemoryBound(to: HelixVerifyPredictionEvidencePayload.self).initialize(to: payload)
        case .verifySeed(let payload):
            scratch.assumingMemoryBound(to: HelixVerifySeedEvidencePayload.self).initialize(to: payload)
        case .promptPrefill:
            break
        }
    }
    let destroyPayload: (UnsafeMutableRawPointer, Int) -> Void = { scratch, localIndex in
        switch localIndex {
        case 0:
            scratch.assumingMemoryBound(to: HelixDecodeEvidencePayload.self).deinitialize(count: 1)
        case 1:
            scratch.assumingMemoryBound(to: HelixVerifyPredictionEvidencePayload.self).deinitialize(count: 1)
        case 2:
            scratch.assumingMemoryBound(to: HelixVerifySeedEvidencePayload.self).deinitialize(count: 1)
        default:
            break
        }
    }
    let inject: (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void = { slot, localIndex, scratch in
        let value: HelixDecoderEvidenceKind
        switch localIndex {
        case 0:
            value = .decode(scratch.assumingMemoryBound(to: HelixDecodeEvidencePayload.self).move())
        case 1:
            value = .verifyPrediction(scratch.assumingMemoryBound(to: HelixVerifyPredictionEvidencePayload.self).move())
        case 2:
            value = .verifySeed(scratch.assumingMemoryBound(to: HelixVerifySeedEvidencePayload.self).move())
        case 3:
            value = .promptPrefill
        default:
            fatalError("bad HelixDecoderEvidenceKind variant index")
        }
        slot.assumingMemoryBound(to: HelixDecoderEvidenceKind.self).initialize(to: value)
    }
    return Descriptor(
        schema: .concrete(HelixSchema.decoderEvidenceKind),
        layout: MemoryLayout<HelixDecoderEvidenceKind>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: projectPayload,
            destroyPayload: destroyPayload,
            inject: inject,
            variants: [
                VariantAccess(
                    wireIndex: 0,
                    payloadFields: [fieldAccess(\HelixDecodeEvidencePayload.inputTokenId, scalarDesc(.u32))],
                    payloadLayout: MemoryLayout<HelixDecodeEvidencePayload>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 1,
                    payloadFields: [
                        fieldAccess(\HelixVerifyPredictionEvidencePayload.verifiedDraftIndex, scalarDesc(.u32)),
                        fieldAccess(\HelixVerifyPredictionEvidencePayload.draftTokenId, scalarDesc(.u32)),
                        fieldAccess(\HelixVerifyPredictionEvidencePayload.queryRow, scalarDesc(.u32)),
                        fieldAccess(\HelixVerifyPredictionEvidencePayload.maxLogit, scalarDesc(.f32)),
                        fieldAccess(\HelixVerifyPredictionEvidencePayload.draftLogit, scalarDesc(.f32)),
                    ],
                    payloadLayout: MemoryLayout<HelixVerifyPredictionEvidencePayload>.phonLayout
                ),
                VariantAccess(
                    wireIndex: 2,
                    payloadFields: [
                        fieldAccess(\HelixVerifySeedEvidencePayload.queryRow, scalarDesc(.u32)),
                        fieldAccess(\HelixVerifySeedEvidencePayload.nextTokenSeed, scalarDesc(.u32)),
                        fieldAccess(\HelixVerifySeedEvidencePayload.maxLogit, scalarDesc(.f32)),
                    ],
                    payloadLayout: MemoryLayout<HelixVerifySeedEvidencePayload>.phonLayout
                ),
                VariantAccess(wireIndex: 3, payloadFields: [], payloadLayout: Layout(size: 0, align: 1)),
            ]
        ))
    )
}

private func decoderEvidenceRecordDesc() -> Descriptor {
    recordDesc(HelixSchema.decoderEvidenceRecord, HelixDecoderEvidenceRecord.self, fields: [
        fieldAccess(\HelixDecoderEvidenceRecord.textTokenId, optionTextTokenIdDesc()),
        fieldAccess(\HelixDecoderEvidenceRecord.queryPosition, scalarDesc(.u32)),
        fieldAccess(\HelixDecoderEvidenceRecord.expectedObservedAudio, audioTokenRangeDesc()),
        fieldAccess(\HelixDecoderEvidenceRecord.records, textSupportRecordListDesc()),
        fieldAccess(\HelixDecoderEvidenceRecord.kind, decoderEvidenceKindDesc()),
    ])
}

private func decoderEvidenceRecordListDesc() -> Descriptor {
    listDesc(HelixSchema.decoderEvidenceRecordList, HelixDecoderEvidenceRecord.self, element: decoderEvidenceRecordDesc())
}

private func queryRowAttentionRecordDesc() -> Descriptor {
    recordDesc(HelixSchema.queryRowAttentionRecord, HelixQueryRowAttentionRecord.self, fields: [
        fieldAccess(\HelixQueryRowAttentionRecord.queryPosition, scalarDesc(.u32)),
        fieldAccess(\HelixQueryRowAttentionRecord.decoderLayerIndex, scalarDesc(.u32)),
        fieldAccess(\HelixQueryRowAttentionRecord.headIndex, scalarDesc(.u32)),
        fieldAccess(\HelixQueryRowAttentionRecord.support, supportSummaryDesc()),
        fieldAccess(\HelixQueryRowAttentionRecord.audioWeights, f32ListDesc()),
    ])
}

private func queryRowAttentionRecordListDesc() -> Descriptor {
    listDesc(HelixSchema.queryRowAttentionRecordList, HelixQueryRowAttentionRecord.self, element: queryRowAttentionRecordDesc())
}

private func attentionSummaryBatchDesc() -> Descriptor {
    recordDesc(HelixSchema.attentionSummaryBatch, HelixAttentionSummaryBatch.self, fields: [
        fieldAccess(\HelixAttentionSummaryBatch.schemaVersion, scalarDesc(.u32)),
        fieldAccess(\HelixAttentionSummaryBatch.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixAttentionSummaryBatch.audioContextId, scalarDesc(.u64)),
        fieldAccess(\HelixAttentionSummaryBatch.textContextId, scalarDesc(.u64)),
        fieldAccess(\HelixAttentionSummaryBatch.audioRepresentationSpans, audioRepresentationSpanListDesc()),
        fieldAccess(\HelixAttentionSummaryBatch.changedAudioRepresentationSpans, audioRepresentationSpanListDesc()),
        fieldAccess(\HelixAttentionSummaryBatch.textSupport, textSupportRecordListDesc()),
        fieldAccess(\HelixAttentionSummaryBatch.headerTextSupport, queryRowAttentionRecordListDesc()),
        fieldAccess(\HelixAttentionSummaryBatch.audioEncoderSupport, audioEncoderSupportRecordListDesc()),
        fieldAccess(\HelixAttentionSummaryBatch.decoderEvidence, decoderEvidenceRecordListDesc()),
    ])
}

private func optionAttentionSummaryBatchDesc() -> Descriptor {
    optionDesc(HelixSchema.optionAttentionSummaryBatch, HelixAttentionSummaryBatch.self, some: attentionSummaryBatchDesc())
}

private func verifyDraftStatusDesc() -> Descriptor {
    unitEnumDesc(
        HelixSchema.verifyDraftStatus,
        HelixVerifyDraftStatus.self,
        variantCount: 3,
        tag: { ptr in
            switch ptr.assumingMemoryBound(to: HelixVerifyDraftStatus.self).pointee {
            case .accepted: return 0
            case .divergent: return 1
            case .discardedAfterDivergence: return 2
            }
        },
        make: { index in
            switch index {
            case 0: return .accepted
            case 1: return .divergent
            case 2: return .discardedAfterDivergence
            default: fatalError("bad HelixVerifyDraftStatus variant index")
            }
        }
    )
}

private func verifyDraftRowDesc() -> Descriptor {
    recordDesc(HelixSchema.verifyDraftRow, HelixVerifyDraftRow.self, fields: [
        fieldAccess(\HelixVerifyDraftRow.draftIndex, scalarDesc(.u32)),
        fieldAccess(\HelixVerifyDraftRow.draftTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixVerifyDraftRow.verifiedTextTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixVerifyDraftRow.text, stringDesc()),
        fieldAccess(\HelixVerifyDraftRow.status, verifyDraftStatusDesc()),
        fieldAccess(\HelixVerifyDraftRow.expectedObservedAudio, audioTokenRangeDesc()),
        fieldAccess(\HelixVerifyDraftRow.maxDominantAudioMass, scalarDesc(.f32)),
        fieldAccess(\HelixVerifyDraftRow.recordCount, scalarDesc(.u32)),
        fieldAccess(\HelixVerifyDraftRow.maxLogit, scalarDesc(.f32)),
        fieldAccess(\HelixVerifyDraftRow.draftLogit, scalarDesc(.f32)),
    ])
}

private func verifyDraftRowListDesc() -> Descriptor {
    listDesc(HelixSchema.verifyDraftRowList, HelixVerifyDraftRow.self, element: verifyDraftRowDesc())
}

private func verifySeedRowDesc() -> Descriptor {
    recordDesc(HelixSchema.verifySeedRow, HelixVerifySeedRow.self, fields: [
        fieldAccess(\HelixVerifySeedRow.queryRow, scalarDesc(.u32)),
        fieldAccess(\HelixVerifySeedRow.nextTokenSeed, scalarDesc(.u32)),
        fieldAccess(\HelixVerifySeedRow.expectedObservedAudio, audioTokenRangeDesc()),
        fieldAccess(\HelixVerifySeedRow.maxDominantAudioMass, scalarDesc(.f32)),
        fieldAccess(\HelixVerifySeedRow.recordCount, scalarDesc(.u32)),
        fieldAccess(\HelixVerifySeedRow.maxLogit, scalarDesc(.f32)),
    ])
}

private func optionVerifySeedRowDesc() -> Descriptor {
    optionDesc(HelixSchema.optionVerifySeedRow, HelixVerifySeedRow.self, some: verifySeedRowDesc())
}

private func verifyEvidenceDigestDesc() -> Descriptor {
    recordDesc(HelixSchema.verifyEvidenceDigest, HelixVerifyEvidenceDigest.self, fields: [
        fieldAccess(\HelixVerifyEvidenceDigest.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixVerifyEvidenceDigest.rewindK, scalarDesc(.u64)),
        fieldAccess(\HelixVerifyEvidenceDigest.acceptedPrefixLen, optionU64Desc()),
        fieldAccess(\HelixVerifyEvidenceDigest.divergenceRow, optionU64Desc()),
        fieldAccess(\HelixVerifyEvidenceDigest.drafts, verifyDraftRowListDesc()),
        fieldAccess(\HelixVerifyEvidenceDigest.seed, optionVerifySeedRowDesc()),
    ])
}

private func optionVerifyEvidenceDigestDesc() -> Descriptor {
    optionDesc(HelixSchema.optionVerifyEvidenceDigest, HelixVerifyEvidenceDigest.self, some: verifyEvidenceDigestDesc())
}

private func decodeFactDesc() -> Descriptor {
    recordDesc(HelixSchema.decodeFact, HelixDecodeFact.self, fields: [
        fieldAccess(\HelixDecodeFact.textTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixDecodeFact.queryPosition, scalarDesc(.u32)),
        fieldAccess(\HelixDecodeFact.inputTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixDecodeFact.observedAudio, audioTokenRangeDesc()),
    ])
}

private func decodeFactListDesc() -> Descriptor {
    listDesc(HelixSchema.decodeFactList, HelixDecodeFact.self, element: decodeFactDesc())
}

private func verifyPredictionFactDesc() -> Descriptor {
    recordDesc(HelixSchema.verifyPredictionFact, HelixVerifyPredictionFact.self, fields: [
        fieldAccess(\HelixVerifyPredictionFact.verifiedTextTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixVerifyPredictionFact.verifiedDraftIndex, scalarDesc(.u32)),
        fieldAccess(\HelixVerifyPredictionFact.draftTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixVerifyPredictionFact.queryRow, scalarDesc(.u32)),
        fieldAccess(\HelixVerifyPredictionFact.queryPosition, scalarDesc(.u32)),
        fieldAccess(\HelixVerifyPredictionFact.observedAudio, audioTokenRangeDesc()),
    ])
}

private func verifyPredictionFactListDesc() -> Descriptor {
    listDesc(HelixSchema.verifyPredictionFactList, HelixVerifyPredictionFact.self, element: verifyPredictionFactDesc())
}

private func verifySeedFactDesc() -> Descriptor {
    recordDesc(HelixSchema.verifySeedFact, HelixVerifySeedFact.self, fields: [
        fieldAccess(\HelixVerifySeedFact.queryRow, scalarDesc(.u32)),
        fieldAccess(\HelixVerifySeedFact.queryPosition, scalarDesc(.u32)),
        fieldAccess(\HelixVerifySeedFact.nextTokenSeed, scalarDesc(.u32)),
        fieldAccess(\HelixVerifySeedFact.observedAudio, audioTokenRangeDesc()),
    ])
}

private func verifySeedFactListDesc() -> Descriptor {
    listDesc(HelixSchema.verifySeedFactList, HelixVerifySeedFact.self, element: verifySeedFactDesc())
}

private func promptPrefillFactDesc() -> Descriptor {
    recordDesc(HelixSchema.promptPrefillFact, HelixPromptPrefillFact.self, fields: [
        fieldAccess(\HelixPromptPrefillFact.queryPosition, scalarDesc(.u32)),
        fieldAccess(\HelixPromptPrefillFact.observedAudio, audioTokenRangeDesc()),
    ])
}

private func promptPrefillFactListDesc() -> Descriptor {
    listDesc(HelixSchema.promptPrefillFactList, HelixPromptPrefillFact.self, element: promptPrefillFactDesc())
}

private func factCountsDesc() -> Descriptor {
    recordDesc(HelixSchema.factCounts, HelixDecoderEvidenceFactCounts.self, fields: [
        fieldAccess(\HelixDecoderEvidenceFactCounts.decode, scalarDesc(.u32)),
        fieldAccess(\HelixDecoderEvidenceFactCounts.verifyPrediction, scalarDesc(.u32)),
        fieldAccess(\HelixDecoderEvidenceFactCounts.verifySeed, scalarDesc(.u32)),
        fieldAccess(\HelixDecoderEvidenceFactCounts.promptPrefill, scalarDesc(.u32)),
    ])
}

private func encoderFactsSnapshotDesc() -> Descriptor {
    recordDesc(HelixSchema.encoderFactsSnapshot, HelixEncoderFactsSnapshot.self, fields: [
        fieldAccess(\HelixEncoderFactsSnapshot.refreshedAudio, audioTokenRangeDesc()),
        fieldAccess(\HelixEncoderFactsSnapshot.audioRepresentationVersion, scalarDesc(.u32)),
        fieldAccess(\HelixEncoderFactsSnapshot.provenance, audioTokenProvenanceListDesc()),
    ])
}

private func optionEncoderFactsSnapshotDesc() -> Descriptor {
    optionDesc(HelixSchema.optionEncoderFactsSnapshot, HelixEncoderFactsSnapshot.self, some: encoderFactsSnapshotDesc())
}

private func pulseEvidenceSnapshotDesc() -> Descriptor {
    recordDesc(HelixSchema.pulseEvidenceSnapshot, HelixPulseEvidenceSnapshot.self, fields: [
        fieldAccess(\HelixPulseEvidenceSnapshot.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixPulseEvidenceSnapshot.encoder, optionEncoderFactsSnapshotDesc()),
        fieldAccess(\HelixPulseEvidenceSnapshot.counts, factCountsDesc()),
        fieldAccess(\HelixPulseEvidenceSnapshot.decode, decodeFactListDesc()),
        fieldAccess(\HelixPulseEvidenceSnapshot.verifyPrediction, verifyPredictionFactListDesc()),
        fieldAccess(\HelixPulseEvidenceSnapshot.verifySeed, verifySeedFactListDesc()),
        fieldAccess(\HelixPulseEvidenceSnapshot.promptPrefill, promptPrefillFactListDesc()),
    ])
}

private func optionPulseEvidenceSnapshotDesc() -> Descriptor {
    optionDesc(HelixSchema.optionPulseEvidenceSnapshot, HelixPulseEvidenceSnapshot.self, some: pulseEvidenceSnapshotDesc())
}

private func provenanceViolationKindDesc() -> Descriptor {
    unitEnumDesc(
        HelixSchema.provenanceViolationKind,
        HelixEncoderProvenanceViolationKind.self,
        variantCount: 4,
        tag: { ptr in
            switch ptr.assumingMemoryBound(to: HelixEncoderProvenanceViolationKind.self).pointee {
            case .missingProvenance: return 0
            case .versionMismatch: return 1
            case .emptyMelFrames: return 2
            case .nonFiniteFrontierDebt: return 3
            }
        },
        make: { index in
            switch index {
            case 0: return .missingProvenance
            case 1: return .versionMismatch
            case 2: return .emptyMelFrames
            case 3: return .nonFiniteFrontierDebt
            default: fatalError("bad HelixEncoderProvenanceViolationKind variant index")
            }
        }
    )
}

private func provenanceViolationDesc() -> Descriptor {
    recordDesc(HelixSchema.provenanceViolation, HelixEncoderProvenanceViolation.self, fields: [
        fieldAccess(\HelixEncoderProvenanceViolation.audioTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixEncoderProvenanceViolation.encoderLayerIndex, scalarDesc(.u32)),
        fieldAccess(\HelixEncoderProvenanceViolation.headIndex, scalarDesc(.u32)),
        fieldAccess(\HelixEncoderProvenanceViolation.observedAudioTokenId, optionAudioTokenIdDesc()),
        fieldAccess(\HelixEncoderProvenanceViolation.kind, provenanceViolationKindDesc()),
        fieldAccess(\HelixEncoderProvenanceViolation.message, stringDesc()),
    ])
}

private func provenanceViolationListDesc() -> Descriptor {
    listDesc(HelixSchema.provenanceViolationList, HelixEncoderProvenanceViolation.self, element: provenanceViolationDesc())
}

private func encoderProvenanceReportDesc() -> Descriptor {
    recordDesc(HelixSchema.encoderProvenanceReport, HelixEncoderProvenanceReport.self, fields: [
        fieldAccess(\HelixEncoderProvenanceReport.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixEncoderProvenanceReport.recordsChecked, scalarDesc(.u64)),
        fieldAccess(\HelixEncoderProvenanceReport.violations, provenanceViolationListDesc()),
    ])
}

private func optionEncoderProvenanceReportDesc() -> Descriptor {
    optionDesc(HelixSchema.optionEncoderProvenanceReport, HelixEncoderProvenanceReport.self, some: encoderProvenanceReportDesc())
}

private func variantCountsDesc() -> Descriptor {
    recordDesc(HelixSchema.variantCounts, HelixDecoderEvidenceVariantCounts.self, fields: [
        fieldAccess(\HelixDecoderEvidenceVariantCounts.decode, scalarDesc(.u64)),
        fieldAccess(\HelixDecoderEvidenceVariantCounts.verifyPrediction, scalarDesc(.u64)),
        fieldAccess(\HelixDecoderEvidenceVariantCounts.verifySeed, scalarDesc(.u64)),
        fieldAccess(\HelixDecoderEvidenceVariantCounts.promptPrefill, scalarDesc(.u64)),
    ])
}

private func decoderEvidenceReportDesc() -> Descriptor {
    recordDesc(HelixSchema.decoderEvidenceReport, HelixDecoderEvidenceReport.self, fields: [
        fieldAccess(\HelixDecoderEvidenceReport.totalBatches, scalarDesc(.u64)),
        fieldAccess(\HelixDecoderEvidenceReport.batchesWithoutDecoderEvidence, scalarDesc(.u64)),
        fieldAccess(\HelixDecoderEvidenceReport.pulsesWithoutDecoderEvidence, u64ListDesc()),
        fieldAccess(\HelixDecoderEvidenceReport.variantEvidenceCounts, variantCountsDesc()),
        fieldAccess(\HelixDecoderEvidenceReport.variantRecordCounts, variantCountsDesc()),
        fieldAccess(\HelixDecoderEvidenceReport.observedDecoderLayerIndices, u32ListDesc()),
        fieldAccess(\HelixDecoderEvidenceReport.observedDecoderHeadIndices, u32ListDesc()),
    ])
}

private func frontierPointDesc() -> Descriptor {
    recordDesc(HelixSchema.frontierPoint, HelixEncoderFrontierPoint.self, fields: [
        fieldAccess(\HelixEncoderFrontierPoint.audioTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixEncoderFrontierPoint.meanFrontierDebt, scalarDesc(.f32)),
        fieldAccess(\HelixEncoderFrontierPoint.headCount, scalarDesc(.u32)),
    ])
}

private func frontierPointListDesc() -> Descriptor {
    listDesc(HelixSchema.frontierPointList, HelixEncoderFrontierPoint.self, element: frontierPointDesc())
}

private func frontierLayerDesc() -> Descriptor {
    recordDesc(HelixSchema.frontierLayer, HelixEncoderFrontierLayer.self, fields: [
        fieldAccess(\HelixEncoderFrontierLayer.encoderLayerIndex, scalarDesc(.u32)),
        fieldAccess(\HelixEncoderFrontierLayer.points, frontierPointListDesc()),
    ])
}

private func frontierLayerListDesc() -> Descriptor {
    listDesc(HelixSchema.frontierLayerList, HelixEncoderFrontierLayer.self, element: frontierLayerDesc())
}

private func frontierSeriesDesc() -> Descriptor {
    recordDesc(HelixSchema.frontierSeries, HelixEncoderFrontierSeries.self, fields: [
        fieldAccess(\HelixEncoderFrontierSeries.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixEncoderFrontierSeries.layers, frontierLayerListDesc()),
        fieldAccess(\HelixEncoderFrontierSeries.minAudioTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixEncoderFrontierSeries.maxAudioTokenId, scalarDesc(.u32)),
        fieldAccess(\HelixEncoderFrontierSeries.minFrontierDebt, scalarDesc(.f32)),
        fieldAccess(\HelixEncoderFrontierSeries.maxFrontierDebt, scalarDesc(.f32)),
    ])
}

private func optionFrontierSeriesDesc() -> Descriptor {
    optionDesc(HelixSchema.optionFrontierSeries, HelixEncoderFrontierSeries.self, some: frontierSeriesDesc())
}

private func tracePositionSpanDesc() -> Descriptor {
    recordDesc(HelixSchema.tracePositionSpan, HelixTracePositionSpan.self, fields: [
        fieldAccess(\HelixTracePositionSpan.logicalStart, scalarDesc(.u64)),
        fieldAccess(\HelixTracePositionSpan.rows, scalarDesc(.u64)),
        fieldAccess(\HelixTracePositionSpan.physicalStart, scalarDesc(.u64)),
    ])
}

private func tracePositionSpanListDesc() -> Descriptor {
    listDesc(HelixSchema.tracePositionSpanList, HelixTracePositionSpan.self, element: tracePositionSpanDesc())
}

private func arDecodeEarlyExitReasonDesc() -> Descriptor {
    unitEnumDesc(
        HelixSchema.arDecodeEarlyExitReason,
        HelixArDecodeEarlyExitReason.self,
        variantCount: 4,
        tag: { ptr in
            switch ptr.assumingMemoryBound(to: HelixArDecodeEarlyExitReason.self).pointee {
            case .budgetExhausted: return 0
            case .noBudget: return 1
            case .seedWasEos: return 2
            case .producedEos: return 3
            }
        },
        make: { index in
            switch index {
            case 0: return .budgetExhausted
            case 1: return .noBudget
            case 2: return .seedWasEos
            case 3: return .producedEos
            default: fatalError("bad HelixArDecodeEarlyExitReason variant index")
            }
        }
    )
}

private func verifySkippedReasonDesc() -> Descriptor {
    unitEnumDesc(
        HelixSchema.verifySkippedReason,
        HelixVerifySkippedReason.self,
        variantCount: 2,
        tag: { ptr in
            switch ptr.assumingMemoryBound(to: HelixVerifySkippedReason.self).pointee {
            case .rewindGuardFailed: return 0
            case .preCommitFullRewind: return 1
            }
        },
        make: { index in
            switch index {
            case 0: return .rewindGuardFailed
            case 1: return .preCommitFullRewind
            default: fatalError("bad HelixVerifySkippedReason variant index")
            }
        }
    )
}

private func pulseTracePayloadDesc() -> Descriptor {
    recordDesc(HelixSchema.streamingTraceEvent, HelixPulseTracePayload.self, fields: [
        fieldAccess(\HelixPulseTracePayload.startUs, scalarDesc(.u64)),
        fieldAccess(\HelixPulseTracePayload.durationUs, scalarDesc(.u64)),
        fieldAccess(\HelixPulseTracePayload.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixPulseTracePayload.previousConsumedMelFrames, scalarDesc(.u64)),
        fieldAccess(\HelixPulseTracePayload.consumedMelFrames, scalarDesc(.u64)),
        fieldAccess(\HelixPulseTracePayload.pulseMelFrames, scalarDesc(.u64)),
        fieldAccess(\HelixPulseTracePayload.committedTextLenStart, scalarDesc(.u64)),
        fieldAccess(\HelixPulseTracePayload.speculativeLenStart, scalarDesc(.u64)),
        fieldAccess(\HelixPulseTracePayload.committedTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseTracePayload.retainedSpeculativeTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseTracePayload.residentCommittedTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseTracePayload.evictedAudioTokens, scalarDesc(.u64)),
        fieldAccess(\HelixPulseTracePayload.evictedCommittedTokens, scalarDesc(.u64)),
    ])
}

private func refreshPromptTracePayloadDesc() -> Descriptor {
    recordDesc(HelixSchema.streamingTraceEvent, HelixRefreshPromptTracePayload.self, fields: [
        fieldAccess(\HelixRefreshPromptTracePayload.startUs, scalarDesc(.u64)),
        fieldAccess(\HelixRefreshPromptTracePayload.durationUs, scalarDesc(.u64)),
        fieldAccess(\HelixRefreshPromptTracePayload.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixRefreshPromptTracePayload.firstAudioTokenId, scalarDesc(.u64)),
        fieldAccess(\HelixRefreshPromptTracePayload.residentAudioFrames, scalarDesc(.u64)),
        fieldAccess(\HelixRefreshPromptTracePayload.committedTextLen, scalarDesc(.u64)),
        fieldAccess(\HelixRefreshPromptTracePayload.residentCommittedLen, scalarDesc(.u64)),
        fieldAccess(\HelixRefreshPromptTracePayload.residentTextLen, scalarDesc(.u64)),
        fieldAccess(\HelixRefreshPromptTracePayload.logicalStart, scalarDesc(.u64)),
        fieldAccess(\HelixRefreshPromptTracePayload.logicalEnd, scalarDesc(.u64)),
        fieldAccess(\HelixRefreshPromptTracePayload.textTokenStart, scalarDesc(.u64)),
        fieldAccess(\HelixRefreshPromptTracePayload.textTokenEnd, scalarDesc(.u64)),
        fieldAccess(\HelixRefreshPromptTracePayload.spans, tracePositionSpanListDesc()),
    ])
}

private func verifyTracePayloadDesc() -> Descriptor {
    recordDesc(HelixSchema.streamingTraceEvent, HelixVerifyTracePayload.self, fields: [
        fieldAccess(\HelixVerifyTracePayload.startUs, scalarDesc(.u64)),
        fieldAccess(\HelixVerifyTracePayload.durationUs, scalarDesc(.u64)),
        fieldAccess(\HelixVerifyTracePayload.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixVerifyTracePayload.rewindK, scalarDesc(.u64)),
        fieldAccess(\HelixVerifyTracePayload.postRewindTextLen, scalarDesc(.u64)),
        fieldAccess(\HelixVerifyTracePayload.textTokenStart, scalarDesc(.u64)),
        fieldAccess(\HelixVerifyTracePayload.textTokenEnd, scalarDesc(.u64)),
        fieldAccess(\HelixVerifyTracePayload.logicalStart, scalarDesc(.u64)),
        fieldAccess(\HelixVerifyTracePayload.logicalEnd, scalarDesc(.u64)),
        fieldAccess(\HelixVerifyTracePayload.spans, tracePositionSpanListDesc()),
        fieldAccess(\HelixVerifyTracePayload.acceptedPrefixLen, optionU64Desc()),
        fieldAccess(\HelixVerifyTracePayload.divergenceRow, optionU64Desc()),
        fieldAccess(\HelixVerifyTracePayload.nextTokenSeed, optionU64Desc()),
        fieldAccess(\HelixVerifyTracePayload.discardedSpeculativeTokens, optionU64Desc()),
        fieldAccess(\HelixVerifyTracePayload.invalidatedSpeculativeSlots, optionU64Desc()),
    ])
}

private func arDecodeTracePayloadDesc() -> Descriptor {
    recordDesc(HelixSchema.streamingTraceEvent, HelixArDecodeTracePayload.self, fields: [
        fieldAccess(\HelixArDecodeTracePayload.startUs, scalarDesc(.u64)),
        fieldAccess(\HelixArDecodeTracePayload.durationUs, scalarDesc(.u64)),
        fieldAccess(\HelixArDecodeTracePayload.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixArDecodeTracePayload.decodeSteps, scalarDesc(.u64)),
        fieldAccess(\HelixArDecodeTracePayload.decodedTokens, scalarDesc(.u64)),
        fieldAccess(\HelixArDecodeTracePayload.speculativeLenEntering, scalarDesc(.u64)),
        fieldAccess(\HelixArDecodeTracePayload.liveSpeculativeTokens, scalarDesc(.u64)),
        fieldAccess(\HelixArDecodeTracePayload.hitEos, scalarDesc(.bool)),
        fieldAccess(\HelixArDecodeTracePayload.seedTokenId, scalarDesc(.u64)),
        fieldAccess(\HelixArDecodeTracePayload.seedTokenText, stringDesc()),
        fieldAccess(\HelixArDecodeTracePayload.earlyExitReason, arDecodeEarlyExitReasonDesc()),
        fieldAccess(\HelixArDecodeTracePayload.nextAfterTail, scalarDesc(.u64)),
    ])
}

private func arTokenTracePayloadDesc() -> Descriptor {
    recordDesc(HelixSchema.streamingTraceEvent, HelixArTokenTracePayload.self, fields: [
        fieldAccess(\HelixArTokenTracePayload.startUs, scalarDesc(.u64)),
        fieldAccess(\HelixArTokenTracePayload.durationUs, scalarDesc(.u64)),
        fieldAccess(\HelixArTokenTracePayload.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixArTokenTracePayload.stepIndex, scalarDesc(.u64)),
        fieldAccess(\HelixArTokenTracePayload.inputTokenId, scalarDesc(.u64)),
        fieldAccess(\HelixArTokenTracePayload.inputText, stringDesc()),
        fieldAccess(\HelixArTokenTracePayload.textTokenId, scalarDesc(.u64)),
        fieldAccess(\HelixArTokenTracePayload.queryPosition, scalarDesc(.u64)),
        fieldAccess(\HelixArTokenTracePayload.physicalStart, scalarDesc(.u64)),
        fieldAccess(\HelixArTokenTracePayload.summaryRecords, scalarDesc(.u64)),
        fieldAccess(\HelixArTokenTracePayload.nextTokenId, scalarDesc(.u64)),
        fieldAccess(\HelixArTokenTracePayload.nextText, stringDesc()),
    ])
}

private func commitTracePayloadDesc() -> Descriptor {
    recordDesc(HelixSchema.streamingTraceEvent, HelixCommitTracePayload.self, fields: [
        fieldAccess(\HelixCommitTracePayload.startUs, scalarDesc(.u64)),
        fieldAccess(\HelixCommitTracePayload.durationUs, scalarDesc(.u64)),
        fieldAccess(\HelixCommitTracePayload.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixCommitTracePayload.speculativeLenPre, scalarDesc(.u64)),
        fieldAccess(\HelixCommitTracePayload.revisableTailTarget, scalarDesc(.u64)),
        fieldAccess(\HelixCommitTracePayload.committedTokens, scalarDesc(.u64)),
        fieldAccess(\HelixCommitTracePayload.retainedSpeculativeTokens, scalarDesc(.u64)),
        fieldAccess(\HelixCommitTracePayload.committedTextLen, scalarDesc(.u64)),
        fieldAccess(\HelixCommitTracePayload.nextAfterCommitted, scalarDesc(.u64)),
    ])
}

private func verifySkippedTracePayloadDesc() -> Descriptor {
    recordDesc(HelixSchema.streamingTraceEvent, HelixVerifySkippedTracePayload.self, fields: [
        fieldAccess(\HelixVerifySkippedTracePayload.timestampUs, scalarDesc(.u64)),
        fieldAccess(\HelixVerifySkippedTracePayload.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixVerifySkippedTracePayload.reason, verifySkippedReasonDesc()),
        fieldAccess(\HelixVerifySkippedTracePayload.rewindK, scalarDesc(.u64)),
        fieldAccess(\HelixVerifySkippedTracePayload.residentCommittedLen, scalarDesc(.u64)),
        fieldAccess(\HelixVerifySkippedTracePayload.speculativeLen, scalarDesc(.u64)),
    ])
}

private func streamingTraceEventDesc() -> Descriptor {
    let tag: (UnsafeRawPointer) -> Int = { ptr in
        switch ptr.assumingMemoryBound(to: HelixStreamingTraceEvent.self).pointee {
        case .pulse: return 0
        case .refreshPrompt: return 1
        case .verify: return 2
        case .arDecode: return 3
        case .arToken: return 4
        case .commit: return 5
        case .verifySkipped: return 6
        }
    }
    let projectPayload: (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void = { value, _, scratch in
        switch value.assumingMemoryBound(to: HelixStreamingTraceEvent.self).pointee {
        case .pulse(let payload):
            scratch.assumingMemoryBound(to: HelixPulseTracePayload.self).initialize(to: payload)
        case .refreshPrompt(let payload):
            scratch.assumingMemoryBound(to: HelixRefreshPromptTracePayload.self).initialize(to: payload)
        case .verify(let payload):
            scratch.assumingMemoryBound(to: HelixVerifyTracePayload.self).initialize(to: payload)
        case .arDecode(let payload):
            scratch.assumingMemoryBound(to: HelixArDecodeTracePayload.self).initialize(to: payload)
        case .arToken(let payload):
            scratch.assumingMemoryBound(to: HelixArTokenTracePayload.self).initialize(to: payload)
        case .commit(let payload):
            scratch.assumingMemoryBound(to: HelixCommitTracePayload.self).initialize(to: payload)
        case .verifySkipped(let payload):
            scratch.assumingMemoryBound(to: HelixVerifySkippedTracePayload.self).initialize(to: payload)
        }
    }
    let destroyPayload: (UnsafeMutableRawPointer, Int) -> Void = { scratch, localIndex in
        switch localIndex {
        case 0: scratch.assumingMemoryBound(to: HelixPulseTracePayload.self).deinitialize(count: 1)
        case 1: scratch.assumingMemoryBound(to: HelixRefreshPromptTracePayload.self).deinitialize(count: 1)
        case 2: scratch.assumingMemoryBound(to: HelixVerifyTracePayload.self).deinitialize(count: 1)
        case 3: scratch.assumingMemoryBound(to: HelixArDecodeTracePayload.self).deinitialize(count: 1)
        case 4: scratch.assumingMemoryBound(to: HelixArTokenTracePayload.self).deinitialize(count: 1)
        case 5: scratch.assumingMemoryBound(to: HelixCommitTracePayload.self).deinitialize(count: 1)
        case 6: scratch.assumingMemoryBound(to: HelixVerifySkippedTracePayload.self).deinitialize(count: 1)
        default: break
        }
    }
    let inject: (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void = { slot, localIndex, scratch in
        let value: HelixStreamingTraceEvent
        switch localIndex {
        case 0:
            value = .pulse(scratch.assumingMemoryBound(to: HelixPulseTracePayload.self).move())
        case 1:
            value = .refreshPrompt(scratch.assumingMemoryBound(to: HelixRefreshPromptTracePayload.self).move())
        case 2:
            value = .verify(scratch.assumingMemoryBound(to: HelixVerifyTracePayload.self).move())
        case 3:
            value = .arDecode(scratch.assumingMemoryBound(to: HelixArDecodeTracePayload.self).move())
        case 4:
            value = .arToken(scratch.assumingMemoryBound(to: HelixArTokenTracePayload.self).move())
        case 5:
            value = .commit(scratch.assumingMemoryBound(to: HelixCommitTracePayload.self).move())
        case 6:
            value = .verifySkipped(scratch.assumingMemoryBound(to: HelixVerifySkippedTracePayload.self).move())
        default:
            fatalError("bad HelixStreamingTraceEvent variant index")
        }
        slot.assumingMemoryBound(to: HelixStreamingTraceEvent.self).initialize(to: value)
    }
    return Descriptor(
        schema: .concrete(HelixSchema.streamingTraceEvent),
        layout: MemoryLayout<HelixStreamingTraceEvent>.phonLayout,
        access: .enumeration(EnumAccess(
            tag: tag,
            projectPayload: projectPayload,
            destroyPayload: destroyPayload,
            inject: inject,
            variants: [
                VariantAccess(wireIndex: 0, payloadFields: pulseTracePayloadDesc().recordFields, payloadLayout: MemoryLayout<HelixPulseTracePayload>.phonLayout),
                VariantAccess(wireIndex: 1, payloadFields: refreshPromptTracePayloadDesc().recordFields, payloadLayout: MemoryLayout<HelixRefreshPromptTracePayload>.phonLayout),
                VariantAccess(wireIndex: 2, payloadFields: verifyTracePayloadDesc().recordFields, payloadLayout: MemoryLayout<HelixVerifyTracePayload>.phonLayout),
                VariantAccess(wireIndex: 3, payloadFields: arDecodeTracePayloadDesc().recordFields, payloadLayout: MemoryLayout<HelixArDecodeTracePayload>.phonLayout),
                VariantAccess(wireIndex: 4, payloadFields: arTokenTracePayloadDesc().recordFields, payloadLayout: MemoryLayout<HelixArTokenTracePayload>.phonLayout),
                VariantAccess(wireIndex: 5, payloadFields: commitTracePayloadDesc().recordFields, payloadLayout: MemoryLayout<HelixCommitTracePayload>.phonLayout),
                VariantAccess(wireIndex: 6, payloadFields: verifySkippedTracePayloadDesc().recordFields, payloadLayout: MemoryLayout<HelixVerifySkippedTracePayload>.phonLayout),
            ]
        ))
    )
}

private func streamingTraceEventListDesc() -> Descriptor {
    listDesc(HelixSchema.streamingTraceEventList, HelixStreamingTraceEvent.self, element: streamingTraceEventDesc())
}

private func optionStreamingTraceEventListDesc() -> Descriptor {
    optionDesc(HelixSchema.optionStreamingTraceEventList, [HelixStreamingTraceEvent].self, some: streamingTraceEventListDesc())
}

private func chromeTraceEventDesc() -> Descriptor {
    recordDesc(HelixSchema.chromeTraceEvent, HelixChromeTraceEvent.self, fields: [
        fieldAccess(\HelixChromeTraceEvent.name, stringDesc()),
        fieldAccess(\HelixChromeTraceEvent.cat, stringDesc()),
        fieldAccess(\HelixChromeTraceEvent.ph, stringDesc()),
        fieldAccess(\HelixChromeTraceEvent.ts, scalarDesc(.f64)),
        fieldAccess(\HelixChromeTraceEvent.dur, optionF64Desc()),
        fieldAccess(\HelixChromeTraceEvent.pid, scalarDesc(.u32)),
        fieldAccess(\HelixChromeTraceEvent.tid, scalarDesc(.u32)),
        fieldAccess(\HelixChromeTraceEvent.s, optionStringDesc()),
        fieldAccess(\HelixChromeTraceEvent.args, stringValueMapDesc()),
    ])
}

private func chromeTraceEventListDesc() -> Descriptor {
    listDesc(HelixSchema.chromeTraceEventList, HelixChromeTraceEvent.self, element: chromeTraceEventDesc())
}

private func optionChromeTraceEventListDesc() -> Descriptor {
    optionDesc(HelixSchema.optionChromeTraceEventList, [HelixChromeTraceEvent].self, some: chromeTraceEventListDesc())
}

private extension Descriptor {
    var recordFields: [FieldAccess] {
        if case .record(let record) = access {
            return record.fields
        }
        fatalError("expected record descriptor")
    }
}

private func pulseBundleFieldsDesc() -> Descriptor {
    recordDesc(HelixSchema.pulseBundleFields, HelixPulseBundleFields.self, fields: [
        fieldAccess(\HelixPulseBundleFields.promptLayout, scalarDesc(.bool)),
        fieldAccess(\HelixPulseBundleFields.audioProvenance, scalarDesc(.bool)),
        fieldAccess(\HelixPulseBundleFields.attentionHeatmap, scalarDesc(.bool)),
        fieldAccess(\HelixPulseBundleFields.encoderFrontier, scalarDesc(.bool)),
        fieldAccess(\HelixPulseBundleFields.encoderProvenance, scalarDesc(.bool)),
        fieldAccess(\HelixPulseBundleFields.audioClip, scalarDesc(.bool)),
        fieldAccess(\HelixPulseBundleFields.melClip, scalarDesc(.bool)),
        fieldAccess(\HelixPulseBundleFields.pulseRollup, scalarDesc(.bool)),
        fieldAccess(\HelixPulseBundleFields.timeline, scalarDesc(.bool)),
        fieldAccess(\HelixPulseBundleFields.gpuChromeEvents, scalarDesc(.bool)),
        fieldAccess(\HelixPulseBundleFields.verifyEvidence, scalarDesc(.bool)),
        fieldAccess(\HelixPulseBundleFields.schedulerSnapshot, scalarDesc(.bool)),
    ])
}

private func pulseBundleDesc() -> Descriptor {
    recordDesc(HelixSchema.pulseBundle, HelixPulseBundle.self, fields: [
        fieldAccess(\HelixPulseBundle.pulseId, scalarDesc(.u64)),
        fieldAccess(\HelixPulseBundle.schemaVersion, scalarDesc(.u32)),
        fieldAccess(\HelixPulseBundle.promptLayout, optionPromptLayoutDesc()),
        fieldAccess(\HelixPulseBundle.audioProvenance, optionAudioTokenProvenanceListDesc()),
        fieldAccess(\HelixPulseBundle.attentionHeatmap, optionAttentionHeatmapDesc()),
        fieldAccess(\HelixPulseBundle.encoderFrontier, optionFrontierSeriesDesc()),
        fieldAccess(\HelixPulseBundle.encoderProvenance, optionEncoderProvenanceReportDesc()),
        fieldAccess(\HelixPulseBundle.audioClip, optionAudioClipDesc()),
        fieldAccess(\HelixPulseBundle.melClip, optionMelClipDesc()),
        fieldAccess(\HelixPulseBundle.pulseRollup, optionPulseRollupDesc()),
        fieldAccess(\HelixPulseBundle.timeline, optionStreamingTraceEventListDesc()),
        fieldAccess(\HelixPulseBundle.gpuChromeEvents, optionChromeTraceEventListDesc()),
        fieldAccess(\HelixPulseBundle.verifyEvidence, optionVerifyEvidenceDigestDesc()),
        fieldAccess(\HelixPulseBundle.schedulerSnapshot, optionPulseEvidenceSnapshotDesc()),
    ])
}

private func pieceEvalSnapshotDesc() -> Descriptor {
    recordDesc(HelixSchema.pieceEvalSnapshot, HelixPieceEvalSnapshot.self, fields: [
        fieldAccess(\HelixPieceEvalSnapshot.audioNowMs, scalarDesc(.f64)),
        fieldAccess(\HelixPieceEvalSnapshot.referenceWordsAvailable, scalarDesc(.u32)),
        fieldAccess(\HelixPieceEvalSnapshot.hypothesisWords, scalarDesc(.u32)),
        fieldAccess(\HelixPieceEvalSnapshot.substitutions, scalarDesc(.u32)),
        fieldAccess(\HelixPieceEvalSnapshot.deletions, scalarDesc(.u32)),
        fieldAccess(\HelixPieceEvalSnapshot.insertions, scalarDesc(.u32)),
        fieldAccess(\HelixPieceEvalSnapshot.rollingWer, scalarDesc(.f64)),
        fieldAccess(\HelixPieceEvalSnapshot.s2dMatchedWords, scalarDesc(.u32)),
        fieldAccess(\HelixPieceEvalSnapshot.s2dNewWords, scalarDesc(.u32)),
        fieldAccess(\HelixPieceEvalSnapshot.s2dP50Ms, optionF64Desc()),
        fieldAccess(\HelixPieceEvalSnapshot.s2dP90Ms, optionF64Desc()),
        fieldAccess(\HelixPieceEvalSnapshot.s2dP100Ms, optionF64Desc()),
        fieldAccess(\HelixPieceEvalSnapshot.s2dAvgMs, optionF64Desc()),
        fieldAccess(\HelixPieceEvalSnapshot.audioFrontier, scalarDesc(.u32)),
        fieldAccess(\HelixPieceEvalSnapshot.displayedFrontier, scalarDesc(.u32)),
        fieldAccess(\HelixPieceEvalSnapshot.committedFrontier, scalarDesc(.u32)),
        fieldAccess(\HelixPieceEvalSnapshot.lagMs, scalarDesc(.f64)),
    ])
}

private func optionPieceEvalSnapshotDesc() -> Descriptor {
    optionDesc(HelixSchema.optionPieceEvalSnapshot, HelixPieceEvalSnapshot.self, some: pieceEvalSnapshotDesc())
}

private func pieceEvalReferenceDesc() -> Descriptor {
    recordDesc(HelixSchema.pieceEvalReference, HelixPieceEvalReference.self, fields: [
        fieldAccess(\HelixPieceEvalReference.piece, stringDesc()),
        fieldAccess(\HelixPieceEvalReference.language, stringDesc()),
        fieldAccess(\HelixPieceEvalReference.words, stringListDesc()),
    ])
}

private func optionPieceEvalReferenceDesc() -> Descriptor {
    optionDesc(HelixSchema.optionPieceEvalReference, HelixPieceEvalReference.self, some: pieceEvalReferenceDesc())
}

private func traceServiceSurfaceDesc() -> Descriptor {
    recordDesc(HelixSchema.traceServiceSurface, HelixTraceServiceSurface.self, fields: [
        fieldAccess(\HelixTraceServiceSurface.meta, streamMetaDesc()),
        fieldAccess(\HelixTraceServiceSurface.pulseRollup, optionPulseRollupDesc()),
        fieldAccess(\HelixTraceServiceSurface.timeline, streamingTraceEventListDesc()),
        fieldAccess(\HelixTraceServiceSurface.attentionBatch, optionAttentionSummaryBatchDesc()),
        fieldAccess(\HelixTraceServiceSurface.promptLayout, optionPromptLayoutDesc()),
        fieldAccess(\HelixTraceServiceSurface.audioAttendedBy, textAttendanceRowListDesc()),
        fieldAccess(\HelixTraceServiceSurface.textAttendsTo, audioAttendanceRowListDesc()),
        fieldAccess(\HelixTraceServiceSurface.refreshAttendsTo, refreshAttendanceRowListDesc()),
        fieldAccess(\HelixTraceServiceSurface.audioTokenProvenance, optionAudioTokenProvenanceDesc()),
        fieldAccess(\HelixTraceServiceSurface.audioProvenanceForPulse, audioTokenProvenanceListDesc()),
        fieldAccess(\HelixTraceServiceSurface.audioTokensForMelFrame, u32ListDesc()),
        fieldAccess(\HelixTraceServiceSurface.audioClipForAudioToken, optionAudioClipDesc()),
        fieldAccess(\HelixTraceServiceSurface.audioClipForPrompt, optionAudioClipDesc()),
        fieldAccess(\HelixTraceServiceSurface.audioClipForAudioRange, optionAudioClipDesc()),
        fieldAccess(\HelixTraceServiceSurface.melClipForPrompt, optionMelClipDesc()),
        fieldAccess(\HelixTraceServiceSurface.audioSelfAttention, audioSelfAttentionRowListDesc()),
        fieldAccess(\HelixTraceServiceSurface.transcript, transcriptTokenListDesc()),
        fieldAccess(\HelixTraceServiceSurface.pulseAttentionHeatmap, optionAttentionHeatmapDesc()),
        fieldAccess(\HelixTraceServiceSurface.encoderFrontier, optionFrontierSeriesDesc()),
        fieldAccess(\HelixTraceServiceSurface.streamMetrics, streamMetricsDesc()),
        fieldAccess(\HelixTraceServiceSurface.verifyEvidence, optionVerifyEvidenceDigestDesc()),
        fieldAccess(\HelixTraceServiceSurface.decoderEvidenceReport, decoderEvidenceReportDesc()),
        fieldAccess(\HelixTraceServiceSurface.pulseEvidenceSnapshot, optionPulseEvidenceSnapshotDesc()),
        fieldAccess(\HelixTraceServiceSurface.gpuChromeEventsForPulse, chromeTraceEventListDesc()),
        fieldAccess(\HelixTraceServiceSurface.runInfo, optionRunInfoDesc()),
        fieldAccess(\HelixTraceServiceSurface.pieceEvalReference, optionPieceEvalReferenceDesc()),
        fieldAccess(\HelixTraceServiceSurface.pieceEvalForPulse, optionPieceEvalSnapshotDesc()),
        fieldAccess(\HelixTraceServiceSurface.encoderProvenanceReport, optionEncoderProvenanceReportDesc()),
        fieldAccess(\HelixTraceServiceSurface.pulseBundleFields, pulseBundleFieldsDesc()),
        fieldAccess(\HelixTraceServiceSurface.pulseBundle, pulseBundleDesc()),
        fieldAccess(\HelixTraceServiceSurface.pulseAvailable, pulseAvailableDesc()),
    ])
}

public func helixTraceServiceSurfaceDescriptor() -> (root: Descriptor, registry: Registry) {
    (traceServiceSurfaceDesc(), Registry(helixSchemas()))
}

private func helixRange(_ start: UInt32, _ end: UInt32) -> HelixAudioTokenRange {
    HelixAudioTokenRange(start: start, end: end)
}

private func sampleHelixSupport() -> HelixAttentionSupportSummary {
    HelixAttentionSupportSummary(
        totalAudioMass: 0.5,
        observedAudio: helixRange(32, 40),
        dominantAudio: helixRange(34, 36),
        dominantAudioMass: 0.25,
        centerAudioToken: 35.25,
        widthAudioTokens: 3.5
    )
}

private func sampleHelixTextSupport() -> [HelixTextAttentionSupportRecord] {
    [
        HelixTextAttentionSupportRecord(
            textTokenId: 91,
            queryPosition: 118,
            decoderLayerIndex: 7,
            headIndex: 3,
            support: sampleHelixSupport(),
            audioWeights: [0.0625, 0.125, 0.25, 0.5]
        ),
    ]
}

private func sampleHelixAudioProvenance() -> [HelixAudioTokenProvenance] {
    [
        HelixAudioTokenProvenance(
            audioTokenId: 34,
            audioRepresentationVersion: 7,
            melFrames: [HelixMelFrameRange(start: 128, end: 136)],
            nativeWindow: 2,
            convStemChunk: 4,
            postMergeAudioTokenId: 34,
            merge: .noMerge(HelixNoMergePayload(preMergeAudioTokenId: 34)),
            admission: .admitAll(HelixAdmitAllPayload(admissionSegment: 12)),
            cosineToPrevious: 0.875
        ),
        HelixAudioTokenProvenance(
            audioTokenId: 35,
            audioRepresentationVersion: 7,
            melFrames: [HelixMelFrameRange(start: 136, end: 144), HelixMelFrameRange(start: 144, end: 152)],
            nativeWindow: 2,
            convStemChunk: 4,
            postMergeAudioTokenId: 35,
            merge: .merged(HelixMergedPayload(preMerge: helixRange(35, 37))),
            admission: .admitAll(HelixAdmitAllPayload(admissionSegment: 13)),
            cosineToPrevious: nil
        ),
    ]
}

private func sampleHelixVerifyEvidence() -> HelixVerifyEvidenceDigest {
    HelixVerifyEvidenceDigest(
        pulseId: 17,
        rewindK: 2,
        acceptedPrefixLen: 1,
        divergenceRow: 1,
        drafts: [
            HelixVerifyDraftRow(
                draftIndex: 0,
                draftTokenId: 1201,
                verifiedTextTokenId: 91,
                text: "pho",
                status: .accepted,
                expectedObservedAudio: helixRange(32, 36),
                maxDominantAudioMass: 0.5,
                recordCount: 16,
                maxLogit: 13.5,
                draftLogit: 13.125
            ),
            HelixVerifyDraftRow(
                draftIndex: 1,
                draftTokenId: 1202,
                verifiedTextTokenId: 92,
                text: "n",
                status: .divergent,
                expectedObservedAudio: helixRange(36, 40),
                maxDominantAudioMass: 0.375,
                recordCount: 16,
                maxLogit: 11.5,
                draftLogit: 9.25
            ),
        ],
        seed: HelixVerifySeedRow(
            queryRow: 2,
            nextTokenSeed: 1401,
            expectedObservedAudio: helixRange(40, 48),
            maxDominantAudioMass: 0.25,
            recordCount: 8,
            maxLogit: 10.75
        )
    )
}

private func sampleHelixAudioClip() -> HelixAudioClip {
    HelixAudioClip(
        sampleRate: 16_000,
        firstSample: 262_144,
        samples: [-0.25, -0.125, 0, 0.125, 0.25, 0.5, 0.25, 0]
    )
}

private func sampleHelixMelClip() -> HelixMelClip {
    HelixMelClip(
        numMelBins: 4,
        firstMelFrame: 128,
        numMelFrames: 3,
        values: [0.125, 0.25, 0.375, 0.5, 0.0625, 0.1875, 0.3125, 0.4375, 0.03125, 0.125, 0.25, 0.375],
        minValue: 0.03125,
        maxValue: 0.5,
        corpusMinValue: -1.25,
        corpusMaxValue: 2.75
    )
}

private func sampleHelixEncoderFrontier() -> HelixEncoderFrontierSeries {
    HelixEncoderFrontierSeries(
        pulseId: 17,
        layers: [
            HelixEncoderFrontierLayer(
                encoderLayerIndex: 3,
                points: [
                    HelixEncoderFrontierPoint(audioTokenId: 34, meanFrontierDebt: 0.125, headCount: 4),
                    HelixEncoderFrontierPoint(audioTokenId: 35, meanFrontierDebt: 0.25, headCount: 4),
                ]
            ),
        ],
        minAudioTokenId: 34,
        maxAudioTokenId: 35,
        minFrontierDebt: 0.125,
        maxFrontierDebt: 0.25
    )
}

private func sampleHelixEncoderProvenanceReport() -> HelixEncoderProvenanceReport {
    HelixEncoderProvenanceReport(
        pulseId: 17,
        recordsChecked: 32,
        violations: [
            HelixEncoderProvenanceViolation(
                audioTokenId: 36,
                encoderLayerIndex: 2,
                headIndex: 3,
                observedAudioTokenId: 37,
                kind: .versionMismatch,
                message: "observed audio provenance version lagged refresh"
            ),
        ]
    )
}

private func sampleHelixTimeline() -> [HelixStreamingTraceEvent] {
    [
        .pulse(HelixPulseTracePayload(
            startUs: 1_000_000,
            durationUs: 44_000,
            pulseId: 17,
            previousConsumedMelFrames: 1_600,
            consumedMelFrames: 1_624,
            pulseMelFrames: 24,
            committedTextLenStart: 78,
            speculativeLenStart: 4,
            committedTokens: 3,
            retainedSpeculativeTokens: 5,
            residentCommittedTokens: 80,
            evictedAudioTokens: 2,
            evictedCommittedTokens: 1
        )),
        .refreshPrompt(HelixRefreshPromptTracePayload(
            startUs: 1_002_500,
            durationUs: 8_000,
            pulseId: 17,
            firstAudioTokenId: 32,
            residentAudioFrames: 8,
            committedTextLen: 80,
            residentCommittedLen: 80,
            residentTextLen: 85,
            logicalStart: 90,
            logicalEnd: 118,
            textTokenStart: 90,
            textTokenEnd: 92,
            spans: [HelixTracePositionSpan(logicalStart: 90, rows: 8, physicalStart: 12)]
        )),
        .verify(HelixVerifyTracePayload(
            startUs: 1_004_000,
            durationUs: 4_000,
            pulseId: 17,
            rewindK: 2,
            postRewindTextLen: 81,
            textTokenStart: 90,
            textTokenEnd: 92,
            logicalStart: 114,
            logicalEnd: 117,
            spans: [HelixTracePositionSpan(logicalStart: 114, rows: 3, physicalStart: 46)],
            acceptedPrefixLen: 1,
            divergenceRow: 1,
            nextTokenSeed: 1401,
            discardedSpeculativeTokens: 1,
            invalidatedSpeculativeSlots: 2
        )),
        .arDecode(HelixArDecodeTracePayload(
            startUs: 1_005_000,
            durationUs: 16_000,
            pulseId: 17,
            decodeSteps: 5,
            decodedTokens: 6,
            speculativeLenEntering: 1,
            liveSpeculativeTokens: 6,
            hitEos: false,
            seedTokenId: 1401,
            seedTokenText: "pho",
            earlyExitReason: .budgetExhausted,
            nextAfterTail: 1502
        )),
        .arToken(HelixArTokenTracePayload(
            startUs: 1_005_100,
            durationUs: 300,
            pulseId: 17,
            stepIndex: 0,
            inputTokenId: 1401,
            inputText: "pho",
            textTokenId: 91,
            queryPosition: 118,
            physicalStart: 49,
            summaryRecords: 64,
            nextTokenId: 1502,
            nextText: "n"
        )),
        .commit(HelixCommitTracePayload(
            startUs: 1_007_500,
            durationUs: 1_000,
            pulseId: 17,
            speculativeLenPre: 6,
            revisableTailTarget: 2,
            committedTokens: 3,
            retainedSpeculativeTokens: 5,
            committedTextLen: 83,
            nextAfterCommitted: 1502
        )),
        .verifySkipped(HelixVerifySkippedTracePayload(
            timestampUs: 1_007_800,
            pulseId: 17,
            reason: .preCommitFullRewind,
            rewindK: 0,
            residentCommittedLen: 0,
            speculativeLen: 2
        )),
    ]
}

private func sampleHelixChromeEvents() -> [HelixChromeTraceEvent] {
    [
        HelixChromeTraceEvent(
            name: "metal.dispatch",
            cat: "gpu",
            ph: "X",
            ts: 1_006_000,
            dur: 420,
            pid: 2,
            tid: 7,
            s: nil,
            args: ["pulse_id": .number(.canonical(unsigned: 17))]
        ),
    ]
}

private func sampleHelixPulseEvidence() -> HelixPulseEvidenceSnapshot {
    HelixPulseEvidenceSnapshot(
        pulseId: 17,
        encoder: HelixEncoderFactsSnapshot(
            refreshedAudio: helixRange(32, 40),
            audioRepresentationVersion: 7,
            provenance: sampleHelixAudioProvenance()
        ),
        counts: HelixDecoderEvidenceFactCounts(decode: 1, verifyPrediction: 1, verifySeed: 1, promptPrefill: 1),
        decode: [
            HelixDecodeFact(textTokenId: 91, queryPosition: 118, inputTokenId: 1401, observedAudio: helixRange(32, 40)),
        ],
        verifyPrediction: [
            HelixVerifyPredictionFact(
                verifiedTextTokenId: 92,
                verifiedDraftIndex: 1,
                draftTokenId: 1202,
                queryRow: 2,
                queryPosition: 116,
                observedAudio: helixRange(36, 40)
            ),
        ],
        verifySeed: [
            HelixVerifySeedFact(queryRow: 3, queryPosition: 117, nextTokenSeed: 1401, observedAudio: helixRange(40, 48)),
        ],
        promptPrefill: [
            HelixPromptPrefillFact(queryPosition: 90, observedAudio: helixRange(32, 40)),
        ]
    )
}

private func sampleHelixAttentionBatch() -> HelixAttentionSummaryBatch {
    HelixAttentionSummaryBatch(
        schemaVersion: 5,
        pulseId: 17,
        audioContextId: 7001,
        textContextId: 8001,
        audioRepresentationSpans: [
            HelixAudioRepresentationSpan(audio: helixRange(32, 40), audioRepresentationVersion: 7),
        ],
        changedAudioRepresentationSpans: [
            HelixAudioRepresentationSpan(audio: helixRange(34, 36), audioRepresentationVersion: 8),
        ],
        textSupport: sampleHelixTextSupport(),
        headerTextSupport: [
            HelixQueryRowAttentionRecord(
                queryPosition: 90,
                decoderLayerIndex: 1,
                headIndex: 2,
                support: sampleHelixSupport(),
                audioWeights: [0.0625, 0.125, 0.25, 0.5]
            ),
        ],
        audioEncoderSupport: [
            HelixAudioEncoderSupportRecord(
                audioTokenId: 34,
                audioRepresentationVersion: 7,
                encoderLayerIndex: 3,
                headIndex: 4,
                support: sampleHelixSupport(),
                frontierDebt: 0.125
            ),
        ],
        decoderEvidence: [
            HelixDecoderEvidenceRecord(
                textTokenId: 91,
                queryPosition: 118,
                expectedObservedAudio: helixRange(32, 40),
                records: sampleHelixTextSupport(),
                kind: .decode(HelixDecodeEvidencePayload(inputTokenId: 1401))
            ),
            HelixDecoderEvidenceRecord(
                textTokenId: 92,
                queryPosition: 116,
                expectedObservedAudio: helixRange(36, 40),
                records: sampleHelixTextSupport(),
                kind: .verifyPrediction(HelixVerifyPredictionEvidencePayload(
                    verifiedDraftIndex: 1,
                    draftTokenId: 1202,
                    queryRow: 2,
                    maxLogit: 11.5,
                    draftLogit: 9.25
                ))
            ),
            HelixDecoderEvidenceRecord(
                textTokenId: nil,
                queryPosition: 117,
                expectedObservedAudio: helixRange(40, 48),
                records: sampleHelixTextSupport(),
                kind: .verifySeed(HelixVerifySeedEvidencePayload(queryRow: 3, nextTokenSeed: 1401, maxLogit: 10.75))
            ),
            HelixDecoderEvidenceRecord(
                textTokenId: nil,
                queryPosition: 90,
                expectedObservedAudio: helixRange(32, 40),
                records: sampleHelixTextSupport(),
                kind: .promptPrefill
            ),
        ]
    )
}

private func sampleHelixPieceEval() -> HelixPieceEvalSnapshot {
    HelixPieceEvalSnapshot(
        audioNowMs: 12_500,
        referenceWordsAvailable: 42,
        hypothesisWords: 40,
        substitutions: 2,
        deletions: 1,
        insertions: 0,
        rollingWer: 0.0714,
        s2dMatchedWords: 38,
        s2dNewWords: 4,
        s2dP50Ms: 210,
        s2dP90Ms: 330,
        s2dP100Ms: 410,
        s2dAvgMs: 240.5,
        audioFrontier: 43,
        displayedFrontier: 40,
        committedFrontier: 38,
        lagMs: -120
    )
}

private func sampleHelixPulseBundle(
    pulseId: UInt64,
    promptLayout: HelixPromptLayout,
    heatmap: HelixPulseAttentionHeatmap,
    rollup: HelixPulseRollup
) -> HelixPulseBundle {
    HelixPulseBundle(
        pulseId: pulseId,
        schemaVersion: 1,
        promptLayout: promptLayout,
        audioProvenance: sampleHelixAudioProvenance(),
        attentionHeatmap: heatmap,
        encoderFrontier: sampleHelixEncoderFrontier(),
        encoderProvenance: sampleHelixEncoderProvenanceReport(),
        audioClip: sampleHelixAudioClip(),
        melClip: sampleHelixMelClip(),
        pulseRollup: rollup,
        timeline: sampleHelixTimeline(),
        gpuChromeEvents: sampleHelixChromeEvents(),
        verifyEvidence: sampleHelixVerifyEvidence(),
        schedulerSnapshot: sampleHelixPulseEvidence()
    )
}

public func sampleHelixTraceServiceSurface() -> HelixTraceServiceSurface {
    let pulseId: UInt64 = 17
    let audioRange = helixRange(32, 40)
    let rollup = HelixPulseRollup(
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
        verify: HelixVerifyOutcome(rewindK: 2, acceptedPrefixLen: 3, divergenceRow: 4, discardedSpeculativeTokens: nil),
        hasAttentionBatch: true,
        arTokenCount: 6
    )
    let promptLayout = HelixPromptLayout(
        pulseId: pulseId,
        firstAudioTokenId: audioRange.start,
        residentAudioFrames: 8,
        changedAudioSpans: [
            HelixAudioRepresentationSpan(audio: audioRange, audioRepresentationVersion: 3),
        ],
        textTokenStart: 90,
        textTokenEnd: 92,
        textTokens: [
            HelixTextTokenSnapshot(textTokenId: 90, text: "pho", textBefore: "fo", inVerifyBatch: true, decodedThisPulse: true),
            HelixTextTokenSnapshot(textTokenId: 91, text: "n", textBefore: nil, inVerifyBatch: false, decodedThisPulse: true),
        ]
    )
    let heatmap = HelixPulseAttentionHeatmap(
        pulseId: pulseId,
        firstAudioTokenId: audioRange.start,
        audioTokenCount: 4,
        textTokenStart: 90,
        textTokenCount: 2,
        recordCount: 8,
        maxValue: 0.75,
        meanAudioMass: [0.5, 0.25, 0.125, 0.0625, 0.75, 0.375, 0.1875, 0.09375],
        textTokenGlyphs: ["pho", "n"]
    )
    let metrics = HelixStreamMetrics(
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
        s2dP50Ms: [220, 210]
    )
    let runInfo = HelixRunInfo(
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
    )

    return HelixTraceServiceSurface(
        meta: HelixStreamMeta(schemaVersion: 1, pulseIds: [16, pulseId], timelineEventCount: 420, attentionBatchCount: 17),
        pulseRollup: rollup,
        timeline: sampleHelixTimeline(),
        attentionBatch: sampleHelixAttentionBatch(),
        promptLayout: promptLayout,
        audioAttendedBy: [
            HelixTextAttendanceRow(
                textTokenId: 91,
                decoderLayerIndex: 7,
                headIndex: 3,
                dominantAudioMass: 0.25,
                totalAudioMass: 0.5,
                observedAudio: audioRange,
                dominantAudio: helixRange(34, 36),
                audioWeights: [0.0625, 0.125, 0.25, 0.5],
                queriedAudioWeight: 0.25
            ),
        ],
        textAttendsTo: [
            HelixAudioAttendanceRow(
                decoderLayerIndex: 7,
                headIndex: 3,
                dominantAudioMass: 0.25,
                totalAudioMass: 0.5,
                centerAudioToken: 35.25,
                widthAudioTokens: 3.5,
                observedAudio: audioRange,
                dominantAudio: helixRange(34, 36),
                audioWeights: [0.0625, 0.125, 0.25, 0.5]
            ),
        ],
        refreshAttendsTo: [
            HelixRefreshAttendanceRow(
                queryPosition: 90,
                decoderLayerIndex: 1,
                headIndex: 2,
                dominantAudioMass: 0.1875,
                totalAudioMass: 0.375,
                centerAudioToken: 34.75,
                widthAudioTokens: 2.5,
                observedAudio: audioRange,
                dominantAudio: helixRange(34, 36),
                audioWeights: [0.0625, 0.125, 0.25, 0.375]
            ),
        ],
        audioTokenProvenance: sampleHelixAudioProvenance().first,
        audioProvenanceForPulse: sampleHelixAudioProvenance(),
        audioTokensForMelFrame: [34, 35],
        audioClipForAudioToken: sampleHelixAudioClip(),
        audioClipForPrompt: sampleHelixAudioClip(),
        audioClipForAudioRange: sampleHelixAudioClip(),
        melClipForPrompt: sampleHelixMelClip(),
        audioSelfAttention: [
            HelixAudioSelfAttentionRow(
                encoderLayerIndex: 3,
                headIndex: 4,
                audioRepresentationVersion: 7,
                dominantAudioMass: 0.375,
                totalAudioMass: 0.75,
                centerAudioToken: 35.5,
                widthAudioTokens: 4,
                observedAudio: audioRange,
                dominantAudio: helixRange(34, 36),
                frontierDebt: 0.125
            ),
        ],
        transcript: [
            HelixTranscriptToken(textTokenId: 90, decodedInPulse: pulseId, text: "pho", committed: true),
            HelixTranscriptToken(textTokenId: 91, decodedInPulse: pulseId, text: "n", committed: false),
        ],
        pulseAttentionHeatmap: heatmap,
        encoderFrontier: sampleHelixEncoderFrontier(),
        streamMetrics: metrics,
        verifyEvidence: sampleHelixVerifyEvidence(),
        decoderEvidenceReport: HelixDecoderEvidenceReport(
            totalBatches: 17,
            batchesWithoutDecoderEvidence: 1,
            pulsesWithoutDecoderEvidence: [16],
            variantEvidenceCounts: HelixDecoderEvidenceVariantCounts(decode: 12, verifyPrediction: 6, verifySeed: 3, promptPrefill: 4),
            variantRecordCounts: HelixDecoderEvidenceVariantCounts(decode: 96, verifyPrediction: 48, verifySeed: 24, promptPrefill: 32),
            observedDecoderLayerIndices: [0, 1, 7],
            observedDecoderHeadIndices: [0, 2, 3]
        ),
        pulseEvidenceSnapshot: sampleHelixPulseEvidence(),
        gpuChromeEventsForPulse: sampleHelixChromeEvents(),
        runInfo: runInfo,
        pieceEvalReference: HelixPieceEvalReference(piece: "ceramic", language: "en", words: ["phon", "surface"]),
        pieceEvalForPulse: sampleHelixPieceEval(),
        encoderProvenanceReport: sampleHelixEncoderProvenanceReport(),
        pulseBundleFields: HelixPulseBundleFields(
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
        ),
        pulseBundle: sampleHelixPulseBundle(pulseId: pulseId, promptLayout: promptLayout, heatmap: heatmap, rollup: rollup),
        pulseAvailable: HelixPulseAvailable(pulseId: pulseId)
    )
}

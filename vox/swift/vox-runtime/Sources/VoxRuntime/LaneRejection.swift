import Foundation

public let voxLaneRejectReasonMetadataKey = "vox-lane-reject-reason"
public let voxLaneRejectMessageMetadataKey = "vox-lane-reject-message"

public enum LaneRejectReason: String, CaseIterable, Sendable {
    case unknownService = "unknown-service"
    case forbidden
    case notReady = "not-ready"
    case draining
    case schemaIncompatible = "schema-incompatible"
    case policyRejected = "policy-rejected"
}

// r[impl lane.open.result]
public struct LaneRejection: Sendable {
    public let reason: LaneRejectReason
    public let metadata: Metadata

    private init(reason: LaneRejectReason, metadata: Metadata) {
        self.reason = reason
        self.metadata = metadata
    }

    public static func new(_ reason: LaneRejectReason) -> LaneRejection {
        withMetadata(reason)
    }

    public static func withMessage(
        _ reason: LaneRejectReason,
        _ message: String
    ) -> LaneRejection {
        var metadata = emptyMetadata()
        metadata.metaSet(voxLaneRejectMessageMetadataKey, .string(message))
        return withMetadata(reason, metadata)
    }

    public static func withMetadata(
        _ reason: LaneRejectReason,
        _ metadata: Metadata = emptyMetadata()
    ) -> LaneRejection {
        var next = metadata
        next.metaSet(voxLaneRejectReasonMetadataKey, .string(reason.rawValue))
        return LaneRejection(reason: reason, metadata: next)
    }

    public static func fromMetadata(_ metadata: Metadata) -> LaneRejection {
        let rawReason = metadata.metaStr(voxLaneRejectReasonMetadataKey)
        let reason = rawReason.flatMap(LaneRejectReason.init(rawValue:)) ?? .policyRejected
        return withMetadata(reason, metadata)
    }

    public func message() -> String? {
        metadata.metaStr(voxLaneRejectMessageMetadataKey) ?? metadata.metaStr("error")
    }

    public func toMetadata() -> Metadata {
        metadata
    }
}

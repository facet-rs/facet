import Foundation

private let retryMetadataFlagsNone: UInt64 = 0

public let retrySupportMetadataKey = "roam-retry-support"
public let operationIdMetadataKey = "roam-operation-id"
public let retrySupportVersion: UInt64 = 1

public func appendRetrySupportMetadata(_ metadata: [MetadataEntryV7]) -> [MetadataEntryV7] {
    guard !metadataSupportsRetry(metadata) else {
        return metadata
    }
    return metadata + [
        MetadataEntryV7(
            key: retrySupportMetadataKey,
            value: .u64(retrySupportVersion),
            flags: retryMetadataFlagsNone
        )
    ]
}

public func metadataSupportsRetry(_ metadata: [MetadataEntryV7]) -> Bool {
    metadata.contains { entry in
        entry.key == retrySupportMetadataKey
            && entry.value == .u64(retrySupportVersion)
    }
}

public func metadataOperationId(_ metadata: [MetadataEntryV7]) -> UInt64? {
    for entry in metadata where entry.key == operationIdMetadataKey {
        if case .u64(let operationId) = entry.value {
            return operationId
        }
        return nil
    }
    return nil
}

public func ensureOperationId(
    _ metadata: [MetadataEntryV7],
    operationId: UInt64
) -> [MetadataEntryV7] {
    guard metadataOperationId(metadata) == nil else {
        return metadata
    }
    return metadata + [
        MetadataEntryV7(
            key: operationIdMetadataKey,
            value: .u64(operationId),
            flags: retryMetadataFlagsNone
        )
    ]
}

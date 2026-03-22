import Foundation

private let retryMetadataFlagsNone: UInt64 = 0

public let retrySupportMetadataKey = "roam-retry-support"
public let operationIdMetadataKey = "roam-operation-id"
public let retrySupportVersion: UInt64 = 1

public func appendRetrySupportMetadata(_ metadata: [MetadataEntry]) -> [MetadataEntry] {
    guard !metadataSupportsRetry(metadata) else {
        return metadata
    }
    return metadata + [
        MetadataEntry(
            key: retrySupportMetadataKey,
            value: .u64(retrySupportVersion),
            flags: retryMetadataFlagsNone
        )
    ]
}

public func metadataSupportsRetry(_ metadata: [MetadataEntry]) -> Bool {
    metadata.contains { entry in
        entry.key == retrySupportMetadataKey
            && entry.value == .u64(retrySupportVersion)
    }
}

public func metadataOperationId(_ metadata: [MetadataEntry]) -> UInt64? {
    for entry in metadata where entry.key == operationIdMetadataKey {
        if case .u64(let operationId) = entry.value {
            return operationId
        }
        return nil
    }
    return nil
}

public func ensureOperationId(
    _ metadata: [MetadataEntry],
    operationId: UInt64
) -> [MetadataEntry] {
    guard metadataOperationId(metadata) == nil else {
        return metadata
    }
    return metadata + [
        MetadataEntry(
            key: operationIdMetadataKey,
            value: .u64(operationId),
            flags: retryMetadataFlagsNone
        )
    ]
}

public func replacingOperationId(
    _ metadata: [MetadataEntry],
    operationId: UInt64
) -> [MetadataEntry] {
    metadata.filter { $0.key != operationIdMetadataKey } + [
        MetadataEntry(
            key: operationIdMetadataKey,
            value: .u64(operationId),
            flags: retryMetadataFlagsNone
        )
    ]
}

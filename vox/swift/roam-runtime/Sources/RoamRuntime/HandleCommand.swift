import Foundation

/// Commands from ConnectionHandle to Driver.
enum HandleCommand: Sendable {
    case call(
        requestId: UInt64,
        methodId: UInt64,
        metadata: [MetadataEntryV7],
        payload: [UInt8],
        channels: [UInt64],
        retry: RetryPolicy,
        timeout: TimeInterval?,
        prepareRetry: (@Sendable () async -> PreparedRetryRequest)?,
        responseTx: @Sendable (Result<[UInt8], ConnectionError>) -> Void
    )
}

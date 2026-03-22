import Foundation

/// Commands from ConnectionHandle to Driver.
enum HandleCommand: Sendable {
    case call(
        requestId: UInt64,
        methodId: UInt64,
        metadata: [MetadataEntry],
        payload: [UInt8],
        retry: RetryPolicy,
        timeout: TimeInterval?,
        prepareRetry: (@Sendable () async -> PreparedRetryRequest)?,
        responseTx: @Sendable (Result<[UInt8], ConnectionError>) -> Void
    )
}

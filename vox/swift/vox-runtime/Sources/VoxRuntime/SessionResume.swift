import Foundation

private let sessionResumeMetadataFlagsNone: UInt64 = 0

public let sessionResumeKeyMetadataKey = "vox-session-key"

public func appendSessionResumeKeyMetadata(
    _ metadata: [MetadataEntry],
    key: [UInt8]
) -> [MetadataEntry] {
    metadata + [
        MetadataEntry(
            key: sessionResumeKeyMetadataKey,
            value: .bytes(key),
            flags: sessionResumeMetadataFlagsNone
        )
    ]
}

public func metadataSessionResumeKey(_ metadata: [MetadataEntry]) -> [UInt8]? {
    for entry in metadata where entry.key == sessionResumeKeyMetadataKey {
        if case .bytes(let key) = entry.value, key.count == 16 {
            return key
        }
    }
    return nil
}

func freshSessionResumeKey() -> [UInt8] {
    var generator = SystemRandomNumberGenerator()
    return (0..<16).map { _ in UInt8.random(in: UInt8.min...UInt8.max, using: &generator) }
}

func sessionResumeKeysEqual(_ lhs: [UInt8], _ rhs: [UInt8]) -> Bool {
    lhs == rhs
}

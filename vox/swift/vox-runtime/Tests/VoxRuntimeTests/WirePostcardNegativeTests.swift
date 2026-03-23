#if os(macOS)
import Foundation
import Testing

@testable import VoxRuntime

private func samePostcardError(_ lhs: PostcardError, _ rhs: PostcardError) -> Bool {
    switch (lhs, rhs) {
    case (.truncated, .truncated), (.invalidUtf8, .invalidUtf8), (.unknownVariant, .unknownVariant),
         (.overflow, .overflow):
        return true
    default:
        return false
    }
}

private func expectWireError(_ expected: WireError, _ body: () throws -> Void) {
    do {
        try body()
        Issue.record("expected WireError \(expected)")
    } catch let actual as WireError {
        #expect(actual == expected)
    } catch {
        Issue.record("expected WireError \(expected), got \(error)")
    }
}

private func expectPostcardError(_ expected: PostcardError, _ body: () throws -> Void) {
    do {
        try body()
        Issue.record("expected PostcardError \(expected)")
    } catch let actual as PostcardError {
        #expect(samePostcardError(actual, expected))
    } catch {
        Issue.record("expected PostcardError \(expected), got \(error)")
    }
}

@Suite(.serialized)
struct WirePostcardNegativeTests {
    // r[verify session.message]
    // r[verify session.message.payloads]
    @Test func wireDecodeRejectsTrailingBytes() {
        var bytes = Message.protocolError(description: "bad frame sequence").encode()
        bytes.append(0xFF)
        expectWireError(.trailingBytes) {
            _ = try Message.decode(from: Data(bytes))
        }
    }

    // r[verify session.message]
    // r[verify session.message.payloads]
    @Test func wireDecodeRejectsUnknownPayloadVariant() {
        expectWireError(.unknownVariant(11)) {
            _ = try Message.decode(from: Data([0, 11]))
        }
    }

    // r[verify session.message]
    // r[verify session.message.payloads]
    @Test func wireDecodeRejectsTruncatedStringField() {
        let truncatedProtocolError: [UInt8] = [0, 0, 3, 0x41, 0x42]
        expectWireError(.truncated) {
            _ = try Message.decode(from: Data(truncatedProtocolError))
        }
    }

    // r[verify session.message]
    // r[verify session.message.payloads]
    @Test func wireDecodeRejectsInvalidParityVariant() {
        let bytes: [UInt8] = [0, 1, 3, 1, 0]
        expectWireError(.unknownVariant(3)) {
            _ = try Message.decode(from: Data(bytes))
        }
    }

    // r[verify session.message]
    // r[verify session.message.payloads]
    @Test func wireDecodeRejectsOverflowingU32Field() {
        var bytes: [UInt8] = []
        bytes += encodeVarint(0) // connection_id
        bytes += encodeVarint(1) // payload: connection_open
        bytes += encodeVarint(0) // parity: odd
        bytes += [0x80, 0x80, 0x80, 0x80, 0x10] // max_concurrent_requests > u32
        bytes += encodeVarint(0) // metadata: empty vec

        expectWireError(.overflow) {
            _ = try Message.decode(from: Data(bytes))
        }
    }

    // r[verify session.message]
    // r[verify session.message.payloads]
    @Test func wireDecodeRejectsInvalidUtf8InStringField() {
        let bytes: [UInt8] = [0, 0, 2, 0xC3, 0x28]
        expectWireError(.invalidUtf8) {
            _ = try Message.decode(from: Data(bytes))
        }
    }

    // r[verify rpc.channel.payload-encoding]
    @Test func postcardDecodeBytesRejectsTruncatedLength() {
        var offset = 0
        expectPostcardError(.truncated) {
            _ = try decodeBytes(from: Data([4, 1, 2]), offset: &offset)
        }
    }

    // r[verify rpc.channel.payload-encoding]
    @Test func postcardDecodeStringRejectsInvalidUtf8() {
        var offset = 0
        expectPostcardError(.invalidUtf8) {
            _ = try decodeString(from: Data([2, 0xC3, 0x28]), offset: &offset)
        }
    }

    // r[verify rpc.channel.payload-encoding]
    @Test func postcardDecodeU32RejectsOverflow() {
        var offset = 0
        expectPostcardError(.overflow) {
            _ = try decodeU32(from: Data([0x80, 0x80, 0x80, 0x80, 0x10]), offset: &offset)
        }
    }

    // r[verify rpc.fallible.vox-error]
    @Test func rpcErrorCodeRejectsUnknownDiscriminant() {
        #expect(RpcErrorCode(rawValue: 9) == nil)
    }

    // r[verify rpc.fallible.vox-error]
    @Test func rpcErrorCodeKnownDiscriminantsRoundTrip() {
        #expect(RpcErrorCode(rawValue: 0) == .user)
        #expect(RpcErrorCode(rawValue: 1) == .unknownMethod)
        #expect(RpcErrorCode(rawValue: 2) == .invalidPayload)
        #expect(RpcErrorCode(rawValue: 3) == .cancelled)
    }
}
#endif

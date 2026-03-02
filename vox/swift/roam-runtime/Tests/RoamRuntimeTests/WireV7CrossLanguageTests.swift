#if os(macOS)
import Foundation
import Testing

@testable import RoamRuntime

private func loadWireV7Fixture(_ name: String) throws -> [UInt8] {
    let testFile = URL(fileURLWithPath: #filePath)
    let projectRoot =
        testFile
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
    let path = projectRoot.appendingPathComponent("test-fixtures/golden-vectors/wire-v7/\(name).bin")
    return Array(try Data(contentsOf: path))
}

struct WireV7CrossLanguageTests {
    private func assertSampleMetadata(_ metadata: [MetadataEntryV7]) {
        #expect(metadata.count == 3)
        #expect(metadata[0].key == "trace-id")
        #expect(metadata[1].key == "auth")
        #expect(metadata[2].key == "attempt")

        guard case .string("abc123") = metadata[0].value else {
            Issue.record("expected trace-id metadata string")
            return
        }
        guard case .bytes(let auth) = metadata[1].value else {
            Issue.record("expected auth metadata bytes")
            return
        }
        #expect(auth == [0xDE, 0xAD, 0xBE, 0xEF])
        guard case .u64(2) = metadata[2].value else {
            Issue.record("expected attempt metadata u64")
            return
        }
    }

    private func assertFixtureDecoded(name: String, _ msg: MessageV7) {
        switch name {
        case "message_hello":
            #expect(msg.connectionId == 0)
            guard case .hello(let hello) = msg.payload else {
                Issue.record("expected hello payload")
                return
            }
            #expect(hello.version == 7)
            #expect(hello.connectionSettings.parity == .odd)
            #expect(hello.connectionSettings.maxConcurrentRequests == 64)
            assertSampleMetadata(hello.metadata)

        case "message_hello_yourself":
            #expect(msg.connectionId == 0)
            guard case .helloYourself(let hello) = msg.payload else {
                Issue.record("expected helloYourself payload")
                return
            }
            #expect(hello.connectionSettings.parity == .even)
            #expect(hello.connectionSettings.maxConcurrentRequests == 32)
            assertSampleMetadata(hello.metadata)

        case "message_protocol_error":
            #expect(msg.connectionId == 0)
            guard case .protocolError(let err) = msg.payload else {
                Issue.record("expected protocolError payload")
                return
            }
            #expect(err.description == "bad frame sequence")

        case "message_connection_open":
            #expect(msg.connectionId == 2)
            guard case .connectionOpen(let open) = msg.payload else {
                Issue.record("expected connectionOpen payload")
                return
            }
            #expect(open.connectionSettings.parity == .odd)
            #expect(open.connectionSettings.maxConcurrentRequests == 64)
            assertSampleMetadata(open.metadata)

        case "message_connection_accept":
            #expect(msg.connectionId == 2)
            guard case .connectionAccept(let accept) = msg.payload else {
                Issue.record("expected connectionAccept payload")
                return
            }
            #expect(accept.connectionSettings.parity == .even)
            #expect(accept.connectionSettings.maxConcurrentRequests == 96)
            assertSampleMetadata(accept.metadata)

        case "message_connection_reject":
            #expect(msg.connectionId == 4)
            guard case .connectionReject(let reject) = msg.payload else {
                Issue.record("expected connectionReject payload")
                return
            }
            assertSampleMetadata(reject.metadata)

        case "message_connection_close":
            #expect(msg.connectionId == 2)
            guard case .connectionClose(let close) = msg.payload else {
                Issue.record("expected connectionClose payload")
                return
            }
            assertSampleMetadata(close.metadata)

        case "message_request_call":
            #expect(msg.connectionId == 2)
            guard case .requestMessage(let req) = msg.payload,
                case .call(let call) = req.body
            else {
                Issue.record("expected request/call payload")
                return
            }
            #expect(req.id == 11)
            #expect(call.methodId == 0xE5A1_D6B2_C390_F001)
            #expect(call.channels == [3, 5])
            #expect(call.args.bytes == [0xF8, 0xAC, 0xD1, 0x91, 0x01])
            assertSampleMetadata(call.metadata)

        case "message_request_response":
            #expect(msg.connectionId == 2)
            guard case .requestMessage(let req) = msg.payload,
                case .response(let response) = req.body
            else {
                Issue.record("expected request/response payload")
                return
            }
            #expect(req.id == 11)
            #expect(response.channels == [7])
            #expect(response.ret.bytes == [0x8C, 0xE0, 0xBA, 0xD6, 0x0F])
            assertSampleMetadata(response.metadata)

        case "message_request_cancel":
            #expect(msg.connectionId == 2)
            guard case .requestMessage(let req) = msg.payload,
                case .cancel(let cancel) = req.body
            else {
                Issue.record("expected request/cancel payload")
                return
            }
            #expect(req.id == 11)
            assertSampleMetadata(cancel.metadata)

        case "message_channel_item":
            #expect(msg.connectionId == 2)
            guard case .channelMessage(let channel) = msg.payload,
                case .item(let item) = channel.body
            else {
                Issue.record("expected channel/item payload")
                return
            }
            #expect(channel.id == 3)
            #expect(item.item.bytes == [0x4D])

        case "message_channel_close":
            #expect(msg.connectionId == 2)
            guard case .channelMessage(let channel) = msg.payload,
                case .close(let close) = channel.body
            else {
                Issue.record("expected channel/close payload")
                return
            }
            #expect(channel.id == 3)
            assertSampleMetadata(close.metadata)

        case "message_channel_reset":
            #expect(msg.connectionId == 2)
            guard case .channelMessage(let channel) = msg.payload,
                case .reset(let reset) = channel.body
            else {
                Issue.record("expected channel/reset payload")
                return
            }
            #expect(channel.id == 3)
            assertSampleMetadata(reset.metadata)

        case "message_channel_grant_credit":
            #expect(msg.connectionId == 2)
            guard case .channelMessage(let channel) = msg.payload,
                case .grantCredit(let credit) = channel.body
            else {
                Issue.record("expected channel/grantCredit payload")
                return
            }
            #expect(channel.id == 3)
            #expect(credit.additional == 1024)

        default:
            Issue.record("unexpected fixture name \(name)")
        }
    }

    // r[verify session.message]
    // r[verify session.message.payloads]
    // r[verify session.connection-settings.hello]
    @Test func rustV7HelloFixtureRoundTripsInV7Codec() throws {
        let bytes = try loadWireV7Fixture("message_hello")
        let decoded = try MessageV7.decode(from: Data(bytes))
        #expect(decoded.connectionId == 0)
        guard case .hello(let hello) = decoded.payload else {
            Issue.record("expected hello payload")
            return
        }
        #expect(hello.version == 7)
        #expect(hello.connectionSettings.parity == .odd)
        #expect(hello.connectionSettings.maxConcurrentRequests == 64)
        #expect(decoded.encode() == bytes)
    }

    // r[verify session.message]
    // r[verify session.message.payloads]
    @Test func rustV7NestedPayloadFixturesRoundTrip() throws {
        let names = [
            "message_hello",
            "message_hello_yourself",
            "message_protocol_error",
            "message_connection_open",
            "message_connection_accept",
            "message_connection_reject",
            "message_connection_close",
            "message_request_call",
            "message_request_response",
            "message_request_cancel",
            "message_channel_item",
            "message_channel_close",
            "message_channel_reset",
            "message_channel_grant_credit",
        ]

        for name in names {
            let bytes = try loadWireV7Fixture(name)
            #expect(!bytes.isEmpty)
            let decoded = try MessageV7.decode(from: Data(bytes))
            #expect(decoded.encode() == bytes, "\(name) failed byte-for-byte round trip")
            assertFixtureDecoded(name: name, decoded)
        }
    }
}
#endif

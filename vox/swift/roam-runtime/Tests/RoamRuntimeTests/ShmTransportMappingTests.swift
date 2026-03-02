#if os(macOS)
import Foundation
import Testing

@testable import RoamRuntime

private func assertShmRoundtrip(_ msg: MessageV7) throws {
    let frame = try messageToShmFrame(msg)
    let decoded = try shmFrameToMessage(frame)
    #expect(decoded.encode() == msg.encode())
}

struct ShmTransportMappingTests {
    // r[verify transport.shm]
    // r[verify zerocopy.framing.link.shm]
    @Test func requestResponseRoundtrip() throws {
        let request = MessageV7.request(
            connId: 7,
            requestId: 42,
            methodId: 99,
            metadata: [MetadataEntryV7(key: "k", value: .string("v"), flags: 1)],
            channels: [3, 4],
            payload: [1, 2, 3]
        )
        try assertShmRoundtrip(request)

        let response = MessageV7.response(
            connId: 7,
            requestId: 42,
            metadata: [MetadataEntryV7(key: "status", value: .u64(200), flags: 0)],
            channels: [8],
            payload: [9, 8, 7]
        )
        try assertShmRoundtrip(response)
    }

    // r[verify transport.shm]
    @Test func channelControlRoundtrip() throws {
        try assertShmRoundtrip(.cancel(connId: 9, requestId: 11))
        try assertShmRoundtrip(.data(connId: 9, channelId: 15, payload: [4, 5, 6]))
        try assertShmRoundtrip(.close(connId: 9, channelId: 15))
        try assertShmRoundtrip(.reset(connId: 9, channelId: 15))
        try assertShmRoundtrip(.protocolError(description: "bye"))
    }

    // r[verify transport.shm]
    @Test func virtualConnectionControlRoundtrip() throws {
        try assertShmRoundtrip(
            .connectionOpen(
                connId: 33,
                settings: ConnectionSettingsV7(parity: .odd, maxConcurrentRequests: 8),
                metadata: [MetadataEntryV7(key: "a", value: .bytes([1]), flags: 0)]
            ))
        try assertShmRoundtrip(
            .connectionAccept(
                connId: 17,
                settings: ConnectionSettingsV7(parity: .even, maxConcurrentRequests: 8),
                metadata: [MetadataEntryV7(key: "b", value: .u64(2), flags: 0)]
            ))
        try assertShmRoundtrip(
            .connectionReject(
                connId: 33,
                metadata: [MetadataEntryV7(key: "c", value: .string("x"), flags: 0)]
            ))
    }

    // r[verify transport.shm]
    @Test func helloAndCreditRoundtrip() throws {
        try assertShmRoundtrip(
            .hello(
                HelloV7(
                    version: 7,
                    connectionSettings: ConnectionSettingsV7(parity: .odd, maxConcurrentRequests: 64),
                    metadata: []
                )))
        try assertShmRoundtrip(.credit(connId: 0, channelId: 1, bytes: 64))
    }
}
#endif

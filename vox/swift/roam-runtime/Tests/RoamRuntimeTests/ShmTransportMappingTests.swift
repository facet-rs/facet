#if os(macOS)
import Foundation
import Testing

@testable import RoamRuntime

private func assertShmRoundtrip(_ msg: Message) throws {
    let frame = try messageToShmFrame(msg)
    let decoded = try shmFrameToMessage(frame)
    #expect(decoded.encode() == msg.encode())
}

struct ShmTransportMappingTests {
    @Test func requestResponseRoundtrip() throws {
        let request = Message.request(
            connId: 7,
            requestId: 42,
            methodId: 99,
            metadata: [(key: "k", value: .string("v"), flags: 1)],
            channels: [3, 4],
            payload: [1, 2, 3]
        )
        try assertShmRoundtrip(request)

        let response = Message.response(
            connId: 7,
            requestId: 42,
            metadata: [(key: "status", value: .u64(200), flags: 0)],
            channels: [8],
            payload: [9, 8, 7]
        )
        try assertShmRoundtrip(response)
    }

    @Test func channelControlRoundtrip() throws {
        try assertShmRoundtrip(.cancel(connId: 9, requestId: 11))
        try assertShmRoundtrip(.data(connId: 9, channelId: 15, payload: [4, 5, 6]))
        try assertShmRoundtrip(.close(connId: 9, channelId: 15))
        try assertShmRoundtrip(.reset(connId: 9, channelId: 15))
        try assertShmRoundtrip(.goodbye(connId: 0, reason: "bye"))
    }

    @Test func virtualConnectionControlRoundtrip() throws {
        try assertShmRoundtrip(.connect(requestId: 33, metadata: [(key: "a", value: .bytes([1]), flags: 0)]))
        try assertShmRoundtrip(
            .accept(
                requestId: 33,
                connId: 17,
                metadata: [(key: "b", value: .u64(2), flags: 0)]
            ))
        try assertShmRoundtrip(
            .reject(
                requestId: 33,
                reason: "nope",
                metadata: [(key: "c", value: .string("x"), flags: 0)]
            ))
    }

    @Test func helloAndCreditAreRejected() throws {
        #expect(throws: ShmTransportConvertError.helloNotSupported) {
            _ = try messageToShmFrame(.hello(defaultHello()))
        }
        #expect(throws: ShmTransportConvertError.creditNotSupported) {
            _ = try messageToShmFrame(.credit(connId: 0, channelId: 1, bytes: 64))
        }
    }
}
#endif

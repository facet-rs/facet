import Testing

@testable import VoxRuntime

@Suite(.serialized)
struct FfiTransportTests {
    // r[verify link.message]
    // r[verify link.order]
    // r[verify link.rx.recv]
    @Test func swiftEndpointsRoundTripWhenAInitiates() async throws {
        let endpointA = FfiEndpoint()
        let endpointB = FfiEndpoint()

        let linkA = try endpointA.connect(peer: endpointB.exportedVtable())
        let linkB = try await endpointB.accept()

        try await linkA.sendFrame([1, 2, 3])
        #expect(endpointA.outstandingLoanCount() == 1)
        #expect(try await linkB.recvFrame() == [1, 2, 3])
        #expect(endpointA.outstandingLoanCount() == 0)

        try await linkA.sendFrame([])
        try await linkA.sendFrame([4, 5])
        #expect(try await linkB.recvFrame() == [])
        #expect(try await linkB.recvFrame() == [4, 5])

        try await linkB.sendFrame([9, 8, 7])
        #expect(endpointB.outstandingLoanCount() == 1)
        #expect(try await linkA.recvFrame() == [9, 8, 7])
        #expect(endpointB.outstandingLoanCount() == 0)
    }

    // r[verify link.message]
    // r[verify link.order]
    @Test func swiftEndpointsRoundTripWhenBInitiates() async throws {
        let endpointA = FfiEndpoint()
        let endpointB = FfiEndpoint()

        let linkB = try endpointB.connect(peer: endpointA.exportedVtable())
        let linkA = try await endpointA.accept()

        try await linkB.sendFrame([42])
        #expect(endpointB.outstandingLoanCount() == 1)
        #expect(try await linkA.recvFrame() == [42])
        #expect(endpointB.outstandingLoanCount() == 0)

        try await linkA.sendFrame([24, 12])
        #expect(endpointA.outstandingLoanCount() == 1)
        #expect(try await linkB.recvFrame() == [24, 12])
        #expect(endpointA.outstandingLoanCount() == 0)
    }
}

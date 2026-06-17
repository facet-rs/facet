import Foundation
import PhonSchema
import Testing
import VoxRuntime

// Oracle for the cross-language schema-advertisement bug: encode a `.response`
// Message carrying a non-empty `schemas` closure, then decode it the way a *peer*
// would — via `buildMessageDecoder` keyed on our OWN advertised `MessageSchemaClosure`
// (the writer schema we hand peers at handshake). If `schemas` survives this round
// trip, the Message envelope codec is sound and the loss is in the driver path.

@Test func responseSchemasSurviveEnvelopeRoundTrip() throws {
    let schemaBytes: [UInt8] = Array(1...40)
    let retBytes: [UInt8] = [9, 8, 7, 6, 5]
    let msg = Message(
        laneId: 0,
        payload: .requestMessage(
            RequestMessage(
                id: 42,
                body: .response(
                    RequestResponse(
                        metadata: .null,
                        ret: Data(retBytes),
                        schemas: Data(schemaBytes))))))

    let encoded = encodeMessage(msg)
    let decode = buildMessageDecoder(peerMessageSchema: MessageSchemaClosure)
    let decoded = try decode(encoded)

    guard case .requestMessage(let rm) = decoded.payload,
        case .response(let resp) = rm.body
    else {
        Issue.record("decoded payload was not a response")
        return
    }
    #expect(rm.id == 42)
    #expect(Array(resp.ret) == retBytes)
    #expect(Array(resp.schemas) == schemaBytes)
}

@Test func callSchemasSurviveEnvelopeRoundTrip() throws {
    let schemaBytes: [UInt8] = Array(50...90)
    let argBytes: [UInt8] = [1, 2, 3, 4]
    let msg = Message(
        laneId: 0,
        payload: .requestMessage(
            RequestMessage(
                id: 7,
                body: .call(
                    RequestCall(
                        methodId: 0xDEAD_BEEF,
                        channels: [],
                        metadata: .null,
                        args: Data(argBytes),
                        schemas: Data(schemaBytes))))))

    let encoded = encodeMessage(msg)
    let decode = buildMessageDecoder(peerMessageSchema: MessageSchemaClosure)
    let decoded = try decode(encoded)

    guard case .requestMessage(let rm) = decoded.payload,
        case .call(let call) = rm.body
    else {
        Issue.record("decoded payload was not a call")
        return
    }
    #expect(Array(call.args) == argBytes)
    #expect(Array(call.schemas) == schemaBytes)
}

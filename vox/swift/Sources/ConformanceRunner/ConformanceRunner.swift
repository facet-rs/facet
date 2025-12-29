import Foundation
import Rapace

/// Control verb constants
public enum ControlVerb: UInt32 {
    case hello = 0
    case openChannel = 1
    case closeChannel = 2
    case cancelChannel = 3
    case grantCredits = 4
    case ping = 5
    case pong = 6
    case goAway = 7
}

/// Protocol version 1.0
public let protocolVersion1_0: UInt32 = 0x00010000

/// Feature flags
public struct Features: OptionSet {
    public let rawValue: UInt64
    public init(rawValue: UInt64) { self.rawValue = rawValue }

    public static let attachedStreams = Features(rawValue: 1 << 0)
    public static let callEnvelope = Features(rawValue: 1 << 1)
    public static let creditFlowControl = Features(rawValue: 1 << 2)
    public static let rapacePing = Features(rawValue: 1 << 3)
}

/// Role in handshake
public enum Role: UInt8 {
    case initiator = 1
    case acceptor = 2
}

/// A frame with descriptor and optional external payload
public struct Frame {
    public var desc: MsgDescHot
    public var payload: Data

    public init(desc: MsgDescHot, payload: Data = Data()) {
        self.desc = desc
        self.payload = payload
    }

    /// Get the effective payload bytes
    public func payloadBytes() -> Data {
        if desc.payloadSlot == inlinePayloadSlot {
            return desc.inlinePayloadData
        } else {
            return payload
        }
    }

    /// Create an inline frame
    public static func inline(_ desc: MsgDescHot, payload: Data) -> Frame {
        var d = desc
        d.setInlinePayload(payload)
        return Frame(desc: d, payload: Data())
    }

    /// Create a frame with external payload
    public static func withPayload(_ desc: MsgDescHot, payload: Data) -> Frame {
        var d = desc
        d.payloadSlot = 0
        d.payloadLen = UInt32(payload.count)
        return Frame(desc: d, payload: payload)
    }
}

/// Errors from conformance runner
public enum ConformanceError: Error {
    case processNotStarted
    case frameTooShort(Int)
    case timeout
    case processError(String)
    case unexpectedFrame(String)
}

/// Conformance test runner that communicates with rapace-conformance binary
public class ConformanceRunner {
    private var process: Process?
    private var stdin: FileHandle?
    private var stdout: FileHandle?
    private var buffer = Data()

    public init() {}

    /// Start a conformance test case
    public func start(testCase: String, binary: String = "rapace-conformance") throws {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: binary)
        proc.arguments = ["--case", testCase]

        let stdinPipe = Pipe()
        let stdoutPipe = Pipe()

        proc.standardInput = stdinPipe
        proc.standardOutput = stdoutPipe
        proc.standardError = FileHandle.standardError

        try proc.run()

        self.process = proc
        self.stdin = stdinPipe.fileHandleForWriting
        self.stdout = stdoutPipe.fileHandleForReading
    }

    /// Send a frame to the conformance harness
    public func send(_ frame: Frame) throws {
        guard let stdin = stdin else {
            throw ConformanceError.processNotStarted
        }

        let descBytes = frame.desc.serialize()
        let externalPayload = frame.desc.payloadSlot == inlinePayloadSlot ? Data() : frame.payload

        let totalLen = UInt32(64 + externalPayload.count)

        var data = Data()
        data.append(contentsOf: withUnsafeBytes(of: totalLen.littleEndian) { Array($0) })
        data.append(descBytes)
        data.append(externalPayload)

        stdin.write(data)
    }

    /// Receive a frame from the conformance harness
    public func recv() throws -> Frame {
        guard let stdout = stdout else {
            throw ConformanceError.processNotStarted
        }

        // Read until we have at least 4 bytes for length
        while buffer.count < 4 {
            let chunk = stdout.availableData
            if chunk.isEmpty {
                throw ConformanceError.timeout
            }
            buffer.append(chunk)
        }

        // Read length
        let totalLen = buffer.withUnsafeBytes { ptr -> UInt32 in
            ptr.load(as: UInt32.self).littleEndian
        }

        if totalLen < 64 {
            throw ConformanceError.frameTooShort(Int(totalLen))
        }

        // Read until we have the full frame
        while buffer.count < 4 + Int(totalLen) {
            let chunk = stdout.availableData
            if chunk.isEmpty {
                throw ConformanceError.timeout
            }
            buffer.append(chunk)
        }

        // Parse descriptor
        let descData = buffer.subdata(in: 4..<68)
        let desc = try MsgDescHot(from: descData)

        // Get external payload if present
        let payloadLen = Int(totalLen) - 64
        let payload: Data
        if payloadLen > 0 {
            payload = buffer.subdata(in: 68..<(68 + payloadLen))
        } else {
            payload = Data()
        }

        // Remove consumed data from buffer
        buffer = buffer.subdata(in: (4 + Int(totalLen))..<buffer.count)

        return Frame(desc: desc, payload: payload)
    }

    /// Wait for the conformance process to exit
    public func finish() -> (passed: Bool, exitCode: Int32) {
        stdin?.closeFile()

        process?.waitUntilExit()
        let code = process?.terminationStatus ?? -1

        return (code == 0, code)
    }

    /// Run a complete handshake as initiator
    public func doHandshakeAsInitiator() throws {
        // Build Hello message (simplified - real impl would use postcard)
        var helloData = Data()

        // protocol_version: u32
        helloData.append(contentsOf: withUnsafeBytes(of: protocolVersion1_0.littleEndian) { Array($0) })

        // role: u8
        helloData.append(Role.initiator.rawValue)

        // required_features: u64 (varint, 0)
        helloData.append(0)

        // supported_features: u64 (varint)
        let features: UInt64 = Features.attachedStreams.rawValue | Features.callEnvelope.rawValue
        helloData.append(contentsOf: encodeVarint(features))

        // limits: max_payload_size, max_channels, max_pending_calls (varints)
        helloData.append(contentsOf: encodeVarint(UInt64(1024 * 1024)))
        helloData.append(0) // max_channels = 0
        helloData.append(0) // max_pending_calls = 0

        // methods: empty vec
        helloData.append(0)

        // params: empty vec
        helloData.append(0)

        var desc = MsgDescHot()
        desc.msgId = 1
        desc.channelId = 0
        desc.methodId = ControlVerb.hello.rawValue
        desc.flags = .control

        let frame: Frame
        if helloData.count <= 16 {
            frame = Frame.inline(desc, payload: helloData)
        } else {
            frame = Frame.withPayload(desc, payload: helloData)
        }

        try send(frame)

        // Wait for Hello response
        let response = try recv()
        guard response.desc.channelId == 0 && response.desc.methodId == ControlVerb.hello.rawValue else {
            throw ConformanceError.unexpectedFrame("Expected Hello response")
        }
    }

    private func encodeVarint(_ value: UInt64) -> [UInt8] {
        var v = value
        var result: [UInt8] = []
        while v >= 0x80 {
            result.append(UInt8((v & 0x7F) | 0x80))
            v >>= 7
        }
        result.append(UInt8(v))
        return result
    }
}

/// Run a conformance test
public func runConformanceTest(
    _ testCase: String,
    binary: String = "rapace-conformance",
    testFn: (ConformanceRunner) throws -> Void
) -> Bool {
    let runner = ConformanceRunner()

    do {
        try runner.start(testCase: testCase, binary: binary)
        try testFn(runner)
        let result = runner.finish()
        return result.passed
    } catch {
        print("Test \(testCase) failed: \(error)")
        return false
    }
}

import Foundation
import Network
import Postcard

/// Errors that can occur during RPC
public enum RapaceError: Error, CustomStringConvertible {
    case connectionFailed(String)
    case sendFailed(String)
    case receiveFailed(String)
    case invalidResponse(String)
    case serverError(String)
    case timeout

    public var description: String {
        switch self {
        case .connectionFailed(let msg): return "Connection failed: \(msg)"
        case .sendFailed(let msg): return "Send failed: \(msg)"
        case .receiveFailed(let msg): return "Receive failed: \(msg)"
        case .invalidResponse(let msg): return "Invalid response: \(msg)"
        case .serverError(let msg): return "Server error: \(msg)"
        case .timeout: return "Request timed out"
        }
    }
}

/// Result of an RPC call - either payload bytes or an error
private enum CallResult {
    case success([UInt8])
    case error(RapaceError)
}

/// A rapace RPC client over TCP with proper request/response multiplexing
public actor RapaceClient {
    private let connection: TCPConnection
    private var nextMsgId: UInt64 = 1
    private var nextChannelId: UInt32 = 1

    /// Pending requests waiting for responses, keyed by msgId
    private var pendingRequests: [UInt64: CheckedContinuation<CallResult, Never>] = [:]

    /// Reader task that continuously reads responses and dispatches them
    private var readerTask: Task<Void, Never>?

    /// Whether the client is still running
    private var isRunning: Bool = true

    public init(host: String, port: UInt16) async throws {
        self.connection = TCPConnection()
        try await connection.connect(host: host, port: port)

        // Start the reader task
        readerTask = Task { [weak self] in
            await self?.readLoop()
        }
    }

    /// Close the connection
    public func close() async {
        isRunning = false
        readerTask?.cancel()
        await connection.close()

        // Fail all pending requests
        for (_, continuation) in pendingRequests {
            continuation.resume(returning: .error(.connectionFailed("Client closed")))
        }
        pendingRequests.removeAll()
    }

    /// Read loop that continuously reads frames and dispatches to waiting callers
    private func readLoop() async {
        while isRunning && !Task.isCancelled {
            do {
                // Read frame length (4 bytes)
                let respLenData = try await connection.receive(exactly: 4)
                let respLen = respLenData.withUnsafeBytes { $0.load(as: UInt32.self).littleEndian }

                // Read the rest of the frame
                let respFrame = try await connection.receive(exactly: Int(respLen))

                // Parse response descriptor (first 64 bytes)
                guard respFrame.count >= 64 else {
                    print("[RapaceClient] Response frame too short: \(respFrame.count) bytes")
                    continue
                }

                let respDesc = try MsgDescHot(from: respFrame.prefix(64))
                let msgId = respDesc.msgId

                // Find the pending request for this msgId
                guard let continuation = pendingRequests.removeValue(forKey: msgId) else {
                    print("[RapaceClient] Received response for unknown msgId: \(msgId)")
                    continue
                }

                // Check for error flag
                if respDesc.flags.contains(.error) {
                    continuation.resume(returning: .error(.serverError("Server returned error flag")))
                    continue
                }

                // Extract payload
                let payloadLen = Int(respDesc.payloadLen)
                if payloadLen == 0 {
                    continuation.resume(returning: .success([]))
                    continue
                }

                let result: [UInt8]
                if respDesc.isInline {
                    // Payload is in inline_payload field
                    result = Array(respDesc.inlinePayloadData.prefix(payloadLen))
                } else {
                    // Payload is after the 64-byte descriptor
                    let payloadStart = 64
                    let payloadEnd = payloadStart + payloadLen
                    guard respFrame.count >= payloadEnd else {
                        continuation.resume(returning: .error(.invalidResponse("Response payload truncated")))
                        continue
                    }
                    result = Array(respFrame[payloadStart..<payloadEnd])
                }

                continuation.resume(returning: .success(result))

            } catch {
                if isRunning {
                    print("[RapaceClient] Read error: \(error)")
                }
                // On error, fail all pending requests and stop
                for (_, continuation) in pendingRequests {
                    continuation.resume(returning: .error(.receiveFailed(error.localizedDescription)))
                }
                pendingRequests.removeAll()
                break
            }
        }
    }

    /// Call an RPC method with raw request bytes, returning raw response bytes
    public func call(methodId: UInt32, requestPayload: [UInt8]) async throws -> [UInt8] {
        // Allocate message and channel IDs
        let msgId = nextMsgId
        nextMsgId += 1
        let channelId = nextChannelId
        nextChannelId += 1

        // Build the request descriptor
        var desc = MsgDescHot()
        desc.msgId = msgId
        desc.channelId = channelId
        desc.methodId = methodId
        desc.flags = FrameFlags.data  // DATA flag

        // Set payload length - payload is ALWAYS sent after descriptor on the wire
        desc.payloadSlot = inlinePayloadSlot
        desc.payloadLen = UInt32(requestPayload.count)

        // Serialize the frame
        // Wire format: [4-byte frame_len][64-byte desc][payload bytes]
        // frame_len = 64 + payload.len() (payload is always external on wire)
        let descData = desc.serialize()
        let frameLen = UInt32(64 + requestPayload.count)

        // Build the full frame: [4-byte length][64-byte desc][payload]
        var frame = Data()
        var len = frameLen.littleEndian
        frame.append(Data(bytes: &len, count: 4))
        frame.append(descData)
        if !requestPayload.isEmpty {
            frame.append(contentsOf: requestPayload)
        }

        // Register our continuation BEFORE sending to avoid race with reader
        let result: CallResult = await withCheckedContinuation { continuation in
            pendingRequests[msgId] = continuation

            // Send must happen after registering, but we can't await here
            // So we spawn a task to send
            Task {
                do {
                    try await self.sendFrame(frame)
                } catch {
                    // If send fails, remove from pending and resume with error
                    await self.handleSendError(msgId: msgId, error: error)
                }
            }
        }

        switch result {
        case .success(let bytes):
            return bytes
        case .error(let error):
            throw error
        }
    }

    /// Send a frame (separate method to be called from task)
    private func sendFrame(_ frame: Data) async throws {
        try await connection.send(frame)
    }

    /// Handle send error by failing the pending request
    private func handleSendError(msgId: UInt64, error: Error) {
        if let continuation = pendingRequests.removeValue(forKey: msgId) {
            continuation.resume(returning: .error(.sendFailed(error.localizedDescription)))
        }
    }
}

// MARK: - Helper to compute method IDs (FNV-1a)

/// Compute a rapace method ID from service and method names
public func computeMethodId(service: String, method: String) -> UInt32 {
    let fullName = "\(service).\(method)"

    // FNV-1a 64-bit
    var hash: UInt64 = 0xcbf29ce484222325
    let prime: UInt64 = 0x100000001b3

    for byte in fullName.utf8 {
        hash ^= UInt64(byte)
        hash = hash &* prime
    }

    // Fold to 32 bits
    return UInt32(truncatingIfNeeded: (hash >> 32) ^ hash)
}

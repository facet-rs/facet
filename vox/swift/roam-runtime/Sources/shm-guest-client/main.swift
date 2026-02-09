import Foundation
import RoamRuntime

final class InteropCounter: @unchecked Sendable {
    private let lock = NSLock()
    private var count = 0

    func increment() -> Int {
        lock.lock()
        defer { lock.unlock() }
        count += 1
        return count
    }

    func current() -> Int {
        lock.lock()
        defer { lock.unlock() }
        return count
    }
}

struct InteropDispatcher: ServiceDispatcher {
    let counter: InteropCounter

    func preregister(methodId: UInt64, payload: [UInt8], channels: [UInt64], registry: ChannelRegistry) async {
        _ = methodId
        _ = payload
        _ = channels
        _ = registry
    }

    func dispatch(
        methodId: UInt64,
        payload: [UInt8],
        channels: [UInt64],
        requestId: UInt64,
        registry: ChannelRegistry,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async {
        _ = registry
        _ = channels
        switch methodId {
        case 1:
            let input = String(decoding: payload, as: UTF8.self)
            let response = Array("swift-driver:\(input)".utf8)
            taskTx(.response(requestId: requestId, payload: response))

        case 2:
            if payload.count >= 8 {
                let channelId = payload.prefix(8).enumerated().reduce(UInt64(0)) { acc, elem in
                    acc | (UInt64(elem.element) << (elem.offset * 8))
                }
                taskTx(.data(channelId: channelId, payload: Array("swift-channel".utf8)))
                taskTx(.close(channelId: channelId))
                taskTx(.response(requestId: requestId, payload: Array("channel-ok".utf8)))
            } else {
                taskTx(.response(requestId: requestId, payload: Array("bad-channel-id".utf8)))
            }

        default:
            taskTx(.response(requestId: requestId, payload: Array("unknown-method".utf8)))
        }

        _ = counter.increment()
    }
}

struct SpawnArgs {
    let hubPath: String
    let peerId: UInt8
    let doorbellFd: Int32
    let scenario: String
}

private func fail(_ message: String) -> Never {
    fputs("shm-guest-client: \(message)\n", stderr)
    exit(1)
}

private func parseArgs(_ args: [String]) -> SpawnArgs {
    var hubPath: String?
    var peerId: UInt8?
    var doorbellFd: Int32?
    var scenario = "data-path"

    for arg in args {
        if let value = arg.split(separator: "=", maxSplits: 1).last, arg.hasPrefix("--hub-path=") {
            hubPath = String(value)
        } else if let value = arg.split(separator: "=", maxSplits: 1).last, arg.hasPrefix("--peer-id=") {
            peerId = UInt8(value)
        } else if let value = arg.split(separator: "=", maxSplits: 1).last, arg.hasPrefix("--doorbell-fd=") {
            guard let fd = Int32(value) else {
                fail("invalid --doorbell-fd value")
            }
            doorbellFd = fd
        } else if let value = arg.split(separator: "=", maxSplits: 1).last, arg.hasPrefix("--scenario=") {
            scenario = String(value)
        }
    }

    guard let hubPath else {
        fail("missing --hub-path")
    }
    guard let peerId else {
        fail("missing --peer-id")
    }
    guard let doorbellFd else {
        fail("missing --doorbell-fd")
    }

    return SpawnArgs(hubPath: hubPath, peerId: peerId, doorbellFd: doorbellFd, scenario: scenario)
}

@main
struct ShmGuestClientMain {
    static func main() async {
        let args = parseArgs(Array(CommandLine.arguments.dropFirst()))

        let ticket = ShmBootstrapTicket(peerId: args.peerId, hubPath: args.hubPath, doorbellFd: args.doorbellFd)
        let guest: ShmGuestRuntime

        do {
            guest = try ShmGuestRuntime.attach(ticket: ticket)
        } catch {
            fail("attach failed: \(error)")
        }

        switch args.scenario {
        case "data-path":
    let inlinePayload = Array("swift-inline".utf8)
    let slotPayload = (0..<2048).map { UInt8(truncatingIfNeeded: $0) }

    let inlineFrame = ShmGuestFrame(msgType: 4, id: 1, methodId: 0, payload: inlinePayload)
    let slotFrame = ShmGuestFrame(msgType: 4, id: 2, methodId: 0, payload: slotPayload)

    do {
        try guest.send(frame: inlineFrame)
        try guest.send(frame: slotFrame)
    } catch {
        fail("send failed: \(error)")
    }

    var gotInlineAck = false
    var gotSlotAck = false
    let deadline = Date().addingTimeInterval(5)

    while Date() < deadline {
        do {
            if let frame = try guest.receive() {
                if frame.id == 101, frame.payload == Array("ack-inline".utf8) {
                    gotInlineAck = true
                } else if frame.id == 102, frame.payload == Array("ack-slot".utf8) {
                    gotSlotAck = true
                }

                if gotInlineAck && gotSlotAck {
                    guest.detach()
                    print("ok")
                    exit(0)
                }
            }
        } catch {
            fail("receive failed: \(error)")
        }

        usleep(10_000)
    }

            fail("timed out waiting for host responses")

        case "remap-recv":
    var got201 = false
    var got202 = false
    let deadline = Date().addingTimeInterval(2.5)

    while Date() < deadline {
        do {
            _ = try? guest.checkRemap()
            if let frame = try guest.receive() {
                if frame.id == 201, frame.payload.count == 3000 {
                    got201 = true
                } else if frame.id == 202, frame.payload.count == 3000 {
                    got202 = true
                }

                if got201 && got202 {
                    let ack = ShmGuestFrame(
                        msgType: 4,
                        id: 777,
                        methodId: 0,
                        payload: Array("remap-recv-ok".utf8)
                    )
                    try guest.send(frame: ack)
                    guest.detach()
                    print("ok")
                    exit(0)
                }
            }
        } catch {
            fail("remap scenario failed: \(error)")
        }

        usleep(10_000)
    }

            fail("timed out waiting for remap receive scenario")

        case "remap-send":
    let firstPayload = [UInt8](repeating: 0xCD, count: 3000)
    let first = ShmGuestFrame(msgType: 4, id: 301, methodId: 0, payload: firstPayload)
    do {
        try guest.send(frame: first)
    } catch {
        fail("failed to send first remap-send payload: \(error)")
    }

    var gotStart = false
    let deadline = Date().addingTimeInterval(2.5)
    while Date() < deadline {
        do {
            _ = try? guest.checkRemap()
            if let frame = try guest.receive(),
                frame.id == 401,
                frame.payload == Array("start-second-send".utf8)
            {
                gotStart = true
                break
            }
        } catch {
            fail("failed while waiting for remap-send trigger: \(error)")
        }
        usleep(10_000)
    }

    if !gotStart {
        fail("timed out waiting for remap-send trigger")
    }

    let secondPayload = [UInt8](repeating: 0xEF, count: 3000)
    let second = ShmGuestFrame(msgType: 4, id: 302, methodId: 0, payload: secondPayload)
    do {
        try guest.send(frame: second)
    } catch {
        fail("failed to send second remap-send payload: \(error)")
    }

    var gotAck = false
    let ackDeadline = Date().addingTimeInterval(2.0)
    while Date() < ackDeadline {
        do {
            if let frame = try guest.receive(),
                frame.id == 402,
                frame.payload == Array("send-remap-ack".utf8)
            {
                gotAck = true
                break
            }
        } catch {
            fail("failed waiting for remap-send ack: \(error)")
        }
        usleep(10_000)
    }

    if !gotAck {
        fail("timed out waiting for remap-send ack")
    }

            guest.detach()
            print("ok")
            exit(0)

        case "driver-interop":
            let counter = InteropCounter()
            let dispatcher = InteropDispatcher(counter: counter)
            let transport = ShmGuestTransport(runtime: guest)
            let (_, driver) = establishShmGuest(
                transport: transport,
                dispatcher: dispatcher,
                role: .acceptor,
                acceptConnections: true
            )

            let runTask = Task {
                do {
                    try await driver.run()
                } catch {
                    // Transport closure is expected during teardown.
                }
            }

            let deadline = Date().addingTimeInterval(5)
            while Date() < deadline {
                if counter.current() >= 2 {
                    break
                }
                try? await Task.sleep(nanoseconds: 10_000_000)
            }
            if counter.current() < 2 {
                runTask.cancel()
                fail("timed out waiting for driver interop requests")
            }

            // Give the host side a short window to drain queued response/data/close frames
            // before we detach and mark the peer as goodbye.
            try? await Task.sleep(nanoseconds: 200_000_000)
            try? await transport.close()
            runTask.cancel()
            _ = await runTask.result
            print("ok")
            exit(0)

        default:
            fail("unknown scenario: \(args.scenario)")
        }
    }
}

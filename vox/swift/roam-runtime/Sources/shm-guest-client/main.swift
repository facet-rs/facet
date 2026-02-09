import Foundation
import RoamRuntime

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

default:
    fail("unknown scenario: \(args.scenario)")
}

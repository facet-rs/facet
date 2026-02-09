import Foundation
import RoamRuntime
#if os(macOS)
import Darwin
#elseif canImport(Glibc)
import Glibc
#endif

func main() -> Int32 {
    let args = CommandLine.arguments
    guard args.count == 3 else {
        fputs("usage: shm-bootstrap-client <control.sock> <sid>\n", stderr)
        return 2
    }

    let controlSock = args[1]
    let sid = args[2]

    do {
        let ticket = try requestShmBootstrapTicket(controlSocketPath: controlSock, sid: sid)
        if fcntl(ticket.doorbellFd, F_GETFD) == -1 {
            fputs("invalid received fd\n", stderr)
            return 3
        }
        if fcntl(ticket.shmFd, F_GETFD) == -1 {
            fputs("invalid received shm fd\n", stderr)
            return 3
        }

        print("peer_id=\(ticket.peerId)")
        print("hub_path=\(ticket.hubPath)")

        close(ticket.doorbellFd)
        close(ticket.shmFd)
        return 0
    } catch {
        fputs("bootstrap failed: \(error)\n", stderr)
        return 1
    }
}

exit(main())

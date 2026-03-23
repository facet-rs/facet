import Foundation
import VoxRuntime
#if os(macOS)
import Darwin
#elseif canImport(Glibc)
import Glibc
#endif

func writeStderr(_ msg: String) {
    FileHandle.standardError.write(Data(msg.utf8))
}

func main() -> Int32 {
    let args = CommandLine.arguments
    guard args.count == 3 else {
        writeStderr("usage: shm-bootstrap-client <control.sock> <sid>\n")
        return 2
    }

    let controlSock = args[1]
    let sid = args[2]

    do {
        let ticket = try requestShmBootstrapTicket(controlSocketPath: controlSock, sid: sid)
        if fcntl(ticket.doorbellFd, F_GETFD) == -1 {
            writeStderr("invalid received fd\n")
            return 3
        }
        if fcntl(ticket.shmFd, F_GETFD) == -1 {
            writeStderr("invalid received shm fd\n")
            return 3
        }

        print("peer_id=\(ticket.peerId)")
        print("hub_path=\(ticket.hubPath)")

        close(ticket.doorbellFd)
        close(ticket.shmFd)
        return 0
    } catch {
        writeStderr("bootstrap failed: \(error)\n")
        return 1
    }
}

exit(main())

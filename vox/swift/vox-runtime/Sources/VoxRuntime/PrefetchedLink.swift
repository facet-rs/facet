actor PrefetchedLink: Link {
    private let base: any Link
    private var firstRawPrologue: [UInt8]?

    init(firstRawPrologue: [UInt8], base: any Link) {
        self.base = base
        self.firstRawPrologue = firstRawPrologue
    }

    func sendFrame(_ bytes: [UInt8]) async throws {
        try await base.sendFrame(bytes)
    }

    func recvFrame() async throws -> [UInt8]? {
        if let firstRawPrologue {
            self.firstRawPrologue = nil
            return firstRawPrologue
        }
        return try await base.recvFrame()
    }

    func setMaxFrameSize(_ size: Int) async throws {
        try await base.setMaxFrameSize(size)
    }

    func close() async throws {
        try await base.close()
    }
}

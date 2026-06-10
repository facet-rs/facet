actor RequestIdAllocator {
    private var nextId: UInt64

    init(role: Role) {
        nextId = firstId(for: role)
    }

    // r[impl rpc.request.id-allocation]
    // r[impl connection.parity]
    func allocate() -> UInt64 {
        let id = nextId
        nextId += 2
        return id
    }
}

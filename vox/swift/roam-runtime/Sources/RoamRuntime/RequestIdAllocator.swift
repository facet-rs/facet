actor RequestIdAllocator {
    private var nextId: UInt64 = 1

    func allocate() -> UInt64 {
        let id = nextId
        nextId += 1
        return id
    }
}

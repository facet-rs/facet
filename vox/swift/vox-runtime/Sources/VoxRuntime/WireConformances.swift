// Hand-written conformances for generated wire types the runtime compares.
// (Codegen emits the types `Sendable`-only; these add the `Equatable` the
// settings-negotiation paths need. Kept out of the generated file.)

extension Parity: Equatable {
    public static func == (a: Parity, b: Parity) -> Bool {
        switch (a, b) {
        case (.odd, .odd), (.even, .even): return true
        default: return false
        }
    }
}

extension ConnectionSettings: Equatable {
    public static func == (a: ConnectionSettings, b: ConnectionSettings) -> Bool {
        a.parity == b.parity
            && a.maxConcurrentRequests == b.maxConcurrentRequests
            && a.initialChannelCredit == b.initialChannelCredit
    }
}

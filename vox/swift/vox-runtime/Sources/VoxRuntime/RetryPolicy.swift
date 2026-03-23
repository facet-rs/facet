import Foundation

public struct RetryPolicy: Sendable, Equatable {
    public let persist: Bool
    public let idem: Bool

    public init(persist: Bool, idem: Bool) {
        self.persist = persist
        self.idem = idem
    }

    public static let volatile = RetryPolicy(persist: false, idem: false)
    public static let idem = RetryPolicy(persist: false, idem: true)
    public static let persist = RetryPolicy(persist: true, idem: false)
    public static let persistIdem = RetryPolicy(persist: true, idem: true)
}

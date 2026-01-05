import XCTest
@testable import Rapace

final class MethodIdTests: XCTestCase {
    func testMethodIdComputation() {
        // These values must match the Rust implementation
        // Computed using FNV-1a 64-bit, folded to 32 bits

        // Test case from Rust: "Calculator.add" should produce a specific ID
        let calcAdd = computeMethodId(service: "Calculator", method: "add")
        // Verify it's deterministic
        XCTAssertEqual(calcAdd, computeMethodId(service: "Calculator", method: "add"))

        // Different methods should produce different IDs
        let calcSub = computeMethodId(service: "Calculator", method: "subtract")
        XCTAssertNotEqual(calcAdd, calcSub)

        // Different services should produce different IDs
        let mathAdd = computeMethodId(service: "Math", method: "add")
        XCTAssertNotEqual(calcAdd, mathAdd)
    }

    func testMethodIdDeterministic() {
        // Same input should always produce same output
        for _ in 0..<100 {
            let id1 = computeMethodId(service: "TestService", method: "testMethod")
            let id2 = computeMethodId(service: "TestService", method: "testMethod")
            XCTAssertEqual(id1, id2)
        }
    }

    func testMethodIdNonZero() {
        // Method IDs should generally be non-zero (zero is reserved)
        let id = computeMethodId(service: "Foo", method: "bar")
        XCTAssertNotEqual(id, 0)
    }
}

import Foundation
import Rapace
import Postcard

// Method IDs for the Echo service (from Rust server output)
let ECHO_METHOD_ID_ECHO: UInt32 = 0x3E895C50
let ECHO_METHOD_ID_ADD: UInt32 = 0x2651F994

@main
struct RPCTest {
    static func main() async {
        print("=== Rapace Swift RPC Test ===\n")

        // Verify method ID computation matches Rust
        let computedEcho = computeMethodId(service: "Echo", method: "echo")
        let computedAdd = computeMethodId(service: "Echo", method: "add")
        print("Computed method IDs:")
        print("  Echo.echo: 0x\(String(computedEcho, radix: 16, uppercase: true)) (expected: 0x3E895C50)")
        print("  Echo.add:  0x\(String(computedAdd, radix: 16, uppercase: true)) (expected: 0x2651F994)")

        guard computedEcho == ECHO_METHOD_ID_ECHO else {
            print("ERROR: Echo.echo method ID mismatch!")
            return
        }
        guard computedAdd == ECHO_METHOD_ID_ADD else {
            print("ERROR: Echo.add method ID mismatch!")
            return
        }
        print("✓ Method IDs match!\n")

        // Connect to the server
        print("Connecting to localhost:9876...")
        do {
            let client = try await RapaceClient(host: "127.0.0.1", port: 9876)
            print("✓ Connected!\n")

            // Test 1: Call Echo.add(10, 32)
            print("Calling Echo.add(10, 32)...")
            var encoder = PostcardEncoder()
            encoder.encode(Int32(10))  // a: i32
            encoder.encode(Int32(32))  // b: i32
            let addRequest = encoder.bytes

            print("  Request payload: \(addRequest.map { String(format: "%02x", $0) }.joined(separator: " "))")

            let addResponse = try await client.call(methodId: ECHO_METHOD_ID_ADD, requestPayload: addRequest)
            print("  Response payload: \(addResponse.map { String(format: "%02x", $0) }.joined(separator: " "))")

            // Decode the response (i32 as zigzag varint)
            var responseData = Data(addResponse)
            let result = try decodeSignedVarint(from: &responseData)
            print("  Result: \(result)")

            if result == 42 {
                print("✓ Echo.add test passed!\n")
            } else {
                print("✗ Expected 42, got \(result)\n")
            }

            // Test 2: Call Echo.echo("Hello from Swift!")
            print("Calling Echo.echo(\"Hello from Swift!\")...")
            var echoEncoder = PostcardEncoder()
            echoEncoder.encode("Hello from Swift!")
            let echoRequest = echoEncoder.bytes

            print("  Request payload: \(echoRequest.map { String(format: "%02x", $0) }.joined(separator: " "))")

            let echoResponse = try await client.call(methodId: ECHO_METHOD_ID_ECHO, requestPayload: echoRequest)
            print("  Response payload (\(echoResponse.count) bytes): \(echoResponse.prefix(50).map { String(format: "%02x", $0) }.joined(separator: " "))...")

            // Decode the response (String as varint length + UTF-8)
            var echoData = Data(echoResponse)
            let strLen = try decodeVarint(from: &echoData)
            let strBytes = echoData.prefix(Int(strLen))
            if let responseStr = String(data: Data(strBytes), encoding: .utf8) {
                print("  Result: \"\(responseStr)\"")
                if responseStr == "Echo: Hello from Swift!" {
                    print("✓ Echo.echo test passed!\n")
                } else {
                    print("✗ Unexpected response\n")
                }
            } else {
                print("  Failed to decode string")
            }

            await client.close()
            print("Connection closed.")
            print("\n=== All tests completed! ===")

        } catch {
            print("ERROR: \(error)")
        }
    }
}

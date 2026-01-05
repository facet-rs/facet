import Foundation
import Rapace
import Postcard
import GeneratedTest

@main
struct StressTest {
    static func main() async {
        // Disable output buffering
        setbuf(stdout, nil)
        setbuf(stderr, nil)
        fputs("=== VFS Stress Test ===\n\n", stdout)
        fflush(stdout)

        do {
            // Connect to the fs-kitty server
            print("Connecting to localhost:10001...")
            let client = try await VfsClient(host: "127.0.0.1", port: 10001)
            print("Connected!\n")

            // Test 1: Sequential calls
            print("--- Test 1: Sequential getAttributes calls ---")
            for i in 1...10 {
                let result = try await client.getAttributes(1)
                assert(result.error == 0, "Expected error=0, got \(result.error)")
                assert(result.attrs.item_id == 1, "Expected item_id=1, got \(result.attrs.item_id)")
                print("  Call \(i): OK (item_type=\(result.attrs.item_type))")
            }
            print("  Sequential test passed!\n")

            // Test 2: Concurrent calls (simulate what FSKit might do)
            print("--- Test 2: Concurrent getAttributes calls ---")
            try await withThrowingTaskGroup(of: (Int, GetAttributesResult).self) { group in
                for i in 1...20 {
                    group.addTask {
                        let result = try await client.getAttributes(1)
                        return (i, result)
                    }
                }

                for try await (i, result) in group {
                    assert(result.error == 0, "Task \(i): Expected error=0, got \(result.error)")
                    assert(result.attrs.item_id == 1, "Task \(i): Expected item_id=1, got \(result.attrs.item_id)")
                    print("  Task \(i): OK (item_type=\(result.attrs.item_type))")
                }
            }
            print("  Concurrent test passed!\n")

            // Test 3: Mixed operations
            print("--- Test 3: Mixed concurrent operations ---")
            try await withThrowingTaskGroup(of: String.self) { group in
                // getAttributes calls
                for i in 1...5 {
                    group.addTask {
                        let result = try await client.getAttributes(1)
                        return "getAttributes[\(i)]: item_id=\(result.attrs.item_id), type=\(result.attrs.item_type)"
                    }
                }

                // readDir calls
                for i in 1...5 {
                    group.addTask {
                        let result = try await client.readDir(item_id: 1, cursor: 0)
                        return "readDir[\(i)]: \(result.entries.count) entries"
                    }
                }

                // lookup calls
                for i in 1...5 {
                    group.addTask {
                        let result = try await client.lookup(parent_id: 1, name: "hello.txt")
                        return "lookup[\(i)]: item_id=\(result.item_id), type=\(result.item_type)"
                    }
                }

                // ping calls
                for i in 1...5 {
                    group.addTask {
                        let result = try await client.ping()
                        return "ping[\(i)]: \(result)"
                    }
                }

                for try await msg in group {
                    print("  \(msg)")
                }
            }
            print("  Mixed test passed!\n")

            // Test 4: Rapid sequential with validation
            print("--- Test 4: Rapid sequential validation ---")
            for i in 1...50 {
                let attrsResult = try await client.getAttributes(1)

                // Validate all fields
                if attrsResult.error != 0 {
                    fatalError("Call \(i): error=\(attrsResult.error)")
                }
                if attrsResult.attrs.item_id != 1 {
                    fatalError("Call \(i): item_id=\(attrsResult.attrs.item_id), expected 1")
                }
                switch attrsResult.attrs.item_type {
                case .directory:
                    break // expected for root
                case .file:
                    fatalError("Call \(i): unexpected file type for root")
                case .symlink:
                    fatalError("Call \(i): unexpected symlink type for root")
                }

                if i % 10 == 0 {
                    print("  Calls 1-\(i): OK")
                }
            }
            print("  Rapid sequential test passed!\n")

            print("=== All stress tests passed! ===")

        } catch {
            print("ERROR: \(error)")
            exit(1)
        }
    }
}

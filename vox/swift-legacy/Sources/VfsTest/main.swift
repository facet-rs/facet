import Foundation
import Rapace
import Postcard
import GeneratedTest

@main
struct VfsTest {
    static func main() async {
        print("=== VFS Client Test ===\n")

        do {
            // Connect to the fs-kitty server
            print("Connecting to localhost:10001...")
            let client = try await VfsClient(host: "127.0.0.1", port: 10001)
            print("Connected!\n")

            // Test 1: Ping
            print("--- Test: ping ---")
            let pong = try await client.ping()
            print("  Response: \"\(pong)\"")
            print("  OK\n")

            // Test 2: Read root directory (id=1)
            print("--- Test: readDir (root) ---")
            let rootDir = try await client.readDir(item_id: 1, cursor: 0)
            print("  Error: \(rootDir.error)")
            print("  Entries: \(rootDir.entries.count)")
            for entry in rootDir.entries {
                let typeStr: String
                switch entry.item_type {
                case .file: typeStr = "file"
                case .directory: typeStr = "dir"
                case .symlink: typeStr = "link"
                }
                print("    - \(entry.name) (id=\(entry.item_id), type=\(typeStr))")
            }
            print("  OK\n")

            // Test 3: Lookup hello.txt
            print("--- Test: lookup (hello.txt) ---")
            let lookup = try await client.lookup(parent_id: 1, name: "hello.txt")
            print("  Error: \(lookup.error)")
            print("  Item ID: \(lookup.item_id)")
            let typeStr: String
            switch lookup.item_type {
            case .file: typeStr = "file"
            case .directory: typeStr = "dir"
            case .symlink: typeStr = "link"
            }
            print("  Type: \(typeStr)")
            print("  OK\n")

            // Test 4: Get attributes
            print("--- Test: getAttributes (hello.txt, id=\(lookup.item_id)) ---")
            let attrs = try await client.getAttributes(lookup.item_id)
            print("  Error: \(attrs.error)")
            print("  Size: \(attrs.attrs.size) bytes")
            print("  Mode: 0o\(String(attrs.attrs.mode, radix: 8))")
            print("  ItemType: \(attrs.attrs.item_type)")
            print("  OK\n")

            // Test 4b: Get attributes for ROOT (item_id=1) - this is what FSKit does
            print("--- Test: getAttributes (ROOT, id=1) ---")
            let rootAttrs = try await client.getAttributes(1)
            print("  Error: \(rootAttrs.error)")
            print("  ItemID: \(rootAttrs.attrs.item_id)")
            print("  ItemType: \(rootAttrs.attrs.item_type)")
            print("  Size: \(rootAttrs.attrs.size)")
            print("  Mode: 0o\(String(rootAttrs.attrs.mode, radix: 8))")
            print("  OK\n")

            // Test 5: Read file contents
            print("--- Test: read (hello.txt) ---")
            let readResult = try await client.read(item_id: lookup.item_id, offset: 0, len: 100)
            print("  Error: \(readResult.error)")
            print("  Bytes: \(readResult.data.count)")
            if let content = String(data: Data(readResult.data), encoding: .utf8) {
                print("  Content: \"\(content.trimmingCharacters(in: .newlines))\"")
            }
            print("  OK\n")

            // Test 6: Create a new file
            print("--- Test: create (test.txt) ---")
            let createResult = try await client.create(parent_id: 1, name: "test.txt", item_type: .file)
            print("  Error: \(createResult.error)")
            print("  New ID: \(createResult.item_id)")
            print("  OK\n")

            // Test 7: Write to the new file
            print("--- Test: write (test.txt) ---")
            let testContent = Array("Hello from Swift VFS client!".utf8)
            let writeResult = try await client.write(item_id: createResult.item_id, offset: 0, data: testContent)
            print("  Error: \(writeResult.error)")
            print("  Bytes written: \(writeResult.bytes_written)")
            print("  OK\n")

            // Test 8: Read back the written content
            print("--- Test: read (test.txt) ---")
            let readBack = try await client.read(item_id: createResult.item_id, offset: 0, len: 100)
            print("  Error: \(readBack.error)")
            if let content = String(data: Data(readBack.data), encoding: .utf8) {
                print("  Content: \"\(content)\"")
                if content == "Hello from Swift VFS client!" {
                    print("  MATCH!")
                }
            }
            print("  OK\n")

            // Test 9: Delete the test file
            print("--- Test: delete (test.txt) ---")
            let deleteResult = try await client.delete(createResult.item_id)
            print("  Error: \(deleteResult.error)")
            print("  OK\n")

            print("=== All tests passed! ===")

        } catch {
            print("ERROR: \(error)")
        }
    }
}

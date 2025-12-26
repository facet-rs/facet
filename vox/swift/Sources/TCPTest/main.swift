import Foundation
import Rapace

@main
struct TCPTest {
    static func main() async {
        print("TCP Connection Test")
        print("==================")
        print("Connecting to localhost:8080...")

        let connection = TCPConnection()

        do {
            // Connect to the server
            try await connection.connect(host: "127.0.0.1", port: 8080)
            print("Connected!")

            // Send "hello"
            let message = "hello"
            let data = Data(message.utf8)
            print("Sending: \(message)")
            try await connection.send(data)
            print("Sent \(data.count) bytes")

            // Read response (up to 1024 bytes)
            print("Waiting for response...")
            let response = try await connection.receive(upTo: 1024)

            if let responseString = String(data: response, encoding: .utf8) {
                print("Received: \(responseString)")
            } else {
                print("Received \(response.count) bytes (binary)")
            }

            // Close connection
            await connection.close()
            print("Connection closed")

        } catch {
            print("Error: \(error)")
            await connection.close()
        }
    }
}

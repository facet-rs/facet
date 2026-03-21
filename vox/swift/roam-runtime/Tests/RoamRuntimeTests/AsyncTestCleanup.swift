import Foundation

func withAsyncCleanup<T>(
    _ cleanup: () async -> Void,
    operation: () async throws -> T
) async rethrows -> T {
    do {
        let result = try await operation()
        await cleanup()
        return result
    } catch {
        await cleanup()
        throw error
    }
}

func cancelAndDrain<T>(_ task: Task<T, Error>) async {
    task.cancel()
    _ = try? await task.value
}

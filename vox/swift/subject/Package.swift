// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "subject-swift",
    platforms: [
        .macOS(.v13),
    ],
    products: [
        .executable(name: "subject-swift", targets: ["subject-swift"]),
    ],
    targets: [
        .executableTarget(
            name: "subject-swift",
            path: "Sources/subject-swift"
        ),
    ]
)


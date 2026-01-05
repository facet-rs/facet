// swift-tools-version: 5.9
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


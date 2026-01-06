// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "subject-swift",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(name: "subject-swift", targets: ["subject-swift"])
    ],
    dependencies: [
        .package(path: "../roam-runtime")
    ],
    targets: [
        .executableTarget(
            name: "subject-swift",
            dependencies: [
                .product(name: "RoamRuntime", package: "roam-runtime")
            ],
            path: "Sources/subject-swift"
        )
    ]
)

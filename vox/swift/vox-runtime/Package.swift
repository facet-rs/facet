// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "vox-runtime",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .library(name: "VoxRuntime", targets: ["VoxRuntime"])
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-nio.git", from: "2.97.1")
    ],
    targets: [
        .target(
            name: "VoxRuntime",
            dependencies: [
                .product(name: "NIO", package: "swift-nio"),
                .product(name: "NIOCore", package: "swift-nio"),
                .product(name: "NIOPosix", package: "swift-nio"),
            ],
            path: "Sources/VoxRuntime",
            resources: [
                .copy("wireMessageSchemas.bin")
            ]
        ),
        .testTarget(
            name: "VoxRuntimeTests",
            dependencies: ["VoxRuntime"],
            path: "Tests/VoxRuntimeTests"
        ),
    ]
)

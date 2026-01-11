// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "roam-runtime",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .library(name: "RoamRuntime", targets: ["RoamRuntime"])
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-nio.git", from: "2.92.1")
    ],
    targets: [
        .target(
            name: "RoamRuntime",
            dependencies: [
                .product(name: "NIO", package: "swift-nio"),
                .product(name: "NIOCore", package: "swift-nio"),
                .product(name: "NIOPosix", package: "swift-nio"),
            ],
            path: "Sources/RoamRuntime"
        ),
        .testTarget(
            name: "RoamRuntimeTests",
            dependencies: ["RoamRuntime"],
            path: "Tests/RoamRuntimeTests"
        ),
    ]
)

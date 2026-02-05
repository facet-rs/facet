// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "roam",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .library(name: "RoamRuntime", targets: ["RoamRuntime"])
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-nio.git", from: "2.92.0")
    ],
    targets: [
        .target(
            name: "RoamRuntime",
            dependencies: [
                .product(name: "NIO", package: "swift-nio"),
                .product(name: "NIOCore", package: "swift-nio"),
                .product(name: "NIOPosix", package: "swift-nio"),
            ],
            path: "swift/roam-runtime/Sources/RoamRuntime"
        ),
        .testTarget(
            name: "RoamRuntimeTests",
            dependencies: ["RoamRuntime"],
            path: "swift/roam-runtime/Tests/RoamRuntimeTests"
        ),
    ]
)

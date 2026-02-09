// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "roam-runtime",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .library(name: "RoamRuntime", targets: ["RoamRuntime"]),
        .executable(name: "shm-bootstrap-client", targets: ["shm-bootstrap-client"]),
        .executable(name: "shm-guest-client", targets: ["shm-guest-client"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-nio.git", from: "2.92.0")
    ],
    targets: [
        .target(
            name: "RoamRuntime",
            dependencies: [
                "CRoamShm",
                .product(name: "NIO", package: "swift-nio"),
                .product(name: "NIOCore", package: "swift-nio"),
                .product(name: "NIOPosix", package: "swift-nio"),
            ],
            path: "Sources/RoamRuntime"
        ),
        .target(
            name: "CRoamShm",
            path: "Sources/CRoamShm",
            publicHeadersPath: "include"
        ),
        .executableTarget(
            name: "shm-bootstrap-client",
            dependencies: ["RoamRuntime"],
            path: "Sources/shm-bootstrap-client"
        ),
        .executableTarget(
            name: "shm-guest-client",
            dependencies: ["RoamRuntime"],
            path: "Sources/shm-guest-client"
        ),
        .testTarget(
            name: "RoamRuntimeTests",
            dependencies: ["RoamRuntime"],
            path: "Tests/RoamRuntimeTests"
        ),
    ]
)

// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "roam-runtime",
    platforms: [
        .macOS(.v13),
        .iOS(.v16),
    ],
    products: [
        .library(
            name: "RoamRuntime",
            targets: ["RoamRuntime"]
        )
    ],
    targets: [
        .target(
            name: "RoamRuntime",
            path: "Sources/RoamRuntime"
        ),
        .testTarget(
            name: "RoamRuntimeTests",
            dependencies: ["RoamRuntime"],
            path: "Tests/RoamRuntimeTests"
        ),
    ]
)

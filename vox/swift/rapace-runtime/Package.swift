// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "rapace-runtime",
    platforms: [
        .macOS(.v13),
        .iOS(.v16),
    ],
    products: [
        .library(
            name: "RapaceRuntime",
            targets: ["RapaceRuntime"]
        ),
    ],
    targets: [
        .target(
            name: "RapaceRuntime",
            path: "Sources/RapaceRuntime"
        ),
        .testTarget(
            name: "RapaceRuntimeTests",
            dependencies: ["RapaceRuntime"],
            path: "Tests/RapaceRuntimeTests"
        ),
    ]
)

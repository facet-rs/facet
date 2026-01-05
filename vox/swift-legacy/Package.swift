// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "Rapace",
    platforms: [
        .macOS(.v13),
        .iOS(.v16),
    ],
    products: [
        .library(
            name: "Rapace",
            targets: ["Rapace"]
        ),
        .library(
            name: "Postcard",
            targets: ["Postcard"]
        ),
        .executable(
            name: "TCPTest",
            targets: ["TCPTest"]
        ),
        .executable(
            name: "StructTest",
            targets: ["StructTest"]
        ),
    ],
    targets: [
        .target(
            name: "Rapace",
            dependencies: ["Postcard"]
        ),
        .target(
            name: "Postcard"
        ),
        .target(
            name: "ConformanceRunner",
            dependencies: ["Rapace"]
        ),
        .executableTarget(
            name: "TCPTest",
            dependencies: ["Rapace"]
        ),
        .executableTarget(
            name: "StructTest",
            dependencies: ["Postcard"]
        ),
        .executableTarget(
            name: "RPCTest",
            dependencies: ["Rapace", "Postcard"]
        ),
        .target(
            name: "GeneratedTest",
            dependencies: ["Rapace", "Postcard"]
        ),
        .executableTarget(
            name: "VfsTest",
            dependencies: ["Rapace", "Postcard", "GeneratedTest"]
        ),
        .executableTarget(
            name: "StressTest",
            dependencies: ["Rapace", "Postcard", "GeneratedTest"]
        ),
        .testTarget(
            name: "PostcardTests",
            dependencies: ["Postcard"]
        ),
        .testTarget(
            name: "RapaceTests",
            dependencies: ["Rapace", "ConformanceRunner"]
        ),
    ]
)

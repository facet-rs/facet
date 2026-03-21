// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "subject-swift",
    platforms: [
        .macOS(.v14)
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
            sources: [
                "Server.swift",
                "Subject.swift",
                "Testbed.swift",
            ]
        ),
        .testTarget(
            name: "subject-swiftTests",
            dependencies: [
                .byName(name: "subject-swift"),
                .product(name: "RoamRuntime", package: "roam-runtime"),
            ]
        )
    ]
)

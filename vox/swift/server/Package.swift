// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "swift-server",
    platforms: [.macOS(.v14)],
    dependencies: [
        .package(path: "../roam-runtime"),
    ],
    targets: [
        .executableTarget(
            name: "swift-server",
            dependencies: [
                .product(name: "RoamRuntime", package: "roam-runtime"),
            ],
            path: "Sources"
        ),
    ]
)

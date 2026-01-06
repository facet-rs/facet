// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "subject-swift",
    platforms: [
        .macOS(.v13),
    ],
    products: [
        .executable(name: "subject-swift", targets: ["subject-swift"]),
    ],
    dependencies: [
        .package(path: "../rapace-runtime"),
    ],
    targets: [
        .executableTarget(
            name: "subject-swift",
            dependencies: [
                .product(name: "RapaceRuntime", package: "rapace-runtime"),
            ],
            path: "Sources/subject-swift"
        ),
    ]
)


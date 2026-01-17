// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "Styx",
    products: [
        .library(name: "Styx", targets: ["Styx"]),
        .executable(name: "styx-compliance", targets: ["StyxCompliance"]),
    ],
    targets: [
        .target(name: "Styx"),
        .executableTarget(
            name: "StyxCompliance",
            dependencies: ["Styx"]
        ),
        .testTarget(
            name: "StyxTests",
            dependencies: ["Styx"]
        ),
    ]
)

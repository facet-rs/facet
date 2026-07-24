// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "facet",
    platforms: [
        .macOS(.v15)
    ],
    products: [
        .library(name: "Phon", targets: ["Phon"]),
        .library(name: "PhonSchema", targets: ["PhonSchema"]),
        .library(name: "PhonIR", targets: ["PhonIR"]),
        .library(name: "PhonEngine", targets: ["PhonEngine"]),
        .library(name: "PhonJIT", targets: ["PhonJIT"]),
        .library(name: "VoxRuntime", targets: ["VoxRuntime"]),
        .library(name: "VoxRuntimeJIT", targets: ["VoxRuntimeJIT"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-nio.git", from: "2.101.3"),
    ],
    targets: [
        .target(
            name: "CBlake3",
            path: "phon/swift/cblake3/Sources/CBlake3",
            cSettings: [
                .define("BLAKE3_USE_NEON", to: "0"),
                .define("BLAKE3_NO_SSE2"),
                .define("BLAKE3_NO_SSE41"),
                .define("BLAKE3_NO_AVX2"),
                .define("BLAKE3_NO_AVX512"),
            ]
        ),
        .target(
            name: "PhonSchema",
            dependencies: ["CBlake3"],
            path: "phon/swift/phon-schema/Sources/PhonSchema"
        ),
        .target(
            name: "PhonIR",
            dependencies: ["PhonSchema"],
            path: "phon/swift/phon-ir/Sources/PhonIR"
        ),
        .target(
            name: "PhonEngine",
            dependencies: ["PhonSchema", "PhonIR"],
            path: "phon/swift/phon-engine/Sources/PhonEngine"
        ),
        .target(
            name: "CPhonJITStencils",
            path: "phon/swift/cphon-jit-stencils/Sources/CPhonJITStencils",
            publicHeadersPath: "include"
        ),
        .target(
            name: "PhonJIT",
            dependencies: ["CPhonJITStencils", "PhonEngine", "PhonIR", "PhonSchema"],
            path: "phon/swift/phon-jit/Sources/PhonJIT"
        ),
        .target(
            name: "Phon",
            dependencies: ["PhonSchema", "PhonEngine"],
            path: "phon/swift/phon/Sources/Phon"
        ),
        .target(
            name: "VoxRuntime",
            dependencies: [
                .product(name: "NIO", package: "swift-nio"),
                .product(name: "NIOCore", package: "swift-nio"),
                .product(name: "NIOPosix", package: "swift-nio"),
                "PhonSchema",
                "PhonIR",
                "PhonEngine",
            ],
            path: "vox/swift/vox-runtime/Sources/VoxRuntime",
            resources: [
                .copy("wireMessageSchemas.bin")
            ]
        ),
        .target(
            name: "VoxRuntimeJIT",
            dependencies: [
                "VoxRuntime",
                "PhonJIT",
            ],
            path: "vox/swift/vox-runtime/Sources/VoxRuntimeJIT"
        ),
        .testTarget(
            name: "VoxRuntimeTests",
            dependencies: [
                "VoxRuntime",
                "PhonSchema",
            ],
            path: "vox/swift/vox-runtime/Tests/VoxRuntimeTests"
        ),
        .testTarget(
            name: "PhonTests",
            dependencies: ["Phon", "PhonSchema"],
            path: "phon/swift/phon/Tests/PhonTests"
        ),
        .testTarget(
            name: "PhonSchemaTests",
            dependencies: ["PhonSchema"],
            path: "phon/swift/phon-schema/Tests/PhonSchemaTests"
        )
    ]
)

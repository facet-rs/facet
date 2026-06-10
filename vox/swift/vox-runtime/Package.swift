// swift-tools-version: 6.0
import PackageDescription

let package = Package(
  name: "vox-runtime",
  platforms: [
    // macOS 15 to match the phon package (native UInt128 / String(validating:)).
    .macOS(.v15)
  ],
  products: [
    .library(name: "VoxRuntime", targets: ["VoxRuntime"]),
    .library(name: "VoxRuntimeJIT", targets: ["VoxRuntimeJIT"]),
  ],
  dependencies: [
    .package(url: "https://github.com/apple/swift-nio.git", from: "2.99.0"),
    .package(
      url: "https://github.com/bearcove/phon.git",
      revision: "429b72badcc5e827613f9245c153cd91c0458f4f"),
  ],
  targets: [
    .target(
      name: "VoxRuntime",
      dependencies: [
        .product(name: "NIO", package: "swift-nio"),
        .product(name: "NIOCore", package: "swift-nio"),
        .product(name: "NIOPosix", package: "swift-nio"),
        .product(name: "PhonSchema", package: "phon"),
        .product(name: "PhonIR", package: "phon"),
        .product(name: "PhonEngine", package: "phon"),
      ],
      path: "Sources/VoxRuntime",
      resources: [
        .copy("wireMessageSchemas.bin")
      ]
    ),
    .target(
      name: "VoxRuntimeJIT",
      dependencies: [
        "VoxRuntime",
        .product(name: "PhonJIT", package: "phon"),
      ],
      path: "Sources/VoxRuntimeJIT"
    ),
    .testTarget(
      name: "VoxRuntimeTests",
      dependencies: [
        "VoxRuntime",
        .product(name: "PhonSchema", package: "phon"),
      ],
      path: "Tests/VoxRuntimeTests"
    ),
  ]
)

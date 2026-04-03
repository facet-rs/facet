// swift-tools-version: 6.0
import PackageDescription

// vox-shm-ffi is a Rust staticlib built by `cargo build --release -p vox-shm-ffi`.
// It lives at target/release/libvox_shm_ffi.a relative to the vox workspace root,
// which is two directories up from this Package.swift.
let voxRoot = "../.."
let rustLibDir = "\(voxRoot)/target/release"

let package = Package(
    name: "vox-runtime",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .library(name: "VoxRuntime", targets: ["VoxRuntime"]),
        .executable(name: "shm-bootstrap-client", targets: ["shm-bootstrap-client"]),
        .executable(name: "shm-guest-client", targets: ["shm-guest-client"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-nio.git", from: "2.92.0"),
    ],
    targets: [
        .target(
            name: "VoxRuntime",
            dependencies: [
                "CVoxShm",
                "CVoxShmFfi",
                .product(name: "NIO", package: "swift-nio"),
                .product(name: "NIOCore", package: "swift-nio"),
                .product(name: "NIOPosix", package: "swift-nio"),
            ],
            path: "Sources/VoxRuntime"
        ),
        .target(
            name: "CVoxShm",
            path: "Sources/CVoxShm",
            publicHeadersPath: "include"
        ),
        .target(
            name: "CVoxShmFfi",
            path: "Sources/CVoxShmFfi",
            publicHeadersPath: "include",
            linkerSettings: [
                .unsafeFlags(["-L\(rustLibDir)", "-lvox_shm_ffi"]),
            ]
        ),
        .executableTarget(
            name: "shm-bootstrap-client",
            dependencies: ["VoxRuntime"],
            path: "Sources/shm-bootstrap-client"
        ),
        .executableTarget(
            name: "shm-guest-client",
            dependencies: ["VoxRuntime"],
            path: "Sources/shm-guest-client"
        ),
        .testTarget(
            name: "VoxRuntimeTests",
            dependencies: ["VoxRuntime"],
            path: "Tests/VoxRuntimeTests"
        ),
    ]
)

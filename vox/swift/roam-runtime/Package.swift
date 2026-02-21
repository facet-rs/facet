// swift-tools-version: 6.0
import PackageDescription

// roam-shm-ffi is a Rust staticlib built by `cargo build --release -p roam-shm-ffi`.
// It lives at target/release/libroam_shm_ffi.a relative to the roam workspace root,
// which is two directories up from this Package.swift.
let roamRoot = "../.."
let rustLibDir = "\(roamRoot)/target/release"

let package = Package(
    name: "roam-runtime",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .library(name: "RoamRuntime", targets: ["RoamRuntime"]),
        .executable(name: "shm-bootstrap-client", targets: ["shm-bootstrap-client"]),
        .executable(name: "shm-guest-client", targets: ["shm-guest-client"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-nio.git", from: "2.92.0")
    ],
    targets: [
        .target(
            name: "RoamRuntime",
            dependencies: [
                "CRoamShm",
                "CRoamShmFfi",
                .product(name: "NIO", package: "swift-nio"),
                .product(name: "NIOCore", package: "swift-nio"),
                .product(name: "NIOPosix", package: "swift-nio"),
            ],
            path: "Sources/RoamRuntime"
        ),
        .target(
            name: "CRoamShm",
            path: "Sources/CRoamShm",
            publicHeadersPath: "include"
        ),
        .target(
            name: "CRoamShmFfi",
            path: "Sources/CRoamShmFfi",
            publicHeadersPath: "include",
            linkerSettings: [
                .unsafeFlags(["-L\(rustLibDir)", "-lroam_shm_ffi"]),
            ]
        ),
        .executableTarget(
            name: "shm-bootstrap-client",
            dependencies: ["RoamRuntime"],
            path: "Sources/shm-bootstrap-client"
        ),
        .executableTarget(
            name: "shm-guest-client",
            dependencies: ["RoamRuntime"],
            path: "Sources/shm-guest-client"
        ),
        .testTarget(
            name: "RoamRuntimeTests",
            dependencies: ["RoamRuntime"],
            path: "Tests/RoamRuntimeTests"
        ),
    ]
)

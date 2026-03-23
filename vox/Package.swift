// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "vox",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .library(name: "VoxRuntime", targets: ["VoxRuntime"])
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-nio.git", from: "2.92.0")
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
            path: "swift/vox-runtime/Sources/VoxRuntime"
        ),
        .target(
            name: "CVoxShm",
            path: "swift/vox-runtime/Sources/CVoxShm",
            publicHeadersPath: "include"
        ),
        .target(
            name: "CVoxShmFfi",
            path: "swift/vox-runtime/Sources/CVoxShmFfi",
            publicHeadersPath: "include",
            linkerSettings: [
                // Consumer must build libvox_shm_ffi.a (cargo build --release -p vox-shm-ffi)
                // and add its directory to LIBRARY_SEARCH_PATHS or pass -Xlinker -L<path>.
                .linkedLibrary("vox_shm_ffi"),
            ]
        ),
        .testTarget(
            name: "VoxRuntimeTests",
            dependencies: ["VoxRuntime"],
            path: "swift/vox-runtime/Tests/VoxRuntimeTests"
        ),
    ]
)

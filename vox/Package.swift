// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "roam",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .library(name: "RoamRuntime", targets: ["RoamRuntime"])
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
            path: "swift/roam-runtime/Sources/RoamRuntime"
        ),
        .target(
            name: "CRoamShm",
            path: "swift/roam-runtime/Sources/CRoamShm",
            publicHeadersPath: "include"
        ),
        .target(
            name: "CRoamShmFfi",
            path: "swift/roam-runtime/Sources/CRoamShmFfi",
            publicHeadersPath: "include",
            linkerSettings: [
                // Consumer must build libroam_shm_ffi.a (cargo build --release -p roam-shm-ffi)
                // and add its directory to LIBRARY_SEARCH_PATHS or pass -Xlinker -L<path>.
                .linkedLibrary("roam_shm_ffi"),
            ]
        ),
        .testTarget(
            name: "RoamRuntimeTests",
            dependencies: ["RoamRuntime"],
            path: "swift/roam-runtime/Tests/RoamRuntimeTests"
        ),
    ]
)

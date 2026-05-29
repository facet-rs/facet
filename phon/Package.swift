// swift-tools-version: 6.0
import PackageDescription

// phon's Swift implementation. The package split mirrors the Rust crates and the
// "Crates and packages" section of docs/content/spec.md: a contract module
// (PhonSchema), the shared execution vocabulary (PhonIR), the backend-blind
// engine (PhonEngine), the optional copy-and-patch JIT (PhonJIT), and the
// binding/front door (Phon). Only the binding probes the Swift runtime for
// descriptors; the engine and JIT stay binding-free.
let package = Package(
    name: "phon",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .library(name: "Phon", targets: ["Phon"]),
        .library(name: "PhonSchema", targets: ["PhonSchema"]),
        // The JIT is reached only by opting in to this product; the baseline is
        // PhonEngine's interpreter (r[crates.jit-opt-in]).
        .library(name: "PhonJIT", targets: ["PhonJIT"]),
    ],
    targets: [
        .target(
            name: "PhonSchema",
            path: "swift/phon-schema/Sources/PhonSchema"
        ),
        .target(
            name: "PhonIR",
            dependencies: ["PhonSchema"],
            path: "swift/phon-ir/Sources/PhonIR"
        ),
        .target(
            name: "PhonEngine",
            dependencies: ["PhonSchema", "PhonIR"],
            path: "swift/phon-engine/Sources/PhonEngine"
        ),
        .target(
            name: "PhonJIT",
            dependencies: ["PhonIR"],
            path: "swift/phon-jit/Sources/PhonJIT"
        ),
        .target(
            name: "Phon",
            dependencies: ["PhonSchema", "PhonEngine"],
            path: "swift/phon/Sources/Phon"
        ),
        .testTarget(
            name: "PhonTests",
            dependencies: ["Phon", "PhonSchema"],
            path: "swift/phon/Tests/PhonTests"
        ),
    ]
)

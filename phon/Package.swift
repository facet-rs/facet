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
        // macOS 15: native `UInt128`/`Int128` (the 128-bit value widths) and
        // `String(validating:as:)` (strict UTF-8 decode).
        .macOS(.v15)
    ],
    products: [
        .library(name: "Phon", targets: ["Phon"]),
        .library(name: "PhonSchema", targets: ["PhonSchema"]),
        // The execution vocabulary (descriptors + IR) and the backend-blind
        // engine — consumed by a binding (e.g. vox-runtime) that supplies
        // descriptors and drives the typed codec.
        .library(name: "PhonIR", targets: ["PhonIR"]),
        .library(name: "PhonEngine", targets: ["PhonEngine"]),
        // The JIT is reached only by opting in to this product; the baseline is
        // PhonEngine's interpreter (r[crates.jit-opt-in]).
        .library(name: "PhonJIT", targets: ["PhonJIT"]),
        // Cross-engine equivalence harness (tree-walk vs interpreter vs JIT), shared by
        // phon's own tests and downstream consumers (vox) so the engine list lives once.
        .library(name: "PhonEngineTestSupport", targets: ["PhonEngineTestSupport"]),
    ],
    targets: [
        // Vendored portable C BLAKE3 — content-hash schema identity. Portable
        // path only (the x86 SIMD sources are not vendored; arm64 excludes them).
        .target(
            name: "CBlake3",
            path: "swift/cblake3/Sources/CBlake3",
            cSettings: [.define("BLAKE3_USE_NEON", to: "0")]
        ),
        .target(
            name: "PhonSchema",
            dependencies: ["CBlake3"],
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
            name: "CPhonJITStencils",
            path: "swift/cphon-jit-stencils/Sources/CPhonJITStencils",
            publicHeadersPath: "include"
        ),
        .target(
            name: "PhonJIT",
            dependencies: ["CPhonJITStencils", "PhonEngine", "PhonIR", "PhonSchema"],
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
        .testTarget(
            name: "PhonSchemaTests",
            dependencies: ["PhonSchema"],
            path: "swift/phon-schema/Tests/PhonSchemaTests"
        ),
        .target(
            name: "PhonEngineTestSupport",
            dependencies: ["PhonEngine", "PhonIR", "PhonJIT", "PhonSchema"],
            path: "swift/phon-engine-test-support/Sources/PhonEngineTestSupport"
        ),
        .testTarget(
            name: "PhonEngineTests",
            dependencies: ["PhonEngine", "PhonEngineTestSupport", "PhonIR", "PhonSchema"],
            path: "swift/phon-engine/Tests/PhonEngineTests"
        ),
        .testTarget(
            name: "PhonJITTests",
            dependencies: ["PhonEngine", "PhonIR", "PhonJIT", "PhonSchema"],
            path: "swift/phon-jit/Tests/PhonJITTests"
        ),
    ]
)

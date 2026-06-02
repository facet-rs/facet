// The cross-engine equivalence harness, shared by phon's own tests and downstream
// consumers (e.g. vox's codegen-corpus tests). It lives in a library target — not a
// test target — so it can be imported across packages while still using Swift Testing's
// `#expect`. The engine list lives here once: adding the copy-and-patch JIT
// (`allTypedEngines.append(JITEngine())`) gives every corpus everywhere
// `tree-walk == interpreter == JIT` coverage with no new tests.

import Testing

import PhonEngine
import PhonIR
import PhonSchema

/// Backends every equivalence check runs through. The JIT appends here once it lands.
public let allTypedEngines: [any TypedEngine] = [InterpreterEngine()]

/// Encode `value` with each engine and assert: (1) every engine agrees byte-for-byte,
/// (2) the bytes are canonical phon — the tree-walk decodes and re-emits them
/// identically (an independent oracle, no per-type `T → Value` bridge), and (3) every
/// engine byte-round-trips: decoding then re-encoding the decoded value reproduces the
/// original bytes (wire-equivalence — no `T: Equatable` needed, so any generated type
/// is corpus-ready). Written once; every backend runs it.
public func assertTypedEquivalence<T>(
    _ value: T, descriptor: Descriptor, registry: Registry,
    _ label: String = "",
    engines: [any TypedEngine] = allTypedEngines,
    sourceLocation: SourceLocation = #_sourceLocation
) throws {
    let root = descriptor.rootId

    // Reference bytes from the first engine.
    let ref = withUnsafeBytes(of: value) {
        try! engines[0].compileEncode(descriptor, registry)($0.baseAddress!)
    }

    // Independent oracle: the tree-walk reads the typed bytes and re-emits them
    // identically — proving they are canonical phon for the value they encode.
    let dyn = try decode(ref, root, registry)
    #expect(
        try encode(dyn, root, registry) == ref,
        "\(label): typed bytes are not canonical phon", sourceLocation: sourceLocation)

    for e in engines {
        let enc = try e.compileEncode(descriptor, registry)
        let bytes = withUnsafeBytes(of: value) { enc($0.baseAddress!) }
        #expect(bytes == ref, "\(label): \(e.name) encode diverges", sourceLocation: sourceLocation)

        // Decode into fresh storage, take ownership (ARC frees its heap data at scope
        // end), then re-encode it: a correct decode reproduces the original bytes.
        let dec = try e.compileDecode(root, descriptor, registry)
        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: MemoryLayout<T>.size, alignment: MemoryLayout<T>.alignment)
        defer { raw.deallocate() }
        try dec(bytes, raw)
        let out = raw.assumingMemoryBound(to: T.self).move()
        let reencoded = withUnsafeBytes(of: out) { enc($0.baseAddress!) }
        #expect(
            reencoded == ref,
            "\(label): \(e.name) round-trip (re-encode) diverges", sourceLocation: sourceLocation)
    }
}

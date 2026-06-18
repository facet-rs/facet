import Testing

@testable import Phon
@testable import PhonSchema

// Smoke test: proves the module graph (Phon -> PhonEngine -> PhonIR ->
// PhonSchema) links. The real tests that live here are the cross-language
// conformance checks: load the shared corpus under conformance/ and verify Swift
// encodes the same compact bytes and computes the same SchemaId as Rust.
@Test
func modulesLink() {
    _ = Phon.self
    _ = PhonSchema.self
}

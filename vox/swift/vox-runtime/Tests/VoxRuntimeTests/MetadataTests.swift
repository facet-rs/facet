import Testing

@testable import VoxRuntime

struct MetadataTests {
    // r[verify rpc.metadata]
    // r[verify rpc.metadata.value]
    // r[verify rpc.metadata.keys]
    // r[verify rpc.metadata.duplicates]
    // r[verify rpc.metadata.unknown]
    @Test func metadataIsAValueMapWithStringKeys() {
        var metadata: Metadata = .null
        metadata.metaSet("trace", .string("abc"))
        metadata.metaSet("attempt", metaU64Value(7))
        metadata.metaSet("Trace", .string("case-sensitive"))
        metadata.metaSet("unknown-key", .string("ignored unless read explicitly"))
        metadata.metaSet("trace", .string("replacement"))

        #expect(metadata.metaStr("trace") == "replacement")
        #expect(metadata.metaStr("Trace") == "case-sensitive")
        #expect(metadata.metaStr("TRACE") == nil)
        #expect(metadata.metaU64("attempt") == 7)
        #expect(metadata.metaStr("unknown-key") == "ignored unless read explicitly")
        #expect(metadata.metaLen == 4)
    }

    // r[verify rpc.metadata.sigils]
    @Test func metadataSigilsAreKeyStringConventions() {
        #expect(!metadataKeyIsRedacted("regular.metadata"))
        #expect(!metadataKeyIsNoPropagate("regular.metadata"))

        #expect(metadataKeyIsRedacted("#sensitive.metadata"))
        #expect(!metadataKeyIsNoPropagate("#sensitive.metadata"))

        #expect(!metadataKeyIsRedacted("-no-propagate-metadata"))
        #expect(metadataKeyIsNoPropagate("-no-propagate-metadata"))

        #expect(metadataKeyIsRedacted("-#sensitive-and-no-propagate-metadata"))
        #expect(metadataKeyIsNoPropagate("-#sensitive-and-no-propagate-metadata"))
    }
}

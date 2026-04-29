import Testing

@testable import VoxRuntime

struct SwiftValueDescriptorTests {
    @Test func concreteDescriptorCapturesLayout() {
        let desc = VoxSwiftTypeDescriptor.concrete(
            of: Int32.self,
            kind: VoxSwiftTypeKindPrimitive,
            primitiveKind: VoxSwiftPrimitiveI32,
            flags: VoxSwiftTypeFlagTrivial,
            schemaId: 0x1234
        )

        #expect(desc.magic == VoxSwiftTypeDescriptorMagic)
        #expect(desc.abiVersion == VoxSwiftTypeDescriptorAbiVersion)
        #expect(desc.size == UInt32(MemoryLayout<VoxSwiftTypeDescriptor>.stride))
        #expect(desc.kind == VoxSwiftTypeKindPrimitive)
        #expect(desc.primitiveKind == VoxSwiftPrimitiveI32)
        #expect(desc.schemaId == 0x1234)
        #expect(desc.typeMetadata != nil)
        #expect(desc.valueSize == MemoryLayout<Int32>.size)
        #expect(desc.valueStride == MemoryLayout<Int32>.stride)
        #expect(desc.valueAlign == MemoryLayout<Int32>.alignment)
    }
}

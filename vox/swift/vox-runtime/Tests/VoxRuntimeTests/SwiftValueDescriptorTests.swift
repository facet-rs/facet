import Foundation
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

  @Test func registryOwnsReservedDescriptorsAndFields() {
    let registry = VoxSwiftDescriptorRegistry()
    let fields = registry.allocateFields([
      VoxSwiftFieldDescriptor(name: .staticString("value"), schemaId: 0x11, type: nil, offset: 0)
    ])
    let reserved = registry.reserveDescriptor()
    let ptr = registry.defineDescriptor(
      reserved,
      as: VoxSwiftTypeDescriptor.concrete(
        of: Int32.self,
        kind: VoxSwiftTypeKindPrimitive,
        primitiveKind: VoxSwiftPrimitiveI32,
        schemaId: 0x55,
        fields: fields,
        fieldCount: 1
      )
    )

    #expect(registry.bySchemaId[0x55] == ptr)
    #expect(ptr.pointee.fieldCount == 1)
    #expect(ptr.pointee.fields?[0].name.len == 5)
  }

  @Test func registryStoresMethodRoots() {
    let registry = VoxSwiftDescriptorRegistry()
    let argsRoot = registry.insert(
      VoxSwiftTypeDescriptor.concrete(
        of: Int32.self,
        kind: VoxSwiftTypeKindPrimitive,
        primitiveKind: VoxSwiftPrimitiveI32,
        schemaId: 0x10
      )
    )
    let responseRoot = registry.insert(
      VoxSwiftTypeDescriptor.concrete(
        of: String.self,
        kind: VoxSwiftTypeKindString,
        schemaId: 0x20
      )
    )

    registry.defineMethod(methodId: 0x99, argsRoot: argsRoot, responseRoot: responseRoot)

    #expect(registry.methodById[0x99]?.argsRoot == argsRoot)
    #expect(registry.methodById[0x99]?.responseRoot == responseRoot)
  }

  @Test func codecConfigCapturesEntrypointAbi() {
    let registry = VoxSwiftDescriptorRegistry()
    let root = registry.insert(
      VoxSwiftTypeDescriptor.concrete(
        of: Int32.self,
        kind: VoxSwiftTypeKindPrimitive,
        primitiveKind: VoxSwiftPrimitiveI32,
        schemaId: 0x10
      )
    )
    let cbor = [UInt8](repeating: 0xA5, count: 4)

    cbor.withUnsafeBufferPointer { buffer in
      let config = VoxSwiftCodecConfig(
        methodId: 0x99,
        direction: VoxSwiftCodecDirectionArgs,
        localRoot: root,
        remoteSchemaCbor: VoxSwiftBytes(ptr: buffer.baseAddress, len: buffer.count)
      )

      #expect(config.abiVersion == VoxSwiftCodecConfigAbiVersion)
      #expect(config.size == UInt32(MemoryLayout<VoxSwiftCodecConfig>.stride))
      #expect(config.methodId == 0x99)
      #expect(config.direction == VoxSwiftCodecDirectionArgs)
      #expect(config.localRoot == root)
      #expect(config.remoteSchemaCbor.len == 4)
    }
  }

  @Test func preparesCodecThroughRustDylibWhenAvailable() throws {
    guard let dylibPath = ProcessInfo.processInfo.environment["VOX_SWIFT_ABI_DYLIB"],
      !dylibPath.isEmpty
    else {
      return
    }

    let registry = VoxSwiftDescriptorRegistry()
    let root = registry.insert(
      VoxSwiftTypeDescriptor.concrete(
        of: Int32.self,
        kind: VoxSwiftTypeKindPrimitive,
        primitiveKind: VoxSwiftPrimitiveI32,
        schemaId: 0x10
      )
    )
    let library = try VoxSwiftCodecDynamicLibrary(path: URL(fileURLWithPath: dylibPath))

    _ = try library.prepare(
      methodId: 0x99,
      direction: VoxSwiftCodecDirectionArgs,
      localRoot: root,
      remoteSchemaPayload: SchemaPayload(schemas: [], root: .concrete(0x10))
    )
  }
}

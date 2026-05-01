/// Wire-level status type returned by every Swift codec FFI entry point.
///
/// Numeric values 0..=8 mirror `DecodeStatus` from `vox-jit-abi`; values
/// 9..=11 are codec-lifecycle errors specific to the FFI boundary.
public typealias VoxSwiftStatus = UInt32

public let VoxSwiftStatusOK: VoxSwiftStatus = 0
public let VoxSwiftStatusUnexpectedEof: VoxSwiftStatus = 1
public let VoxSwiftStatusVarintOverflow: VoxSwiftStatus = 2
public let VoxSwiftStatusInvalidBool: VoxSwiftStatus = 3
public let VoxSwiftStatusInvalidUtf8: VoxSwiftStatus = 4
public let VoxSwiftStatusInvalidOptionTag: VoxSwiftStatus = 5
public let VoxSwiftStatusInvalidEnumDiscriminant: VoxSwiftStatus = 6
public let VoxSwiftStatusUnknownVariant: VoxSwiftStatus = 7
public let VoxSwiftStatusAllocFailed: VoxSwiftStatus = 8
public let VoxSwiftStatusBadABI: VoxSwiftStatus = 9
public let VoxSwiftStatusUnsupported: VoxSwiftStatus = 10
public let VoxSwiftStatusPanic: VoxSwiftStatus = 11

public let VoxSwiftTypeDescriptorMagic: UInt64 = 0x564F_5853_5746_5431
public let VoxSwiftTypeDescriptorAbiVersion: UInt32 = 1
public let VoxSwiftCodecConfigAbiVersion: UInt32 = 1

public let VoxSwiftCodecDirectionArgs: UInt32 = 0
public let VoxSwiftCodecDirectionResponse: UInt32 = 1

public let VoxSwiftTypeKindPrimitive: UInt32 = 0
public let VoxSwiftTypeKindStruct: UInt32 = 1
public let VoxSwiftTypeKindEnum: UInt32 = 2
public let VoxSwiftTypeKindTuple: UInt32 = 3
public let VoxSwiftTypeKindList: UInt32 = 4
public let VoxSwiftTypeKindMap: UInt32 = 5
public let VoxSwiftTypeKindArray: UInt32 = 6
public let VoxSwiftTypeKindOption: UInt32 = 7
public let VoxSwiftTypeKindString: UInt32 = 8
public let VoxSwiftTypeKindBytes: UInt32 = 9
public let VoxSwiftTypeKindChannel: UInt32 = 10
public let VoxSwiftTypeKindOpaque: UInt32 = 11

public let VoxSwiftPrimitiveUnit: UInt32 = 0
public let VoxSwiftPrimitiveBool: UInt32 = 1
public let VoxSwiftPrimitiveU8: UInt32 = 2
public let VoxSwiftPrimitiveU16: UInt32 = 3
public let VoxSwiftPrimitiveU32: UInt32 = 4
public let VoxSwiftPrimitiveU64: UInt32 = 5
public let VoxSwiftPrimitiveI8: UInt32 = 6
public let VoxSwiftPrimitiveI16: UInt32 = 7
public let VoxSwiftPrimitiveI32: UInt32 = 8
public let VoxSwiftPrimitiveI64: UInt32 = 9
public let VoxSwiftPrimitiveF32: UInt32 = 10
public let VoxSwiftPrimitiveF64: UInt32 = 11

public let VoxSwiftTypeFlagTrivial: UInt32 = 1 << 0
public let VoxSwiftTypeFlagBitwiseMovable: UInt32 = 1 << 1
public let VoxSwiftTypeFlagHasDefault: UInt32 = 1 << 2
public let VoxSwiftTypeFlagFixedLayout: UInt32 = 1 << 3

public let VoxSwiftFieldFlagHasDefault: UInt32 = 1 << 0

public typealias VoxSwiftDestroyFn =
  @convention(c) (
    _ value: UnsafeMutableRawPointer?,
    _ context: UnsafeRawPointer?
  ) -> Void

public typealias VoxSwiftCopyInitFn =
  @convention(c) (
    _ dst: UnsafeMutableRawPointer?,
    _ src: UnsafeRawPointer?,
    _ context: UnsafeRawPointer?
  ) -> VoxSwiftStatus

public typealias VoxSwiftTakeInitFn =
  @convention(c) (
    _ dst: UnsafeMutableRawPointer?,
    _ src: UnsafeMutableRawPointer?,
    _ context: UnsafeRawPointer?
  ) -> VoxSwiftStatus

public typealias VoxSwiftDefaultInitFn =
  @convention(c) (
    _ dst: UnsafeMutableRawPointer?,
    _ context: UnsafeRawPointer?
  ) -> VoxSwiftStatus

public typealias VoxSwiftEnumFieldVisitorFn =
  @convention(c) (
    _ visitorContext: UnsafeMutableRawPointer?,
    _ fieldIndex: Int,
    _ fieldPtr: UnsafeRawPointer?
  ) -> VoxSwiftStatus

public typealias VoxSwiftEnumTagFn =
  @convention(c) (
    _ value: UnsafeRawPointer?,
    _ context: UnsafeRawPointer?
  ) -> UInt32

public typealias VoxSwiftEnumProjectFn =
  @convention(c) (
    _ value: UnsafeRawPointer?,
    _ variantIndex: UInt32,
    _ visitorContext: UnsafeMutableRawPointer?,
    _ visitor: VoxSwiftEnumFieldVisitorFn?,
    _ context: UnsafeRawPointer?
  ) -> VoxSwiftStatus

public typealias VoxSwiftEnumInjectFn =
  @convention(c) (
    _ dst: UnsafeMutableRawPointer?,
    _ variantIndex: UInt32,
    _ fieldValues: UnsafePointer<UnsafeRawPointer?>?,
    _ fieldCount: Int,
    _ context: UnsafeRawPointer?
  ) -> VoxSwiftStatus

@frozen
public struct VoxSwiftBytes {
  public var ptr: UnsafePointer<UInt8>?
  public var len: Int

  public init(ptr: UnsafePointer<UInt8>?, len: Int) {
    self.ptr = ptr
    self.len = len
  }

  public static var empty: Self {
    .init(ptr: nil, len: 0)
  }

  public static func staticString(_ value: StaticString) -> Self {
    .init(ptr: value.utf8Start, len: value.utf8CodeUnitCount)
  }
}

@frozen
public struct VoxSwiftOwnedBytes {
  public var ptr: UnsafeMutablePointer<UInt8>?
  public var len: Int
  public var capacity: Int

  public init(ptr: UnsafeMutablePointer<UInt8>?, len: Int, capacity: Int) {
    self.ptr = ptr
    self.len = len
    self.capacity = capacity
  }

  public static var empty: Self {
    .init(ptr: nil, len: 0, capacity: 0)
  }
}

@frozen
public struct VoxSwiftValueWitnesses {
  public var destroy: VoxSwiftDestroyFn?
  public var copyInit: VoxSwiftCopyInitFn?
  public var takeInit: VoxSwiftTakeInitFn?
  public var defaultInit: VoxSwiftDefaultInitFn?

  public init(
    destroy: VoxSwiftDestroyFn? = nil,
    copyInit: VoxSwiftCopyInitFn? = nil,
    takeInit: VoxSwiftTakeInitFn? = nil,
    defaultInit: VoxSwiftDefaultInitFn? = nil
  ) {
    self.destroy = destroy
    self.copyInit = copyInit
    self.takeInit = takeInit
    self.defaultInit = defaultInit
  }
}

@frozen
public struct VoxSwiftEnumWitnesses {
  public var tag: VoxSwiftEnumTagFn?
  public var project: VoxSwiftEnumProjectFn?
  public var inject: VoxSwiftEnumInjectFn?

  public init(
    tag: VoxSwiftEnumTagFn? = nil,
    project: VoxSwiftEnumProjectFn? = nil,
    inject: VoxSwiftEnumInjectFn? = nil
  ) {
    self.tag = tag
    self.project = project
    self.inject = inject
  }
}

@frozen
public struct VoxSwiftFieldDescriptor {
  public var name: VoxSwiftBytes
  public var schemaId: UInt64
  public var type: UnsafePointer<VoxSwiftTypeDescriptor>?
  public var offset: Int
  public var flags: UInt32
  public var reserved: UInt32

  public init(
    name: VoxSwiftBytes,
    schemaId: UInt64,
    type: UnsafePointer<VoxSwiftTypeDescriptor>?,
    offset: Int,
    flags: UInt32 = 0
  ) {
    self.name = name
    self.schemaId = schemaId
    self.type = type
    self.offset = offset
    self.flags = flags
    self.reserved = 0
  }
}

@frozen
public struct VoxSwiftVariantDescriptor {
  public var name: VoxSwiftBytes
  public var index: UInt32
  public var reserved: UInt32
  public var fields: UnsafePointer<VoxSwiftFieldDescriptor>?
  public var fieldCount: Int

  public init(
    name: VoxSwiftBytes,
    index: UInt32,
    fields: UnsafePointer<VoxSwiftFieldDescriptor>?,
    fieldCount: Int
  ) {
    self.name = name
    self.index = index
    self.reserved = 0
    self.fields = fields
    self.fieldCount = fieldCount
  }
}

@frozen
public struct VoxSwiftTypeDescriptor {
  public var magic: UInt64
  public var abiVersion: UInt32
  public var size: UInt32
  public var kind: UInt32
  public var primitiveKind: UInt32
  public var flags: UInt32
  public var schemaId: UInt64
  public var typeMetadata: UnsafeRawPointer?
  public var valueSize: Int
  public var valueStride: Int
  public var valueAlign: Int
  public var typeArgs: UnsafePointer<UnsafePointer<VoxSwiftTypeDescriptor>?>?
  public var typeArgCount: Int
  public var fields: UnsafePointer<VoxSwiftFieldDescriptor>?
  public var fieldCount: Int
  public var variants: UnsafePointer<VoxSwiftVariantDescriptor>?
  public var variantCount: Int
  public var witnesses: VoxSwiftValueWitnesses
  public var enumWitnesses: VoxSwiftEnumWitnesses
  public var context: UnsafeRawPointer?

  public init(
    kind: UInt32,
    primitiveKind: UInt32 = VoxSwiftPrimitiveUnit,
    flags: UInt32 = 0,
    schemaId: UInt64,
    typeMetadata: UnsafeRawPointer?,
    valueSize: Int,
    valueStride: Int,
    valueAlign: Int,
    typeArgs: UnsafePointer<UnsafePointer<VoxSwiftTypeDescriptor>?>? = nil,
    typeArgCount: Int = 0,
    fields: UnsafePointer<VoxSwiftFieldDescriptor>? = nil,
    fieldCount: Int = 0,
    variants: UnsafePointer<VoxSwiftVariantDescriptor>? = nil,
    variantCount: Int = 0,
    witnesses: VoxSwiftValueWitnesses = .init(),
    enumWitnesses: VoxSwiftEnumWitnesses = .init(),
    context: UnsafeRawPointer? = nil
  ) {
    self.magic = VoxSwiftTypeDescriptorMagic
    self.abiVersion = VoxSwiftTypeDescriptorAbiVersion
    self.size = UInt32(MemoryLayout<VoxSwiftTypeDescriptor>.stride)
    self.kind = kind
    self.primitiveKind = primitiveKind
    self.flags = flags
    self.schemaId = schemaId
    self.typeMetadata = typeMetadata
    self.valueSize = valueSize
    self.valueStride = valueStride
    self.valueAlign = valueAlign
    self.typeArgs = typeArgs
    self.typeArgCount = typeArgCount
    self.fields = fields
    self.fieldCount = fieldCount
    self.variants = variants
    self.variantCount = variantCount
    self.witnesses = witnesses
    self.enumWitnesses = enumWitnesses
    self.context = context
  }

  public static func concrete<T>(
    of type: T.Type = T.self,
    kind: UInt32,
    primitiveKind: UInt32 = VoxSwiftPrimitiveUnit,
    flags: UInt32 = 0,
    schemaId: UInt64,
    typeArgs: UnsafePointer<UnsafePointer<VoxSwiftTypeDescriptor>?>? = nil,
    typeArgCount: Int = 0,
    fields: UnsafePointer<VoxSwiftFieldDescriptor>? = nil,
    fieldCount: Int = 0,
    variants: UnsafePointer<VoxSwiftVariantDescriptor>? = nil,
    variantCount: Int = 0,
    witnesses: VoxSwiftValueWitnesses = .init(),
    enumWitnesses: VoxSwiftEnumWitnesses = .init(),
    context: UnsafeRawPointer? = nil
  ) -> Self {
    .init(
      kind: kind,
      primitiveKind: primitiveKind,
      flags: flags,
      schemaId: schemaId,
      typeMetadata: unsafeBitCast(type, to: UnsafeRawPointer.self),
      valueSize: MemoryLayout<T>.size,
      valueStride: MemoryLayout<T>.stride,
      valueAlign: MemoryLayout<T>.alignment,
      typeArgs: typeArgs,
      typeArgCount: typeArgCount,
      fields: fields,
      fieldCount: fieldCount,
      variants: variants,
      variantCount: variantCount,
      witnesses: witnesses,
      enumWitnesses: enumWitnesses,
      context: context
    )
  }
}

@frozen
public struct VoxSwiftMethodValueDescriptorInfo {
  public var argsRoot: UnsafePointer<VoxSwiftTypeDescriptor>
  public var responseRoot: UnsafePointer<VoxSwiftTypeDescriptor>

  public init(
    argsRoot: UnsafePointer<VoxSwiftTypeDescriptor>,
    responseRoot: UnsafePointer<VoxSwiftTypeDescriptor>
  ) {
    self.argsRoot = argsRoot
    self.responseRoot = responseRoot
  }
}

@frozen
public struct VoxSwiftCodecConfig {
  public var abiVersion: UInt32
  public var size: UInt32
  public var methodId: UInt64
  public var direction: UInt32
  public var localRoot: UnsafePointer<VoxSwiftTypeDescriptor>?
  public var remoteSchemaCbor: VoxSwiftBytes

  public init(
    methodId: UInt64,
    direction: UInt32,
    localRoot: UnsafePointer<VoxSwiftTypeDescriptor>?,
    remoteSchemaCbor: VoxSwiftBytes
  ) {
    self.abiVersion = VoxSwiftCodecConfigAbiVersion
    self.size = UInt32(MemoryLayout<VoxSwiftCodecConfig>.stride)
    self.methodId = methodId
    self.direction = direction
    self.localRoot = localRoot
    self.remoteSchemaCbor = remoteSchemaCbor
  }
}

public final class VoxSwiftDescriptorRegistry: @unchecked Sendable {
  public private(set) var bySchemaId: [UInt64: UnsafePointer<VoxSwiftTypeDescriptor>] = [:]
  public private(set) var methodById: [UInt64: VoxSwiftMethodValueDescriptorInfo] = [:]

  private var cleanups: [() -> Void] = []

  public init() {}

  deinit {
    for cleanup in cleanups.reversed() {
      cleanup()
    }
  }

  public func insert(_ descriptor: VoxSwiftTypeDescriptor) -> UnsafePointer<VoxSwiftTypeDescriptor>
  {
    let ptr = allocateOne(descriptor)
    bySchemaId[descriptor.schemaId] = ptr
    return ptr
  }

  public func reserveDescriptor() -> UnsafeMutablePointer<VoxSwiftTypeDescriptor> {
    allocateOneMutable(
      VoxSwiftTypeDescriptor(
        kind: VoxSwiftTypeKindOpaque,
        schemaId: 0,
        typeMetadata: nil,
        valueSize: 0,
        valueStride: 0,
        valueAlign: 1
      )
    )
  }

  public func defineDescriptor(
    _ reserved: UnsafeMutablePointer<VoxSwiftTypeDescriptor>,
    as descriptor: VoxSwiftTypeDescriptor
  ) -> UnsafePointer<VoxSwiftTypeDescriptor> {
    reserved.pointee = descriptor
    bySchemaId[descriptor.schemaId] = UnsafePointer(reserved)
    return UnsafePointer(reserved)
  }

  public func defineMethod(
    methodId: UInt64,
    argsRoot: UnsafePointer<VoxSwiftTypeDescriptor>,
    responseRoot: UnsafePointer<VoxSwiftTypeDescriptor>
  ) {
    methodById[methodId] = VoxSwiftMethodValueDescriptorInfo(
      argsRoot: argsRoot,
      responseRoot: responseRoot
    )
  }

  public func allocateFields(_ values: [VoxSwiftFieldDescriptor]) -> UnsafePointer<
    VoxSwiftFieldDescriptor
  >? {
    allocateArray(values)
  }

  public func allocateVariants(_ values: [VoxSwiftVariantDescriptor]) -> UnsafePointer<
    VoxSwiftVariantDescriptor
  >? {
    allocateArray(values)
  }

  public func allocateTypeArgs(
    _ values: [UnsafePointer<VoxSwiftTypeDescriptor>?]
  ) -> UnsafePointer<UnsafePointer<VoxSwiftTypeDescriptor>?>? {
    allocateArray(values)
  }

  private func allocateOne<T>(_ value: T) -> UnsafePointer<T> {
    UnsafePointer(allocateOneMutable(value))
  }

  private func allocateOneMutable<T>(_ value: T) -> UnsafeMutablePointer<T> {
    let ptr = UnsafeMutablePointer<T>.allocate(capacity: 1)
    ptr.initialize(to: value)
    cleanups.append {
      ptr.deinitialize(count: 1)
      ptr.deallocate()
    }
    return ptr
  }

  private func allocateArray<T>(_ values: [T]) -> UnsafePointer<T>? {
    if values.isEmpty {
      return nil
    }

    let ptr = UnsafeMutablePointer<T>.allocate(capacity: values.count)
    ptr.initialize(from: values, count: values.count)
    cleanups.append {
      ptr.deinitialize(count: values.count)
      ptr.deallocate()
    }
    return UnsafePointer(ptr)
  }
}

import Foundation

#if canImport(Darwin)
  import Darwin
#elseif canImport(Glibc)
  import Glibc
#endif

public typealias VoxSwiftCodecPrepareFn =
  @convention(c) (
    _ config: UnsafeRawPointer?,
    _ outCodec: UnsafeMutableRawPointer?
  ) -> VoxSwiftStatus

public typealias VoxSwiftCodecReleaseFn =
  @convention(c) (
    _ codec: UnsafeMutableRawPointer?
  ) -> Void

public typealias VoxSwiftCodecEncodeFn =
  @convention(c) (
    _ codec: UnsafeRawPointer?,
    _ value: UnsafeRawPointer?,
    _ outBytes: UnsafeMutableRawPointer?
  ) -> VoxSwiftStatus

public typealias VoxSwiftCodecDecodeFn =
  @convention(c) (
    _ codec: UnsafeRawPointer?,
    _ inputPtr: UnsafePointer<UInt8>?,
    _ inputLen: Int,
    _ dst: UnsafeMutableRawPointer?
  ) -> VoxSwiftStatus

public typealias VoxSwiftOwnedBytesFreeFn =
  @convention(c) (
    _ bytes: UnsafeMutableRawPointer?
  ) -> Void

public enum VoxSwiftCodecFfiError: Error, CustomStringConvertible {
  case openFailed(path: String, message: String)
  case missingSymbol(path: String, symbol: String)
  case badStatus(operation: String, status: VoxSwiftStatus)
  case nullCodec
  case nullOutputBytes

  public var description: String {
    switch self {
    case .openFailed(let path, let message):
      return "failed to dlopen \(path): \(message)"
    case .missingSymbol(let path, let symbol):
      return "missing \(symbol) in \(path)"
    case .badStatus(let operation, let status):
      return "\(operation) returned status \(status)"
    case .nullCodec:
      return "codec prepare returned a null handle"
    case .nullOutputBytes:
      return "codec encode returned null output bytes"
    }
  }
}

public final class VoxSwiftCodecDynamicLibrary: @unchecked Sendable {
  public let path: URL

  private let handle: UnsafeMutableRawPointer
  private let prepareFn: VoxSwiftCodecPrepareFn
  private let releaseFn: VoxSwiftCodecReleaseFn
  private let encodeFn: VoxSwiftCodecEncodeFn
  private let decodeFn: VoxSwiftCodecDecodeFn
  private let freeBytesFn: VoxSwiftOwnedBytesFreeFn

  public init(path: URL) throws {
    self.path = path
    guard let handle = path.path.withCString({ dlopen($0, RTLD_NOW | RTLD_LOCAL) }) else {
      throw VoxSwiftCodecFfiError.openFailed(
        path: path.path,
        message: String(cString: dlerror())
      )
    }

    do {
      self.handle = handle
      self.prepareFn = try Self.load(handle, path: path.path, symbol: "vox_swift_codec_prepare_v1")
      self.releaseFn = try Self.load(handle, path: path.path, symbol: "vox_swift_codec_release_v1")
      self.encodeFn = try Self.load(handle, path: path.path, symbol: "vox_swift_codec_encode_v1")
      self.decodeFn = try Self.load(handle, path: path.path, symbol: "vox_swift_codec_decode_v1")
      self.freeBytesFn = try Self.load(
        handle,
        path: path.path,
        symbol: "vox_swift_owned_bytes_free_v1"
      )
    } catch {
      dlclose(handle)
      throw error
    }
  }

  deinit {
    dlclose(handle)
  }

  public func prepare(
    methodId: UInt64,
    direction: UInt32,
    localRoot: UnsafePointer<VoxSwiftTypeDescriptor>,
    remoteSchemaPayload: SchemaPayload
  ) throws -> VoxSwiftCodec {
    try prepare(
      methodId: methodId,
      direction: direction,
      localRoot: localRoot,
      remoteSchemaCbor: remoteSchemaPayload.encodeCbor()
    )
  }

  public func prepare(
    methodId: UInt64,
    direction: UInt32,
    localRoot: UnsafePointer<VoxSwiftTypeDescriptor>,
    remoteSchemaCbor: [UInt8]
  ) throws -> VoxSwiftCodec {
    var codec: UnsafeMutableRawPointer?
    let status = remoteSchemaCbor.withUnsafeBufferPointer { buffer in
      var config = VoxSwiftCodecConfig(
        methodId: methodId,
        direction: direction,
        localRoot: localRoot,
        remoteSchemaCbor: VoxSwiftBytes(ptr: buffer.baseAddress, len: buffer.count)
      )
      return withUnsafePointer(to: &config) { configPtr in
        withUnsafeMutablePointer(to: &codec) { codecPtr in
          prepareFn(UnsafeRawPointer(configPtr), UnsafeMutableRawPointer(codecPtr))
        }
      }
    }

    guard status == VoxSwiftStatusOK else {
      throw VoxSwiftCodecFfiError.badStatus(operation: "vox_swift_codec_prepare_v1", status: status)
    }
    guard let codec else {
      throw VoxSwiftCodecFfiError.nullCodec
    }

    return VoxSwiftCodec(
      handle: codec,
      releaseFn: releaseFn,
      encodeFn: encodeFn,
      decodeFn: decodeFn,
      freeBytesFn: freeBytesFn
    )
  }

  private static func load<Function>(
    _ handle: UnsafeMutableRawPointer,
    path: String,
    symbol: String
  ) throws -> Function {
    guard let rawSymbol = dlsym(handle, symbol) else {
      throw VoxSwiftCodecFfiError.missingSymbol(path: path, symbol: symbol)
    }
    return unsafeBitCast(rawSymbol, to: Function.self)
  }
}

public final class VoxSwiftCodec: @unchecked Sendable {
  private let releaseFn: VoxSwiftCodecReleaseFn
  private let encodeFn: VoxSwiftCodecEncodeFn
  private let decodeFn: VoxSwiftCodecDecodeFn
  private let freeBytesFn: VoxSwiftOwnedBytesFreeFn
  private var handle: UnsafeMutableRawPointer?

  fileprivate init(
    handle: UnsafeMutableRawPointer,
    releaseFn: @escaping VoxSwiftCodecReleaseFn,
    encodeFn: @escaping VoxSwiftCodecEncodeFn,
    decodeFn: @escaping VoxSwiftCodecDecodeFn,
    freeBytesFn: @escaping VoxSwiftOwnedBytesFreeFn
  ) {
    self.handle = handle
    self.releaseFn = releaseFn
    self.encodeFn = encodeFn
    self.decodeFn = decodeFn
    self.freeBytesFn = freeBytesFn
  }

  deinit {
    if let handle {
      releaseFn(handle)
    }
  }

  public func encode<Value>(_ value: inout Value) throws -> [UInt8] {
    guard let handle else {
      throw VoxSwiftCodecFfiError.nullCodec
    }

    var out = VoxSwiftOwnedBytes.empty
    let status = withUnsafePointer(to: &value) { valuePtr in
      withUnsafeMutablePointer(to: &out) { outPtr in
        encodeFn(
          UnsafeRawPointer(handle), UnsafeRawPointer(valuePtr), UnsafeMutableRawPointer(outPtr))
      }
    }
    defer {
      withUnsafeMutablePointer(to: &out) { outPtr in
        freeBytesFn(UnsafeMutableRawPointer(outPtr))
      }
    }

    guard status == VoxSwiftStatusOK else {
      throw VoxSwiftCodecFfiError.badStatus(operation: "vox_swift_codec_encode_v1", status: status)
    }
    guard out.len == 0 || out.ptr != nil else {
      throw VoxSwiftCodecFfiError.nullOutputBytes
    }

    return Array(UnsafeBufferPointer(start: out.ptr, count: out.len))
  }

  public func decode<Value>(_ input: [UInt8], into value: inout Value) throws {
    guard let handle else {
      throw VoxSwiftCodecFfiError.nullCodec
    }

    let status = input.withUnsafeBufferPointer { buffer in
      withUnsafeMutablePointer(to: &value) { valuePtr in
        decodeFn(
          UnsafeRawPointer(handle),
          buffer.baseAddress,
          buffer.count,
          UnsafeMutableRawPointer(valuePtr)
        )
      }
    }

    guard status == VoxSwiftStatusOK else {
      throw VoxSwiftCodecFfiError.badStatus(operation: "vox_swift_codec_decode_v1", status: status)
    }
  }
}

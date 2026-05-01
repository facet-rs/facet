// End-to-end demonstration: a Swift enum gets calibrated into a
// `VoxValueLayout` by the Rust dylib's probe (running once, at
// calibration time, against pre-built sample buffers), and a fresh
// `Foo.ok(31)` is then constructed from Swift code by reading the
// layout's match/store patterns and emitting the stores ourselves —
// no FFI call goes from Swift back into Rust to do the stores.

import Foundation
import Testing

#if canImport(Darwin)
  import Darwin
#elseif canImport(Glibc)
  import Glibc
#endif

@testable import VoxRuntime

/// The Swift enum the test calibrates. One payload-bearing variant
/// (`ok(UInt64)`) and one unit variant (`err`). Layout is whatever the
/// Swift compiler chose; the probe doesn't care.
private enum Foo {
  case ok(UInt64)
  case err
}

private final class CodecFFI {
  let handle: UnsafeMutableRawPointer
  let create: VoxLayoutArenaCreateFn
  let destroy: VoxLayoutArenaDestroyFn
  let probe: VoxProbeTwoVariantEnumFn
  let probeNiche: VoxProbeOptionNicheFn
  let encode: VoxLayoutEncodeFn
  let decode: VoxLayoutDecodeFn
  let freeBytes: VoxSwiftOwnedBytesFreeFn

  init(path: String) throws {
    guard let h = path.withCString({ dlopen($0, RTLD_NOW | RTLD_LOCAL) }) else {
      throw NSError(
        domain: "VoxValueLayoutTests",
        code: 1,
        userInfo: [NSLocalizedDescriptionKey: "dlopen failed: \(String(cString: dlerror()))"])
    }
    self.handle = h
    self.create = try Self.loadFn(h, "vox_swift_layout_arena_create_v1")
    self.destroy = try Self.loadFn(h, "vox_swift_layout_arena_destroy_v1")
    self.probe = try Self.loadFn(h, "vox_swift_probe_two_variant_enum_v1")
    self.probeNiche = try Self.loadFn(h, "vox_swift_probe_option_niche_v1")
    self.encode = try Self.loadFn(h, "vox_swift_layout_encode_v1")
    self.decode = try Self.loadFn(h, "vox_swift_layout_decode_v1")
    self.freeBytes = try Self.loadFn(h, "vox_swift_owned_bytes_free_v1")
  }

  deinit {
    dlclose(handle)
  }

  private static func loadFn<F>(_ h: UnsafeMutableRawPointer, _ name: String) throws -> F {
    guard let sym = dlsym(h, name) else {
      throw NSError(
        domain: "VoxValueLayoutTests",
        code: 2,
        userInfo: [NSLocalizedDescriptionKey: "dlsym failed for \(name)"])
    }
    return unsafeBitCast(sym, to: F.self)
  }
}

private func swiftAbiDylibPath() -> String? {
  let fileManager = FileManager.default
  let envPath = ProcessInfo.processInfo.environment["VOX_SWIFT_ABI_DYLIB"]
  let cwd = URL(fileURLWithPath: fileManager.currentDirectoryPath)
  let candidates = [
    envPath,
    cwd.appendingPathComponent("target/debug/libvox_swift_abi.dylib").path,
    cwd.appendingPathComponent("target/debug/libvox_swift_abi.so").path,
  ].compactMap { $0 }.filter { !$0.isEmpty }
  return candidates.first { fileManager.fileExists(atPath: $0) }
}

/// Snapshot the bytes of `value` into a freshly allocated buffer the
/// caller owns. We can't pass `&value` directly to FFI because Swift may
/// move the value during the call; copying once into stable memory keeps
/// pointers valid for the duration of the probe call.
private func snapshotBytes<T>(of value: T, size: Int) -> [UInt8] {
  var v = value
  var bytes = [UInt8](repeating: 0, count: size)
  withUnsafeBytes(of: &v) { src in
    bytes.withUnsafeMutableBytes { dst in
      dst.copyMemory(from: src)
    }
  }
  return bytes
}

struct VoxValueLayoutTests {
  @Test func probesSwiftEnumAndConstructsOkVariantFromLayout() throws {
    guard let dylibPath = swiftAbiDylibPath() else {
      // No dylib built yet; skip silently. (Same convention as the
      // existing SwiftValueDescriptorTests dylib-backed test.)
      return
    }
    let ffi = try CodecFFI(path: dylibPath)

    let valueSize = MemoryLayout<Foo>.size
    let valueAlign = MemoryLayout<Foo>.alignment

    // Build three sample buffers. (Snapshotted into [UInt8] so we can
    // hand stable pointers to the probe.)
    let okZeroBytes = snapshotBytes(of: Foo.ok(0), size: valueSize)
    let okMaxBytes = snapshotBytes(of: Foo.ok(0xDEAD_BEEF_CAFE_BABE), size: valueSize)
    let errBytes = snapshotBytes(of: Foo.err, size: valueSize)

    let arena = ffi.create()!
    defer { ffi.destroy(arena) }

    // The Ok variant's payload is a UInt64. Build a primitive layout
    // describing it; we cheat for now and synthesize one inline (the
    // arena-allocated layout pointer would normally come from the same
    // Rust calibration pipeline).
    var u64Layout = VoxValueLayout(
      fields: nil,
      variants: nil,
      kind: VoxValueLayoutKind.primitive.rawValue,
      size: VoxPrimitiveKind.u64.size,
      align: 8,
      primitiveKind: VoxPrimitiveKind.u64.rawValue,
      fieldCount: 0,
      variantCount: 0,
      opaqueHandle: 0,
      reserved: 0
    )

    let okName = Array("Ok".utf8)
    let errName = Array("Err".utf8)

    var layoutPtr: UnsafePointer<VoxValueLayout>? = nil
    let status = withUnsafePointer(to: &u64Layout) { u64Ptr in
      okZeroBytes.withUnsafeBufferPointer { okZeroBuf in
        okMaxBytes.withUnsafeBufferPointer { okMaxBuf in
          errBytes.withUnsafeBufferPointer { errBuf in
            okName.withUnsafeBufferPointer { okNameBuf in
              errName.withUnsafeBufferPointer { errNameBuf in
                withUnsafeMutablePointer(to: &layoutPtr) { outPtr in
                  ffi.probe(
                    arena,
                    UInt32(valueSize),
                    UInt32(valueAlign),
                    UnsafeRawPointer(okZeroBuf.baseAddress),
                    UnsafeRawPointer(okMaxBuf.baseAddress),
                    UnsafeRawPointer(errBuf.baseAddress),
                    UnsafeRawPointer(okNameBuf.baseAddress),
                    okNameBuf.count,
                    UnsafeRawPointer(u64Ptr),
                    UnsafeRawPointer(errNameBuf.baseAddress),
                    errNameBuf.count,
                    nil,  // err has no payload
                    UnsafeMutableRawPointer(outPtr)
                  )
                }
              }
            }
          }
        }
      }
    }

    #expect(status == VoxSwiftStatusOK)
    #expect(layoutPtr != nil)
    let layout = layoutPtr!.pointee
    #expect(layout.kindEnum == .enum)
    #expect(layout.variantCount == 2)

    // Find the Ok variant by name.
    var okVariant: VoxVariantLayout? = nil
    for variant in layout.variantsBuffer {
      if variant.name.string == "Ok" {
        okVariant = variant
      }
    }
    let ok = try #require(okVariant)
    #expect(!ok.isDefault)
    #expect(ok.fieldCount == 1)

    // Construct Foo.ok(31) from the layout: zero a fresh buffer, apply
    // Ok's store_pattern (writes the discriminant byte(s)), then store
    // the UInt64 payload at the calibrated offset. Two store loops, all
    // in Swift — no FFI call into the dylib for the writes.
    var storage = [UInt8](repeating: 0, count: Int(layout.size))
    let result: Foo = storage.withUnsafeMutableBufferPointer { buf in
      let dst = UnsafeMutableRawPointer(buf.baseAddress!)
      dst.applyStorePattern(ok.storePatternBuffer)
      let payloadOffset = Int(ok.fieldsBuffer[0].offset)
      dst.advanced(by: payloadOffset).storeBytes(of: UInt64(31), as: UInt64.self)
      return dst.load(as: Foo.self)
    }

    if case .ok(let v) = result {
      #expect(v == 31)
    } else {
      Issue.record("expected Foo.ok(31), got something else")
    }

    // Sanity check the match pattern: a real Foo.ok value's bytes match
    // Ok's match pattern; a real Foo.err's bytes don't.
    var realOk = Foo.ok(99)
    var realErr = Foo.err
    let okMatches = withUnsafePointer(to: &realOk) { ptr in
      UnsafeRawPointer(ptr).matches(ok.matchPatternBuffer)
    }
    let errMatches = withUnsafePointer(to: &realErr) { ptr in
      UnsafeRawPointer(ptr).matches(ok.matchPatternBuffer)
    }
    #expect(okMatches)
    #expect(!errMatches)
  }

  /// Real RPC-shaped round-trip: a Swift value goes through the Rust
  /// dylib's codec to postcard bytes and back, and the Swift caller
  /// gets the equivalent value out the other side. This is what the
  /// real codec does on every encode and every decode of an RPC
  /// request/response — only here we plug both sides into the same
  /// process to verify end-to-end.
  @Test func encodesAndDecodesFooViaRustCodec() throws {
    guard let dylibPath = swiftAbiDylibPath() else {
      return
    }
    let ffi = try CodecFFI(path: dylibPath)

    let valueSize = MemoryLayout<Foo>.size
    let valueAlign = MemoryLayout<Foo>.alignment

    let okZeroBytes = snapshotBytes(of: Foo.ok(0), size: valueSize)
    let okMaxBytes = snapshotBytes(of: Foo.ok(0xDEAD_BEEF_CAFE_BABE), size: valueSize)
    let errBytes = snapshotBytes(of: Foo.err, size: valueSize)

    let arena = ffi.create()!
    defer { ffi.destroy(arena) }

    var u64Layout = VoxValueLayout(
      fields: nil,
      variants: nil,
      kind: VoxValueLayoutKind.primitive.rawValue,
      size: VoxPrimitiveKind.u64.size,
      align: 8,
      primitiveKind: VoxPrimitiveKind.u64.rawValue,
      fieldCount: 0,
      variantCount: 0,
      opaqueHandle: 0,
      reserved: 0
    )

    let okName = Array("Ok".utf8)
    let errName = Array("Err".utf8)
    var layoutPtr: UnsafeRawPointer? = nil

    let probeStatus = withUnsafePointer(to: &u64Layout) { u64Ptr in
      okZeroBytes.withUnsafeBufferPointer { z in
        okMaxBytes.withUnsafeBufferPointer { m in
          errBytes.withUnsafeBufferPointer { e in
            okName.withUnsafeBufferPointer { okN in
              errName.withUnsafeBufferPointer { errN in
                withUnsafeMutablePointer(to: &layoutPtr) { outPtr in
                  ffi.probe(
                    arena,
                    UInt32(valueSize),
                    UInt32(valueAlign),
                    UnsafeRawPointer(z.baseAddress),
                    UnsafeRawPointer(m.baseAddress),
                    UnsafeRawPointer(e.baseAddress),
                    UnsafeRawPointer(okN.baseAddress),
                    okN.count,
                    UnsafeRawPointer(u64Ptr),
                    UnsafeRawPointer(errN.baseAddress),
                    errN.count,
                    nil,
                    UnsafeMutableRawPointer(outPtr)
                  )
                }
              }
            }
          }
        }
      }
    }
    #expect(probeStatus == VoxSwiftStatusOK)
    let layout = try #require(layoutPtr)

    // Encode Foo.ok(31) via the Rust codec. The Swift side never reads
    // or writes any byte of `value` itself: the dylib's postcard codec
    // walks the calibrated layout against the value's memory and emits
    // postcard bytes.
    var value = Foo.ok(31)
    var encoded = VoxSwiftOwnedBytes.empty
    let encodeStatus = withUnsafePointer(to: &value) { valuePtr in
      withUnsafeMutablePointer(to: &encoded) { outPtr in
        ffi.encode(
          layout,
          UnsafeRawPointer(valuePtr),
          UnsafeMutableRawPointer(outPtr)
        )
      }
    }
    #expect(encodeStatus == VoxSwiftStatusOK)
    #expect(encoded.len > 0)
    #expect(encoded.ptr != nil)

    // Postcard wire bytes: variant 0 (Ok) as a varint = 0x00, then
    // u64 31 as a varint = 0x1F. Two bytes total.
    let encodedSlice = UnsafeBufferPointer(start: encoded.ptr, count: encoded.len)
    let encodedArray = Array(encodedSlice)
    #expect(encodedArray == [0x00, 0x1F])

    // Decode those bytes back into a fresh Foo via the Rust codec. The
    // Swift side allocates uninit storage for Foo, hands the dylib a
    // pointer, and gets a fully initialised Foo back.
    let storage = UnsafeMutablePointer<Foo>.allocate(capacity: 1)
    defer { storage.deallocate() }
    var consumed = 0
    let decodeStatus = ffi.decode(
      layout,
      UnsafeRawPointer(encoded.ptr),
      encoded.len,
      UnsafeMutableRawPointer(storage),
      &consumed
    )
    #expect(decodeStatus == VoxSwiftStatusOK)
    #expect(consumed == encoded.len)

    let decoded = storage.pointee
    if case .ok(let v) = decoded {
      #expect(v == 31)
    } else {
      Issue.record("expected Foo.ok(31) after decode")
    }

    // Release the owned bytes the dylib handed us.
    withUnsafeMutablePointer(to: &encoded) { outPtr in
      ffi.freeBytes(UnsafeMutableRawPointer(outPtr))
    }
  }

  /// Niche-filled round-trip: a Swift `Optional<UnsafeRawPointer>` has
  /// no separate discriminant — `nil` is "all 8 bytes zero," any other
  /// value is `.some`. The niche probe records None's pattern across
  /// the whole value; Some is the catch-all. We then encode/decode a
  /// `.some(0xDEADBEEFCAFEBABE)` through the codec FFI and verify the
  /// payload bytes round-trip even though the variant has no separate
  /// tag region in memory.
  @Test func roundTripsNicheFilledOptionalPointerViaRustCodec() throws {
    guard let dylibPath = swiftAbiDylibPath() else {
      return
    }
    let ffi = try CodecFFI(path: dylibPath)

    let valueSize = MemoryLayout<UnsafeRawPointer?>.size
    let valueAlign = MemoryLayout<UnsafeRawPointer?>.alignment

    let noneSample: UnsafeRawPointer? = nil
    let someASample: UnsafeRawPointer? = UnsafeRawPointer(bitPattern: 0x1111_1111_1111_1111)
    let someBSample: UnsafeRawPointer? = UnsafeRawPointer(bitPattern: 0x2222_2222_2222_2222)

    let noneBytes = snapshotBytes(of: noneSample, size: valueSize)
    let someABytes = snapshotBytes(of: someASample, size: valueSize)
    let someBBytes = snapshotBytes(of: someBSample, size: valueSize)

    let arena = ffi.create()!
    defer { ffi.destroy(arena) }

    // The catch-all's payload is a u64 (the pointer's bit pattern).
    var u64Layout = VoxValueLayout(
      fields: nil,
      variants: nil,
      kind: VoxValueLayoutKind.primitive.rawValue,
      size: VoxPrimitiveKind.u64.size,
      align: 8,
      primitiveKind: VoxPrimitiveKind.u64.rawValue,
      fieldCount: 0,
      variantCount: 0,
      opaqueHandle: 0,
      reserved: 0
    )

    let noneName = Array("None".utf8)
    let someName = Array("Some".utf8)
    var layoutPtr: UnsafeRawPointer? = nil

    let probeStatus = withUnsafePointer(to: &u64Layout) { u64Ptr in
      noneBytes.withUnsafeBufferPointer { n in
        someABytes.withUnsafeBufferPointer { a in
          someBBytes.withUnsafeBufferPointer { b in
            noneName.withUnsafeBufferPointer { nn in
              someName.withUnsafeBufferPointer { sn in
                withUnsafeMutablePointer(to: &layoutPtr) { outPtr in
                  ffi.probeNiche(
                    arena,
                    UInt32(valueSize),
                    UInt32(valueAlign),
                    UnsafeRawPointer(n.baseAddress),
                    UnsafeRawPointer(a.baseAddress),
                    UnsafeRawPointer(b.baseAddress),
                    UnsafeRawPointer(nn.baseAddress),
                    nn.count,
                    UnsafeRawPointer(sn.baseAddress),
                    sn.count,
                    UnsafeRawPointer(u64Ptr),
                    UnsafeMutableRawPointer(outPtr)
                  )
                }
              }
            }
          }
        }
      }
    }
    #expect(probeStatus == VoxSwiftStatusOK)
    let layout = try #require(layoutPtr)

    // Round-trip a non-nil Optional through the codec.
    var value: UnsafeRawPointer? = UnsafeRawPointer(bitPattern: UInt(0xDEAD_BEEF_CAFE_BABE))
    var encoded = VoxSwiftOwnedBytes.empty
    let encodeStatus = withUnsafePointer(to: &value) { valuePtr in
      withUnsafeMutablePointer(to: &encoded) { outPtr in
        ffi.encode(layout, UnsafeRawPointer(valuePtr), UnsafeMutableRawPointer(outPtr))
      }
    }
    #expect(encodeStatus == VoxSwiftStatusOK)
    #expect(encoded.len > 0)

    // Decode back into a fresh Optional.
    var decoded: UnsafeRawPointer? = nil
    var consumed = 0
    let decodeStatus = withUnsafeMutablePointer(to: &decoded) { decodedPtr in
      ffi.decode(
        layout,
        UnsafeRawPointer(encoded.ptr),
        encoded.len,
        UnsafeMutableRawPointer(decodedPtr),
        &consumed
      )
    }
    #expect(decodeStatus == VoxSwiftStatusOK)
    #expect(consumed == encoded.len)
    #expect(decoded == value)

    withUnsafeMutablePointer(to: &encoded) { outPtr in
      ffi.freeBytes(UnsafeMutableRawPointer(outPtr))
    }

    // Also round-trip nil — a niche-filled None goes through the same
    // code path but with the catch-all's payload missing.
    var nilValue: UnsafeRawPointer? = nil
    var nilEncoded = VoxSwiftOwnedBytes.empty
    let nilEncodeStatus = withUnsafePointer(to: &nilValue) { valuePtr in
      withUnsafeMutablePointer(to: &nilEncoded) { outPtr in
        ffi.encode(layout, UnsafeRawPointer(valuePtr), UnsafeMutableRawPointer(outPtr))
      }
    }
    #expect(nilEncodeStatus == VoxSwiftStatusOK)

    var nilDecoded: UnsafeRawPointer? = UnsafeRawPointer(bitPattern: 0x1234)
    var nilConsumed = 0
    let nilDecodeStatus = withUnsafeMutablePointer(to: &nilDecoded) { decodedPtr in
      ffi.decode(
        layout,
        UnsafeRawPointer(nilEncoded.ptr),
        nilEncoded.len,
        UnsafeMutableRawPointer(decodedPtr),
        &nilConsumed
      )
    }
    #expect(nilDecodeStatus == VoxSwiftStatusOK)
    #expect(nilDecoded == nil)

    withUnsafeMutablePointer(to: &nilEncoded) { outPtr in
      ffi.freeBytes(UnsafeMutableRawPointer(outPtr))
    }
  }
}

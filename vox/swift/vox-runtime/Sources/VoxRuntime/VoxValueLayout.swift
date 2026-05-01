// Swift mirror of vox_jit_cal::value_layout.
//
// These `@frozen` structs match the Rust `#[repr(C)]` definitions
// byte-for-byte (field order chosen on the Rust side to avoid implicit
// padding under 64-bit C ABI). Swift code reads them directly through
// `UnsafePointer<Vox*>?` returned by the calibration FFI; no bridging,
// no per-field copy.
//
// Hot-path byte writes (storing a discriminant pattern, storing a
// payload field at a calibrated offset) are emitted from Swift code
// reading these structs — no FFI call goes from Swift back into Rust to
// do a single store. See `notes/codec-architecture.md`.

import Foundation

#if canImport(Darwin)
  import Darwin
#elseif canImport(Glibc)
  import Glibc
#endif

/// Tag for `VoxValueLayout.kind`. Numeric values are part of the ABI
/// (mirror of `vox_jit_cal::ValueLayoutKind`).
public enum VoxValueLayoutKind: UInt32 {
  case primitive = 0
  case `struct` = 1
  case `enum` = 2
  case opaque = 3
}

/// Numeric identifier for a fixed-size scalar primitive. Mirror of
/// `vox_jit_cal::PrimitiveKind`.
public enum VoxPrimitiveKind: UInt32 {
  case unit = 0
  case bool = 1
  case u8 = 2
  case u16 = 3
  case u32 = 4
  case u64 = 5
  case i8 = 6
  case i16 = 7
  case i32 = 8
  case i64 = 9
  case f32 = 10
  case f64 = 11

  public var size: UInt32 {
    switch self {
    case .bool, .u8, .i8: return 1
    case .u16, .i16: return 2
    case .u32, .i32, .f32: return 4
    case .u64, .i64, .f64: return 8
    case .unit: return 0
    }
  }
}

/// Borrowed UTF-8 byte slice. Mirror of `LayoutBytes` (16 bytes, 8-aligned).
@frozen
public struct VoxLayoutBytes {
  public var ptr: UnsafePointer<UInt8>?
  public var len: Int

  public init(ptr: UnsafePointer<UInt8>?, len: Int) {
    self.ptr = ptr
    self.len = len
  }

  public static var empty: Self { .init(ptr: nil, len: 0) }

  /// Decode the bytes as a Swift `String`, or `nil` if the storage is
  /// null or the bytes aren't valid UTF-8.
  public var string: String? {
    guard let ptr, len > 0 else { return nil }
    let buf = UnsafeBufferPointer(start: ptr, count: len)
    return String(bytes: buf, encoding: .utf8)
  }
}

/// One byte (or a masked group of bits) of a variant's match or store
/// pattern. Mirror of `BytePattern` (8 bytes, 4-aligned).
@frozen
public struct VoxBytePattern {
  public var offset: UInt32
  public var value: UInt8
  public var mask: UInt8
  public var reserved: UInt16

  public init(offset: UInt32, value: UInt8, mask: UInt8 = 0xFF) {
    self.offset = offset
    self.value = value
    self.mask = mask
    self.reserved = 0
  }
}

/// One field of a struct, or one piece of an enum-variant payload.
/// Mirror of `FieldLayout` (32 bytes, 8-aligned).
@frozen
public struct VoxFieldLayout {
  public var name: VoxLayoutBytes
  public var layout: UnsafePointer<VoxValueLayout>?
  public var offset: UInt32
  public var pad: UInt32

  public init(name: VoxLayoutBytes, layout: UnsafePointer<VoxValueLayout>?, offset: UInt32) {
    self.name = name
    self.layout = layout
    self.offset = offset
    self.pad = 0
  }
}

/// One variant of an enum-shaped value.
///
/// `match_pattern` describes the bytes that must match for this variant
/// to be selected. An empty `match_pattern` (count = 0) makes the
/// variant the default / catch-all (must be last in the variant list).
///
/// `store_pattern` describes the bytes to write to construct this
/// variant. For niche-filled variants where the payload write itself
/// produces the variant (a non-null pointer makes a `Some`), it's empty.
///
/// Mirror of `VariantLayout` (56 bytes, 8-aligned).
@frozen
public struct VoxVariantLayout {
  public var name: VoxLayoutBytes
  public var matchPattern: UnsafePointer<VoxBytePattern>?
  public var storePattern: UnsafePointer<VoxBytePattern>?
  public var fields: UnsafePointer<VoxFieldLayout>?
  public var matchPatternCount: UInt32
  public var storePatternCount: UInt32
  public var fieldCount: UInt32
  public var pad: UInt32

  public var matchPatternBuffer: UnsafeBufferPointer<VoxBytePattern> {
    UnsafeBufferPointer(start: matchPattern, count: Int(matchPatternCount))
  }

  public var storePatternBuffer: UnsafeBufferPointer<VoxBytePattern> {
    UnsafeBufferPointer(start: storePattern, count: Int(storePatternCount))
  }

  public var fieldsBuffer: UnsafeBufferPointer<VoxFieldLayout> {
    UnsafeBufferPointer(start: fields, count: Int(fieldCount))
  }

  /// `true` for the catch-all variant (empty match pattern).
  public var isDefault: Bool { matchPatternCount == 0 }
}

/// Layout of one value. Tagged-struct representation: `kind` selects
/// which trailing fields are meaningful. Mirror of `ValueLayout`
/// (48 bytes, 8-aligned).
@frozen
public struct VoxValueLayout {
  public var fields: UnsafePointer<VoxFieldLayout>?
  public var variants: UnsafePointer<VoxVariantLayout>?
  public var kind: UInt32
  public var size: UInt32
  public var align: UInt32
  public var primitiveKind: UInt32
  public var fieldCount: UInt32
  public var variantCount: UInt32
  public var opaqueHandle: UInt32
  public var reserved: UInt32

  public var kindEnum: VoxValueLayoutKind? {
    VoxValueLayoutKind(rawValue: kind)
  }

  public var primitiveKindEnum: VoxPrimitiveKind? {
    VoxPrimitiveKind(rawValue: primitiveKind)
  }

  public var fieldsBuffer: UnsafeBufferPointer<VoxFieldLayout> {
    UnsafeBufferPointer(start: fields, count: Int(fieldCount))
  }

  public var variantsBuffer: UnsafeBufferPointer<VoxVariantLayout> {
    UnsafeBufferPointer(start: variants, count: Int(variantCount))
  }
}

// MARK: - Layout-driven byte operations
//
// These are implemented in Swift, against Swift memory, reading the
// layout's plain integers. Crucially they do NOT call into the Rust
// dylib to do the stores: the same loop a Cranelift codegen would emit
// inline is just spelled out here in Swift. When we eventually grow a
// Swift-side codegen, it inlines these stores into the generated method.

extension UnsafeMutableRawPointer {
  /// Apply each entry of `pattern` to the bytes at `self`. Full-byte
  /// patterns (`mask == 0xFF`) collapse to a plain store; partial-bit
  /// patterns do a load / mask / OR / store.
  public func applyStorePattern(_ pattern: UnsafeBufferPointer<VoxBytePattern>) {
    for entry in pattern {
      let dst = self.advanced(by: Int(entry.offset))
      if entry.mask == 0xFF {
        dst.storeBytes(of: entry.value, as: UInt8.self)
      } else {
        let existing = dst.load(as: UInt8.self)
        let merged = (existing & ~entry.mask) | (entry.value & entry.mask)
        dst.storeBytes(of: merged, as: UInt8.self)
      }
    }
  }
}

extension UnsafeRawPointer {
  /// `true` if every entry of `pattern` matches the byte at the
  /// corresponding offset. An empty pattern matches by definition (the
  /// default variant).
  public func matches(_ pattern: UnsafeBufferPointer<VoxBytePattern>) -> Bool {
    if pattern.isEmpty { return true }
    for entry in pattern {
      let actual = self.advanced(by: Int(entry.offset)).load(as: UInt8.self)
      if (actual & entry.mask) != (entry.value & entry.mask) {
        return false
      }
    }
    return true
  }
}

// MARK: - Calibration FFI signatures
//
// The actual symbols live in the vox-swift-abi cdylib; callers dlopen
// the dylib and `dlsym` the entry points by name (the project's
// established pattern — see `VoxSwiftCodecDynamicLibrary`). These
// typealiases describe the C function signatures so the dlsym result
// can be cast to something callable.

public typealias VoxLayoutArenaHandle = OpaquePointer

public typealias VoxLayoutArenaCreateFn =
  @convention(c) () -> VoxLayoutArenaHandle?

public typealias VoxLayoutArenaDestroyFn =
  @convention(c) (VoxLayoutArenaHandle?) -> Void

/// Function signature uses `UnsafeRawPointer` for layout pointers because
/// Swift's `@convention(c)` rejects `UnsafePointer<VoxValueLayout>` —
/// `VoxValueLayout` is a `@frozen` Swift struct and `@convention(c)`
/// requires Objective-C-representable parameter types. Callers cast the
/// raw pointer back to `UnsafePointer<VoxValueLayout>` before reading.
public typealias VoxProbeTwoVariantEnumFn =
  @convention(c) (
    _ arena: VoxLayoutArenaHandle?,
    _ valueSize: UInt32,
    _ valueAlign: UInt32,
    _ variantAZeroBytes: UnsafeRawPointer?,
    _ variantAMaxBytes: UnsafeRawPointer?,
    _ variantBZeroBytes: UnsafeRawPointer?,
    _ variantANamePtr: UnsafeRawPointer?,
    _ variantANameLen: Int,
    _ variantAFieldLayout: UnsafeRawPointer?,
    _ variantBNamePtr: UnsafeRawPointer?,
    _ variantBNameLen: Int,
    _ variantBFieldLayout: UnsafeRawPointer?,
    _ outLayout: UnsafeMutableRawPointer?
  ) -> UInt32

/// Probe a niche-filled `Optional`-shaped enum: one variant is recognised
/// by an exact byte pattern across the whole value, the other is
/// "anything else." See `vox_swift_probe_option_niche_v1`.
public typealias VoxProbeOptionNicheFn =
  @convention(c) (
    _ arena: VoxLayoutArenaHandle?,
    _ valueSize: UInt32,
    _ valueAlign: UInt32,
    _ nicheVariantBytes: UnsafeRawPointer?,
    _ catchallABytes: UnsafeRawPointer?,
    _ catchallBBytes: UnsafeRawPointer?,
    _ nicheNamePtr: UnsafeRawPointer?,
    _ nicheNameLen: Int,
    _ catchallNamePtr: UnsafeRawPointer?,
    _ catchallNameLen: Int,
    _ catchallFieldLayout: UnsafeRawPointer?,
    _ outLayout: UnsafeMutableRawPointer?
  ) -> UInt32

/// Build a struct-shaped `VoxValueLayout` from explicit field info. See
/// `vox_swift_make_struct_layout_v1`. Field info is passed as four
/// parallel arrays: name pointers, name lengths, offsets, inner layout
/// pointers.
public typealias VoxMakeStructLayoutFn =
  @convention(c) (
    _ arena: VoxLayoutArenaHandle?,
    _ size: UInt32,
    _ align: UInt32,
    _ fieldCount: Int,
    _ fieldNamePtrs: UnsafeRawPointer?,
    _ fieldNameLens: UnsafeRawPointer?,
    _ fieldOffsets: UnsafeRawPointer?,
    _ fieldLayouts: UnsafeRawPointer?,
    _ outLayout: UnsafeMutableRawPointer?
  ) -> UInt32

/// Encode a Swift value (whose layout is described by `layout`) into a
/// freshly-allocated postcard byte buffer. Returns 0 on success; on
/// success, `outBytes->ptr/len/capacity` describes the encoded buffer
/// and the caller must release it through `vox_swift_owned_bytes_free_v1`.
public typealias VoxLayoutEncodeFn =
  @convention(c) (
    _ layout: UnsafeRawPointer?,
    _ valuePtr: UnsafeRawPointer?,
    _ outBytes: UnsafeMutableRawPointer?
  ) -> UInt32

/// Decode postcard bytes into the value-shaped storage at `dst`.
/// `outConsumed` (optional) receives the number of input bytes used.
public typealias VoxLayoutDecodeFn =
  @convention(c) (
    _ layout: UnsafeRawPointer?,
    _ inputPtr: UnsafeRawPointer?,
    _ inputLen: Int,
    _ dst: UnsafeMutableRawPointer?,
    _ outConsumed: UnsafeMutablePointer<Int>?
  ) -> UInt32

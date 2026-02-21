import Foundation
import CRoamShmFfi

@inline(__always)
func atomicLoadU32Acquire(_ pointer: UnsafeRawPointer) -> UInt32 {
    roam_atomic_load_u32_acquire(pointer.assumingMemoryBound(to: UInt32.self))
}

@inline(__always)
func atomicStoreU32Release(_ pointer: UnsafeMutableRawPointer, _ value: UInt32) {
    roam_atomic_store_u32_release(pointer.assumingMemoryBound(to: UInt32.self), value)
}

@inline(__always)
func atomicCompareExchangeU32(
    _ pointer: UnsafeMutableRawPointer,
    expected: inout UInt32,
    desired: UInt32
) -> Bool {
    roam_atomic_compare_exchange_u32(
        pointer.assumingMemoryBound(to: UInt32.self),
        &expected,
        desired
    ) != 0
}

@inline(__always)
func atomicFetchAddU32(_ pointer: UnsafeMutableRawPointer, _ value: UInt32) -> UInt32 {
    roam_atomic_fetch_add_u32(pointer.assumingMemoryBound(to: UInt32.self), value)
}

@inline(__always)
func atomicLoadU64Acquire(_ pointer: UnsafeRawPointer) -> UInt64 {
    roam_atomic_load_u64_acquire(pointer.assumingMemoryBound(to: UInt64.self))
}

@inline(__always)
func atomicStoreU64Release(_ pointer: UnsafeMutableRawPointer, _ value: UInt64) {
    roam_atomic_store_u64_release(pointer.assumingMemoryBound(to: UInt64.self), value)
}

@inline(__always)
func atomicCompareExchangeU64(
    _ pointer: UnsafeMutableRawPointer,
    expected: inout UInt64,
    desired: UInt64
) -> Bool {
    roam_atomic_compare_exchange_u64(
        pointer.assumingMemoryBound(to: UInt64.self),
        &expected,
        desired
    ) != 0
}

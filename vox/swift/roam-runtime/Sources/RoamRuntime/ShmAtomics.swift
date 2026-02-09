import Foundation
import CRoamShm

@inline(__always)
func atomicLoadU32Relaxed(_ pointer: UnsafeRawPointer) -> UInt32 {
    roam_atomic_load_u32_relaxed(pointer.assumingMemoryBound(to: UInt32.self))
}

@inline(__always)
func atomicLoadU32Acquire(_ pointer: UnsafeRawPointer) -> UInt32 {
    roam_atomic_load_u32_acquire(pointer.assumingMemoryBound(to: UInt32.self))
}

@inline(__always)
func atomicStoreU32Release(_ pointer: UnsafeMutableRawPointer, _ value: UInt32) {
    roam_atomic_store_u32_release(pointer.assumingMemoryBound(to: UInt32.self), value)
}

@inline(__always)
func atomicLoadU64Acquire(_ pointer: UnsafeRawPointer) -> UInt64 {
    roam_atomic_load_u64_acquire(pointer.assumingMemoryBound(to: UInt64.self))
}

@inline(__always)
func atomicStoreU64Release(_ pointer: UnsafeMutableRawPointer, _ value: UInt64) {
    roam_atomic_store_u64_release(pointer.assumingMemoryBound(to: UInt64.self), value)
}

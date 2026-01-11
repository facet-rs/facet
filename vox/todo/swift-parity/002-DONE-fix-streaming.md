# Phase 002: Fix Streaming RPC

**Status**: DONE

## Problem

Streaming tests failed with "received Response before Close - protocol violation (received so far: [])".

## Root Cause

**NOT a channel ordering issue** - the actual problem was in Postcard decoding.

`decodeU32` and `decodeU16` were using **fixed-width** decoding (4 bytes, 2 bytes) instead of **varint** encoding as specified by Postcard format.

The request payload `[5, 1]` should decode as:
- varint 5 (count)
- varint 1 (channelId)

But `decodeU32` expected 4 bytes, causing a "truncated" error. The error was caught and an `InvalidPayload` response was sent immediately, before any handler code ran.

## Fix

Changed `decodeU16`, `decodeI16`, and `decodeU32` in `Postcard.swift` to use varint decoding:

```swift
public func decodeU16(from data: Data, offset: inout Int) throws -> UInt16 {
    // Postcard uses varint for u16
    let v = try decodeVarint(from: data, offset: &offset)
    guard v <= UInt64(UInt16.max) else { throw PostcardError.overflow }
    return UInt16(v)
}

public func decodeI16(from data: Data, offset: inout Int) throws -> Int16 {
    // Postcard uses zigzag + varint for signed integers
    let zigzag = try decodeVarint(from: data, offset: &offset)
    let unsigned = UInt16(truncatingIfNeeded: zigzag)
    return Int16(bitPattern: (unsigned >> 1) ^ (0 &- (unsigned & 1)))
}

public func decodeU32(from data: Data, offset: inout Int) throws -> UInt32 {
    // Postcard uses varint for u32
    let v = try decodeVarint(from: data, offset: &offset)
    guard v <= UInt64(UInt32.max) else { throw PostcardError.overflow }
    return UInt32(v)
}
```

Also added `overflow` case to `PostcardError`.

## Results

All 17 spec tests now pass:
- Protocol tests: 7/7
- Client mode tests: 3/3
- Streaming tests: 3/3
- Testbed tests: 4/4

All 39 Swift runtime tests still pass.

## Files Modified

- `swift/roam-runtime/Sources/RoamRuntime/Postcard.swift` - Fixed varint decoding
- `swift/roam-runtime/Sources/RoamRuntime/Transport.swift` - Improved debugLog to use stderr
- `swift/roam-runtime/Sources/RoamRuntime/Driver.swift` - Cleaned up debug logging

## Debugging Journey

1. Initially suspected channel message ordering (Tx.send not awaiting)
2. Added debug logging to trace message flow
3. Discovered only Response was being sent, no Data/Close
4. Found handler wasn't being called at all
5. Discovered "truncated" error from decodeU32
6. Root cause: Postcard uses varint for u16/u32, not fixed-width

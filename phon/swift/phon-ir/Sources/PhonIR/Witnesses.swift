// Generic witness factories for the typed-path access nodes.
//
// The tricky ARC/layout logic (borrow-for-encode, move-for-decode) lives here,
// generic over the element/inner type, so codegen emits a tiny call —
// `OptionWitness.of(String.self)`, `BytesWitness.string`, `SeqWitness.of(Point.self)`
// — instead of per-type closure bodies. Enum `tag`/`project`/`inject` stay
// codegen-emitted (they switch over concrete cases).
//
// Contract (see Descriptor.swift): encode borrows into engine scratch (the source
// outlives the encode); decode moves out of scratch (the engine then frees it
// without deinitializing).

import PhonSchema

// MARK: - Option

public extension OptionWitness {
    /// For an `Optional<T>`. Relies on Swift placing the some-payload `T` at offset
    /// 0 of the `Optional` storage (niche or tag-after-payload) — true for the
    /// types phon carries.
    static func of<T>(_ type: T.Type) -> OptionWitness {
        OptionWitness(
            projectSome: { option, scratch in
                // Presence: a proper (retained) load just for the nil check.
                guard option.assumingMemoryBound(to: T?.self).pointee != nil else { return false }
                // Borrow the inner `T` (offset 0) — stable for the encode's lifetime.
                if MemoryLayout<T>.size > 0 {
                    scratch.copyMemory(from: option, byteCount: MemoryLayout<T>.size)
                }
                return true
            },
            initSome: { option, value in
                let v = value.assumingMemoryBound(to: T.self).move()
                option.assumingMemoryBound(to: T?.self).initialize(to: v)
            },
            initNone: { option in
                option.assumingMemoryBound(to: T?.self).initialize(to: nil)
            }
        )
    }
}

// MARK: - Bytes (bulk runs)

public extension BytesWitness {
    /// A `String` (UTF-8 bytes, validated on decode).
    static var string: BytesWitness {
        BytesWitness(
            count: { $0.assumingMemoryBound(to: String.self).pointee.utf8.count },
            copyInto: { field, dst in
                var s = field.assumingMemoryBound(to: String.self).pointee
                s.withUTF8 { buf in
                    if buf.count > 0 { dst.copyMemory(from: buf.baseAddress!, byteCount: buf.count) }
                }
            },
            construct: { field, src, count in
                let buf = UnsafeBufferPointer(start: src.assumingMemoryBound(to: UInt8.self), count: count)
                guard let s = String(validating: buf, as: UTF8.self) else { return false }
                field.assumingMemoryBound(to: String.self).initialize(to: s)
                return true
            }
        )
    }

    /// A `[UInt8]` byte run.
    static var byteArray: BytesWitness {
        BytesWitness(
            count: { $0.assumingMemoryBound(to: [UInt8].self).pointee.count },
            copyInto: { field, dst in
                field.assumingMemoryBound(to: [UInt8].self).pointee.withUnsafeBytes { buf in
                    if buf.count > 0 { dst.copyMemory(from: buf.baseAddress!, byteCount: buf.count) }
                }
            },
            construct: { field, src, count in
                let buf = UnsafeBufferPointer(start: src.assumingMemoryBound(to: UInt8.self), count: count)
                field.assumingMemoryBound(to: [UInt8].self).initialize(to: Array(buf))
                return true
            }
        )
    }

    /// A `[Element]` of trivially-copyable scalar `Element` carried as a bulk run.
    static func scalarArray<Element>(_ type: Element.Type) -> BytesWitness {
        BytesWitness(
            count: { $0.assumingMemoryBound(to: [Element].self).pointee.count },
            copyInto: { field, dst in
                field.assumingMemoryBound(to: [Element].self).pointee.withUnsafeBytes { buf in
                    if buf.count > 0 { dst.copyMemory(from: buf.baseAddress!, byteCount: buf.count) }
                }
            },
            construct: { field, src, count in
                let buf = UnsafeBufferPointer(start: src.assumingMemoryBound(to: Element.self), count: count)
                field.assumingMemoryBound(to: [Element].self).initialize(to: Array(buf))
                return true
            }
        )
    }
}

// MARK: - Sequence (per-element)

public extension SeqWitness {
    /// A `[Element]` of structured elements; each element is moved out of the
    /// engine scratch buffer exactly once.
    static func of<Element>(_ type: Element.Type) -> SeqWitness {
        SeqWitness(
            count: { $0.assumingMemoryBound(to: [Element].self).pointee.count },
            copyElements: { handle, dst in
                handle.assumingMemoryBound(to: [Element].self).pointee.withUnsafeBytes { buf in
                    if buf.count > 0 { dst.copyMemory(from: buf.baseAddress!, byteCount: buf.count) }
                }
            },
            construct: { handle, src, count in
                var array: [Element] = []
                array.reserveCapacity(count)
                for i in 0..<count {
                    let value = src.advanced(by: i * MemoryLayout<Element>.stride)
                        .assumingMemoryBound(to: Element.self)
                        .move()
                    array.append(value)
                }
                handle.assumingMemoryBound(to: [Element].self).initialize(to: array)
            }
        )
    }

    /// A `Set<Element>` carried on the wire as a unique sequence. Encode walks a
    /// sorted snapshot so the byte stream is deterministic.
    static func setOf<Element: Hashable & Comparable>(_ type: Element.Type) -> SeqWitness {
        SeqWitness(
            count: { $0.assumingMemoryBound(to: Set<Element>.self).pointee.count },
            copyElements: { handle, dst in
                let values = handle.assumingMemoryBound(to: Set<Element>.self).pointee.sorted()
                for (i, value) in values.enumerated() {
                    dst.advanced(by: i * MemoryLayout<Element>.stride)
                        .assumingMemoryBound(to: Element.self)
                        .initialize(to: value)
                }
            },
            destroyElements: { elements, count in
                for i in 0..<count {
                    elements.advanced(by: i * MemoryLayout<Element>.stride)
                        .assumingMemoryBound(to: Element.self)
                        .deinitialize(count: 1)
                }
            },
            construct: { handle, src, count in
                var set = Set<Element>(minimumCapacity: count)
                for i in 0..<count {
                    let value = src.advanced(by: i * MemoryLayout<Element>.stride)
                        .assumingMemoryBound(to: Element.self)
                        .move()
                    set.insert(value)
                }
                handle.assumingMemoryBound(to: Set<Element>.self).initialize(to: set)
            }
        )
    }
}

// MARK: - Map (string-keyed)

public extension MapWitness {
    /// A `[String: Value]`, entries emitted in sorted-key order.
    static func stringKeyed<Value>(_ type: Value.Type) -> MapWitness {
        let kStride = MemoryLayout<String>.stride
        let vStride = MemoryLayout<Value>.stride
        return MapWitness(
            count: { $0.assumingMemoryBound(to: [String: Value].self).pointee.count },
            projectEntries: { handle, keys, values in
                let dict = handle.assumingMemoryBound(to: [String: Value].self).pointee
                for (i, e) in dict.sorted(by: { $0.key < $1.key }).enumerated() {
                    keys.advanced(by: i * kStride).assumingMemoryBound(to: String.self).initialize(to: e.key)
                    values.advanced(by: i * vStride).assumingMemoryBound(to: Value.self).initialize(to: e.value)
                }
            },
            destroyEntries: { keys, values, count in
                for i in 0..<count {
                    keys.advanced(by: i * kStride).assumingMemoryBound(to: String.self).deinitialize(count: 1)
                    values.advanced(by: i * vStride).assumingMemoryBound(to: Value.self).deinitialize(count: 1)
                }
            },
            initWithCapacity: { handle, cap in
                handle.assumingMemoryBound(to: [String: Value].self).initialize(to: .init(minimumCapacity: cap))
            },
            insert: { handle, key, value in
                let k = key.assumingMemoryBound(to: String.self).move()
                let v = value.assumingMemoryBound(to: Value.self).move()
                handle.assumingMemoryBound(to: [String: Value].self).pointee[k] = v
            }
        )
    }
}

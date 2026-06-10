import Foundation
import PhonIR

// vox carries opaque payloads and byte fields as Swift `Data` (what codegen's
// `swift_type_base` maps `Vec<u8>`/opaque to). This `BytesWitness` reads and
// builds a `Data` for the typed codec — the binding-specific counterpart to
// phon's generic `.string`/`.byteArray` factories (kept here so the phon package
// stays Foundation-free).
public extension BytesWitness {
    static var data: BytesWitness {
        BytesWitness(
            count: { $0.assumingMemoryBound(to: Data.self).pointee.count },
            copyInto: { field, dst in
                field.assumingMemoryBound(to: Data.self).pointee.withUnsafeBytes { buf in
                    if buf.count > 0 { dst.copyMemory(from: buf.baseAddress!, byteCount: buf.count) }
                }
            },
            construct: { field, src, count in
                field.assumingMemoryBound(to: Data.self)
                    .initialize(to: Data(UnsafeRawBufferPointer(start: count > 0 ? src : nil, count: count)))
                return true
            }
        )
    }
}

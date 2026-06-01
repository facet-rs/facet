// The typed (memory) side of the IR: a `MemProgram` moves bytes between the wire
// and a value's in-memory layout, at offsets the descriptor supplies. Where
// `Program`/`Op` builds a dynamic `Value` on a stack, a `MemProgram` reads and
// writes the value's memory directly.
//
// Mirrors the `MemOp`/`MemProgram` half of `rust/phon-ir/src/ir.rs`. This first
// cut models fixed scalars (and, via folded offsets, in-place records — a whole
// nested fixed struct dissolves into a flat run of `scalar` copies). Owned
// sequences, options, enums, and maps grow this enum as the engine learns them.

/// A lowered typed program: base-relative memory copies, in wire order.
public typealias MemProgram = [MemOp]

/// One typed step. The base pointer is supplied at run time; `offset` is relative
/// to it.
public indirect enum MemOp {
    /// Copy a run of `size` bytes between memory at `offset` and the wire, which
    /// is first padded to `align`. A single scalar, or a fused run of adjacent
    /// scalars. Encode reads memory and writes the wire; decode reads the wire and
    /// writes memory. Sound only where host byte order equals the wire's
    /// (little-endian), which every phon target is.
    case scalar(offset: Int, size: Int, align: Int)
}

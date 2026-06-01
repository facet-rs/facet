// Coalesce adjacent scalar copies that are contiguous in *both* the wire and
// memory into one larger copy — the specialization the typed IR exists for. A flat
// struct whose wire layout matches its memory layout collapses to a single memcpy;
// a `repr`-padded struct collapses to a copy per contiguous run. This is the
// same-schema fast path *emerging* from lowering, not a hand-written branch.
//
// Two consecutive ops fuse when the second needs no wire padding after the first
// (wire-contiguous) and its memory offset continues the first's (mem-contiguous).
// The bytes produced are identical — only fewer, larger copies — so fusing is
// transparent to correctness.
//
// Mirrors `rust/phon-ir/src/ir.rs::fuse`.

/// Fuse a program's contiguous scalar runs.
public func fuse(_ program: MemProgram) -> MemProgram {
    var out: MemProgram = []
    out.reserveCapacity(program.count)
    // `nil` once a variable-length op makes the static wire position unknown;
    // scalars after that can't be proven contiguous, so they aren't fused.
    var wirePos: Int? = 0

    for op in program {
        switch op {
        case .scalar(let offset, let size, let align):
            // Padding the wire needs to align this scalar at the current position.
            let pad = wirePos.map { p in (align - (p & (align - 1))) & (align - 1) }
            // Fuse iff no wire pad AND the previous op is a scalar whose memory run
            // ends exactly where this one begins.
            if pad == 0, case .scalar(let po, let ps, let pa)? = out.last, po + ps == offset {
                out[out.count - 1] = .scalar(offset: po, size: ps + size, align: pa)
            } else {
                out.append(.scalar(offset: offset, size: size, align: align))
            }
            wirePos = wirePos.map { $0 + (pad ?? 0) + size }
        case .writeDefault:
            // Reads no wire bytes — leaves the static position untouched, but breaks
            // a fuse run (it is not a scalar).
            out.append(op)
        default:
            // Variable-length / data-directed ops poison the static wire position.
            out.append(op)
            wirePos = nil
        }
    }
    return out
}

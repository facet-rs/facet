#ifndef CPHON_JIT_STENCILS_H
#define CPHON_JIT_STENCILS_H

#include <stdint.h>
#include <stddef.h>

// r[impl ir.stencils]
typedef struct {
    const uint8_t *wire;
    const uint8_t *wire_start;
    const uint8_t *wire_end;
    uint8_t *base;
    const uint64_t *prog;
    uint64_t status;
} PhonJITDecodeCtx;

// r[impl ir.stencils]
typedef struct {
    const uint8_t *base;
    const uint64_t *prog;
    uint8_t *out;
    const uint8_t *out_start;
    const uint8_t *out_end;
    uint64_t status;
} PhonJITEncodeCtx;

// r[impl ir.stencils]
const uint8_t *phon_jit_smoke_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_smoke_len(void);
// r[impl ir.stencils]
void phon_jit_flush_instruction_cache(void *start, size_t len);

// r[impl ir.stencils]
const uint8_t *phon_jit_scalar_decode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_scalar_decode_len(void);
// r[impl ir.stencils]
size_t phon_jit_scalar_decode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_scalar_encode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_scalar_encode_len(void);
// r[impl ir.stencils]
size_t phon_jit_scalar_encode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_done_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_done_len(void);

#endif

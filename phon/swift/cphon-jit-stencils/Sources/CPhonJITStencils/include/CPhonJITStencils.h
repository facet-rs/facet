#ifndef CPHON_JIT_STENCILS_H
#define CPHON_JIT_STENCILS_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

// r[impl ir.stencils]
typedef struct {
    const uint8_t *wire;
    const uint8_t *wire_start;
    const uint8_t *wire_end;
    uint8_t *base;
    const uint64_t *prog;
    uint64_t status;
    uint64_t aux;
    uint8_t *scratch;
} PhonJITDecodeCtx;

// r[impl ir.stencils]
typedef struct {
    const uint8_t *base;
    const uint64_t *prog;
    uint8_t *out;
    const uint8_t *out_start;
    const uint8_t *out_end;
    uint64_t status;
    uint8_t *scratch;
} PhonJITEncodeCtx;

// r[impl ir.stencils]
typedef struct {
    uint64_t field_offset;
    uint64_t scratch_offset;
    uintptr_t some_entry;
    uintptr_t some_prog;
    uintptr_t witness_ctx;
    uintptr_t project_some;
    uintptr_t init_some;
    uintptr_t init_none;
} PhonJITOptionInfo;

bool phon_jit_option_project_some(const void *ctx, const uint8_t *option, uint8_t *scratch);
void phon_jit_option_init_some(const void *ctx, uint8_t *option, uint8_t *scratch);
void phon_jit_option_init_none(const void *ctx, uint8_t *option);

// r[impl ir.stencils]
const uint8_t *phon_jit_smoke_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_smoke_len(void);
// r[impl ir.stencils]
void phon_jit_flush_instruction_cache(void *start, size_t len);

// r[impl ir.stencils]
uintptr_t phon_jit_option_project_some_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_option_init_some_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_option_init_none_ptr(void);

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
const uint8_t *phon_jit_option_decode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_option_decode_len(void);
// r[impl ir.stencils]
size_t phon_jit_option_decode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_option_encode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_option_encode_len(void);
// r[impl ir.stencils]
size_t phon_jit_option_encode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_done_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_done_len(void);

#endif

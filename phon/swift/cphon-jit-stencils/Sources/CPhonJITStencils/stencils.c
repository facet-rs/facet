#include "CPhonJITStencils.h"

#include <libkern/OSCacheControl.h>

extern const uint8_t phon_jit_smoke_start[];
extern const uint8_t phon_jit_smoke_end[];
extern const uint8_t phon_jit_scalar_decode_start[];
extern const uint8_t phon_jit_scalar_decode_next[];
extern const uint8_t phon_jit_scalar_decode_end[];
extern const uint8_t phon_jit_scalar_encode_start[];
extern const uint8_t phon_jit_scalar_encode_next[];
extern const uint8_t phon_jit_scalar_encode_end[];
extern const uint8_t phon_jit_option_decode_start[];
extern const uint8_t phon_jit_option_decode_next[];
extern const uint8_t phon_jit_option_decode_end[];
extern const uint8_t phon_jit_option_encode_start[];
extern const uint8_t phon_jit_option_encode_next[];
extern const uint8_t phon_jit_option_encode_end[];
extern const uint8_t phon_jit_done_start[];
extern const uint8_t phon_jit_done_end[];

// r[impl ir.stencils]
static bool phon_jit_call_option_project_some(const void *ctx, const uint8_t *option, uint8_t *scratch) {
    return phon_jit_option_project_some(ctx, option, scratch);
}

// r[impl ir.stencils]
static void phon_jit_call_option_init_some(const void *ctx, uint8_t *option, uint8_t *scratch) {
    phon_jit_option_init_some(ctx, option, scratch);
}

// r[impl ir.stencils]
static void phon_jit_call_option_init_none(const void *ctx, uint8_t *option) {
    phon_jit_option_init_none(ctx, option);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_smoke_bytes(void) {
    return phon_jit_smoke_start;
}

// r[impl ir.stencils]
size_t phon_jit_smoke_len(void) {
    return (size_t)(phon_jit_smoke_end - phon_jit_smoke_start);
}

// r[impl ir.stencils]
void phon_jit_flush_instruction_cache(void *start, size_t len) {
    sys_icache_invalidate(start, len);
}

// r[impl ir.stencils]
uintptr_t phon_jit_option_project_some_ptr(void) {
    return (uintptr_t)&phon_jit_call_option_project_some;
}

// r[impl ir.stencils]
uintptr_t phon_jit_option_init_some_ptr(void) {
    return (uintptr_t)&phon_jit_call_option_init_some;
}

// r[impl ir.stencils]
uintptr_t phon_jit_option_init_none_ptr(void) {
    return (uintptr_t)&phon_jit_call_option_init_none;
}

// r[impl ir.stencils]
const uint8_t *phon_jit_scalar_decode_bytes(void) {
    return phon_jit_scalar_decode_start;
}

// r[impl ir.stencils]
size_t phon_jit_scalar_decode_len(void) {
    return (size_t)(phon_jit_scalar_decode_end - phon_jit_scalar_decode_start);
}

// r[impl ir.stencils]
size_t phon_jit_scalar_decode_branch_offset(void) {
    return (size_t)(phon_jit_scalar_decode_next - phon_jit_scalar_decode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_scalar_encode_bytes(void) {
    return phon_jit_scalar_encode_start;
}

// r[impl ir.stencils]
size_t phon_jit_scalar_encode_len(void) {
    return (size_t)(phon_jit_scalar_encode_end - phon_jit_scalar_encode_start);
}

// r[impl ir.stencils]
size_t phon_jit_scalar_encode_branch_offset(void) {
    return (size_t)(phon_jit_scalar_encode_next - phon_jit_scalar_encode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_option_decode_bytes(void) {
    return phon_jit_option_decode_start;
}

// r[impl ir.stencils]
size_t phon_jit_option_decode_len(void) {
    return (size_t)(phon_jit_option_decode_end - phon_jit_option_decode_start);
}

// r[impl ir.stencils]
size_t phon_jit_option_decode_branch_offset(void) {
    return (size_t)(phon_jit_option_decode_next - phon_jit_option_decode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_option_encode_bytes(void) {
    return phon_jit_option_encode_start;
}

// r[impl ir.stencils]
size_t phon_jit_option_encode_len(void) {
    return (size_t)(phon_jit_option_encode_end - phon_jit_option_encode_start);
}

// r[impl ir.stencils]
size_t phon_jit_option_encode_branch_offset(void) {
    return (size_t)(phon_jit_option_encode_next - phon_jit_option_encode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_done_bytes(void) {
    return phon_jit_done_start;
}

// r[impl ir.stencils]
size_t phon_jit_done_len(void) {
    return (size_t)(phon_jit_done_end - phon_jit_done_start);
}

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

typedef struct {
    uint64_t field_offset;
    uint64_t stride;
    uint64_t elem_align;
    uintptr_t witness_ctx;
    uintptr_t count;
    uintptr_t copy_into;
    uintptr_t construct;
    uintptr_t decode;
    uintptr_t encode;
} PhonJITBytesInfo;

typedef struct {
    uint64_t field_offset;
    const void *variants;
    uint64_t variant_count;
    uintptr_t witness_ctx;
    uintptr_t tag;
    uintptr_t project;
    uintptr_t destroy;
    uintptr_t inject;
    uintptr_t decode;
    uintptr_t encode;
    const uint32_t *writer_only;
    uint64_t writer_only_count;
} PhonJITEnumInfo;

typedef struct {
    uint32_t wire_index;
    uint32_t reader_local_index;
    uint64_t scratch_offset;
    uintptr_t payload_entry;
    uintptr_t payload_prog;
} PhonJITEnumVariantInfo;

typedef struct {
    uint64_t field_offset;
    uint64_t stride;
    uint64_t elem_align;
    uint64_t min_wire;
    uint64_t unique;
    uintptr_t witness_ctx;
    uintptr_t count;
    uintptr_t copy_elements;
    uintptr_t destroy_elements;
    uintptr_t construct;
    uintptr_t element_entry;
    uintptr_t element_prog;
    uintptr_t decode;
    uintptr_t encode;
} PhonJITSeqInfo;

typedef struct {
    uint64_t field_offset;
    uintptr_t decode;
    uintptr_t encode;
} PhonJITDynamicInfo;

typedef struct {
    uintptr_t ctx;
    uintptr_t decode;
} PhonJITSkipWireInfo;

typedef struct {
    uintptr_t ctx;
    uintptr_t decode;
} PhonJITDefaultInfo;

typedef struct {
    uint64_t field_offset;
    uint64_t key_stride;
    uint64_t key_align;
    uint64_t value_stride;
    uint64_t value_align;
    uintptr_t witness_ctx;
    uintptr_t count;
    uintptr_t project_entries;
    uintptr_t destroy_entries;
    uintptr_t init_with_capacity;
    uintptr_t insert;
    uintptr_t key_entry;
    uintptr_t key_prog;
    uintptr_t value_entry;
    uintptr_t value_prog;
    uintptr_t decode;
    uintptr_t encode;
} PhonJITMapInfo;

typedef struct {
    uint64_t field_offset;
    uintptr_t entry;
    uintptr_t prog;
    uint64_t scratch_size;
    uint64_t scratch_align;
    uintptr_t decode;
    uintptr_t encode;
} PhonJITBlockInfo;

bool phon_jit_option_project_some(const void *ctx, const uint8_t *option, uint8_t *scratch);
void phon_jit_option_init_some(const void *ctx, uint8_t *option, uint8_t *scratch);
void phon_jit_option_init_none(const void *ctx, uint8_t *option);
uint64_t phon_jit_bytes_count(const void *ctx, const uint8_t *field);
void phon_jit_bytes_copy_into(const void *ctx, const uint8_t *field, uint8_t *dst);
bool phon_jit_bytes_construct(const void *ctx, uint8_t *field, const uint8_t *src, uint64_t count);
uint32_t phon_jit_enum_tag(const void *ctx, const uint8_t *field);
void phon_jit_enum_project(const void *ctx, const uint8_t *field, uint32_t local_index, uint8_t *scratch);
void phon_jit_enum_destroy(const void *ctx, uint8_t *scratch, uint32_t local_index);
void phon_jit_enum_inject(const void *ctx, uint8_t *field, uint32_t local_index, uint8_t *scratch);
uint64_t phon_jit_seq_count(const void *ctx, const uint8_t *field);
void phon_jit_seq_copy_elements(const void *ctx, const uint8_t *field, uint8_t *dst);
void phon_jit_seq_destroy_elements(const void *ctx, uint8_t *elements, uint64_t count);
void phon_jit_seq_construct(const void *ctx, uint8_t *field, uint8_t *src, uint64_t count);
void phon_jit_dynamic_decode(PhonJITDecodeCtx *ctx, const PhonJITDynamicInfo *info);
void phon_jit_dynamic_encode(PhonJITEncodeCtx *ctx, const PhonJITDynamicInfo *info);
void phon_jit_skipwire_decode(PhonJITDecodeCtx *ctx, const PhonJITSkipWireInfo *info);
void phon_jit_default_decode(PhonJITDecodeCtx *ctx, const PhonJITDefaultInfo *info);
uint64_t phon_jit_map_count(const void *ctx, const uint8_t *field);
void phon_jit_map_project_entries(const void *ctx, const uint8_t *field, uint8_t *keys, uint8_t *values);
void phon_jit_map_destroy_entries(const void *ctx, uint8_t *keys, uint8_t *values, uint64_t count);
void phon_jit_map_init_with_capacity(const void *ctx, uint8_t *field, uint64_t capacity);
void phon_jit_map_insert(const void *ctx, uint8_t *field, uint8_t *key, uint8_t *value);

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
uintptr_t phon_jit_bytes_decode_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_bytes_encode_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_bytes_count_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_bytes_copy_into_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_bytes_construct_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_enum_decode_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_enum_encode_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_enum_tag_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_enum_project_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_enum_destroy_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_enum_inject_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_seq_decode_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_seq_encode_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_seq_count_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_seq_copy_elements_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_seq_destroy_elements_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_seq_construct_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_dynamic_decode_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_dynamic_encode_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_skipwire_decode_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_default_decode_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_map_decode_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_map_encode_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_map_count_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_map_project_entries_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_map_destroy_entries_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_map_init_with_capacity_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_map_insert_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_block_decode_ptr(void);
// r[impl ir.stencils]
uintptr_t phon_jit_block_encode_ptr(void);

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
const uint8_t *phon_jit_bytes_decode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_bytes_decode_len(void);
// r[impl ir.stencils]
size_t phon_jit_bytes_decode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_bytes_encode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_bytes_encode_len(void);
// r[impl ir.stencils]
size_t phon_jit_bytes_encode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_dynamic_decode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_dynamic_decode_len(void);
// r[impl ir.stencils]
size_t phon_jit_dynamic_decode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_dynamic_encode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_dynamic_encode_len(void);
// r[impl ir.stencils]
size_t phon_jit_dynamic_encode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_enum_decode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_enum_decode_len(void);
// r[impl ir.stencils]
size_t phon_jit_enum_decode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_enum_encode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_enum_encode_len(void);
// r[impl ir.stencils]
size_t phon_jit_enum_encode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_seq_decode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_seq_decode_len(void);
// r[impl ir.stencils]
size_t phon_jit_seq_decode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_seq_encode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_seq_encode_len(void);
// r[impl ir.stencils]
size_t phon_jit_seq_encode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_map_decode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_map_decode_len(void);
// r[impl ir.stencils]
size_t phon_jit_map_decode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_map_encode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_map_encode_len(void);
// r[impl ir.stencils]
size_t phon_jit_map_encode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_block_decode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_block_decode_len(void);
// r[impl ir.stencils]
size_t phon_jit_block_decode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_block_encode_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_block_encode_len(void);
// r[impl ir.stencils]
size_t phon_jit_block_encode_branch_offset(void);

// r[impl ir.stencils]
const uint8_t *phon_jit_done_bytes(void);
// r[impl ir.stencils]
size_t phon_jit_done_len(void);

#endif

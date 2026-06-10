#include "CPhonJITStencils.h"

#include <libkern/OSCacheControl.h>
#include <stdlib.h>

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
extern const uint8_t phon_jit_bytes_decode_start[];
extern const uint8_t phon_jit_bytes_decode_next[];
extern const uint8_t phon_jit_bytes_decode_end[];
extern const uint8_t phon_jit_bytes_encode_start[];
extern const uint8_t phon_jit_bytes_encode_next[];
extern const uint8_t phon_jit_bytes_encode_end[];
extern const uint8_t phon_jit_dynamic_decode_start[];
extern const uint8_t phon_jit_dynamic_decode_next[];
extern const uint8_t phon_jit_dynamic_decode_end[];
extern const uint8_t phon_jit_dynamic_encode_start[];
extern const uint8_t phon_jit_dynamic_encode_next[];
extern const uint8_t phon_jit_dynamic_encode_end[];
extern const uint8_t phon_jit_enum_decode_start[];
extern const uint8_t phon_jit_enum_decode_next[];
extern const uint8_t phon_jit_enum_decode_end[];
extern const uint8_t phon_jit_enum_encode_start[];
extern const uint8_t phon_jit_enum_encode_next[];
extern const uint8_t phon_jit_enum_encode_end[];
extern const uint8_t phon_jit_seq_decode_start[];
extern const uint8_t phon_jit_seq_decode_next[];
extern const uint8_t phon_jit_seq_decode_end[];
extern const uint8_t phon_jit_seq_encode_start[];
extern const uint8_t phon_jit_seq_encode_next[];
extern const uint8_t phon_jit_seq_encode_end[];
extern const uint8_t phon_jit_map_decode_start[];
extern const uint8_t phon_jit_map_decode_next[];
extern const uint8_t phon_jit_map_decode_end[];
extern const uint8_t phon_jit_map_encode_start[];
extern const uint8_t phon_jit_map_encode_next[];
extern const uint8_t phon_jit_map_encode_end[];
extern const uint8_t phon_jit_block_decode_start[];
extern const uint8_t phon_jit_block_decode_next[];
extern const uint8_t phon_jit_block_decode_end[];
extern const uint8_t phon_jit_block_encode_start[];
extern const uint8_t phon_jit_block_encode_next[];
extern const uint8_t phon_jit_block_encode_end[];
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

typedef uint64_t (*PhonJITBytesCountFn)(const void *, const uint8_t *);
typedef void (*PhonJITBytesCopyIntoFn)(const void *, const uint8_t *, uint8_t *);
typedef bool (*PhonJITBytesConstructFn)(const void *, uint8_t *, const uint8_t *, uint64_t);
typedef uint32_t (*PhonJITEnumTagFn)(const void *, const uint8_t *);
typedef void (*PhonJITEnumProjectFn)(const void *, const uint8_t *, uint32_t, uint8_t *);
typedef void (*PhonJITEnumDestroyFn)(const void *, uint8_t *, uint32_t);
typedef void (*PhonJITEnumInjectFn)(const void *, uint8_t *, uint32_t, uint8_t *);
typedef void (*PhonJITDecodeEntryFn)(PhonJITDecodeCtx *);
typedef void (*PhonJITEncodeEntryFn)(PhonJITEncodeCtx *);
typedef uint64_t (*PhonJITSeqCountFn)(const void *, const uint8_t *);
typedef void (*PhonJITSeqCopyElementsFn)(const void *, const uint8_t *, uint8_t *);
typedef void (*PhonJITSeqDestroyElementsFn)(const void *, uint8_t *, uint64_t);
typedef void (*PhonJITSeqConstructFn)(const void *, uint8_t *, uint8_t *, uint64_t);
typedef uint64_t (*PhonJITMapCountFn)(const void *, const uint8_t *);
typedef void (*PhonJITMapProjectEntriesFn)(const void *, const uint8_t *, uint8_t *, uint8_t *);
typedef void (*PhonJITMapDestroyEntriesFn)(const void *, uint8_t *, uint8_t *, uint64_t);
typedef void (*PhonJITMapInitWithCapacityFn)(const void *, uint8_t *, uint64_t);
typedef void (*PhonJITMapInsertFn)(const void *, uint8_t *, uint8_t *, uint8_t *);

static uint64_t phon_jit_align_pad(const uint8_t *start, const uint8_t *cursor, uint64_t align) {
    if (align <= 1) {
        return 0;
    }
    uint64_t offset = (uint64_t)(cursor - start);
    uint64_t mask = align - 1;
    return (align - (offset & mask)) & mask;
}

static bool phon_jit_checked_byte_count(uint64_t count, uint64_t stride, uint64_t *out) {
    if (stride != 0 && count > UINT64_MAX / stride) {
        return false;
    }
    *out = count * stride;
    return true;
}

static uint8_t *phon_jit_alloc_temp(uint64_t byte_count, uint64_t align) {
    if (byte_count == 0) {
        byte_count = 1;
    }
    if (align < sizeof(void *)) {
        align = sizeof(void *);
    }
    if ((align & (align - 1)) != 0) {
        return NULL;
    }
    void *ptr = NULL;
    if (posix_memalign(&ptr, (size_t)align, (size_t)byte_count) != 0) {
        return NULL;
    }
    return (uint8_t *)ptr;
}

// r[impl ir.stencils]
static void phon_jit_call_bytes_decode(PhonJITDecodeCtx *ctx, const PhonJITBytesInfo *info) {
    const uint8_t *wire = ctx->wire;
    if ((uint64_t)(ctx->wire_end - wire) < 4) {
        ctx->status = 1;
        return;
    }

    uint32_t count =
        ((uint32_t)wire[0]) |
        ((uint32_t)wire[1] << 8) |
        ((uint32_t)wire[2] << 16) |
        ((uint32_t)wire[3] << 24);
    wire += 4;

    uint8_t *field = ctx->base + info->field_offset;
    PhonJITBytesConstructFn construct = (PhonJITBytesConstructFn)info->construct;

    if (count == 0) {
        uint8_t dummy = 0;
        if (!construct((const void *)info->witness_ctx, field, &dummy, 0)) {
            ctx->status = 3;
        } else {
            ctx->wire = wire;
        }
        return;
    }

    uint64_t pad = phon_jit_align_pad(ctx->wire_start, wire, info->elem_align);
    if ((uint64_t)(ctx->wire_end - wire) < pad) {
        ctx->status = 1;
        return;
    }
    wire += pad;

    uint64_t byte_count = 0;
    if (!phon_jit_checked_byte_count(count, info->stride, &byte_count)) {
        ctx->status = 1;
        return;
    }
    if ((uint64_t)(ctx->wire_end - wire) < byte_count) {
        ctx->status = 1;
        return;
    }

    if (!construct((const void *)info->witness_ctx, field, wire, count)) {
        ctx->status = 3;
        return;
    }
    ctx->wire = wire + byte_count;
}

// r[impl ir.stencils]
static void phon_jit_call_bytes_encode(PhonJITEncodeCtx *ctx, const PhonJITBytesInfo *info) {
    const uint8_t *field = ctx->base + info->field_offset;
    PhonJITBytesCountFn count_fn = (PhonJITBytesCountFn)info->count;
    uint64_t count = count_fn((const void *)info->witness_ctx, field);
    if (count > UINT32_MAX) {
        ctx->status = 2;
        return;
    }

    uint8_t *out = ctx->out;
    if ((uint64_t)(ctx->out_end - out) < 4) {
        ctx->status = 1;
        return;
    }

    uint32_t n = (uint32_t)count;
    out[0] = (uint8_t)(n);
    out[1] = (uint8_t)(n >> 8);
    out[2] = (uint8_t)(n >> 16);
    out[3] = (uint8_t)(n >> 24);
    out += 4;

    if (count == 0) {
        ctx->out = out;
        return;
    }

    uint64_t pad = phon_jit_align_pad(ctx->out_start, out, info->elem_align);
    uint64_t byte_count = 0;
    if (!phon_jit_checked_byte_count(count, info->stride, &byte_count)) {
        ctx->status = 2;
        return;
    }
    if ((uint64_t)(ctx->out_end - out) < pad || (uint64_t)(ctx->out_end - out - pad) < byte_count) {
        ctx->status = 1;
        return;
    }

    for (uint64_t i = 0; i < pad; i++) {
        out[i] = 0;
    }
    out += pad;

    PhonJITBytesCopyIntoFn copy_into = (PhonJITBytesCopyIntoFn)info->copy_into;
    copy_into((const void *)info->witness_ctx, field, out);
    ctx->out = out + byte_count;
}

// r[impl ir.stencils]
static void phon_jit_call_enum_decode(PhonJITDecodeCtx *ctx, const PhonJITEnumInfo *info) {
    const uint8_t *wire = ctx->wire;
    if ((uint64_t)(ctx->wire_end - wire) < 4) {
        ctx->status = 1;
        return;
    }

    uint32_t wire_index =
        ((uint32_t)wire[0]) |
        ((uint32_t)wire[1] << 8) |
        ((uint32_t)wire[2] << 16) |
        ((uint32_t)wire[3] << 24);
    wire += 4;

    const PhonJITEnumVariantInfo *variants = (const PhonJITEnumVariantInfo *)info->variants;
    const PhonJITEnumVariantInfo *variant = NULL;
    for (uint64_t i = 0; i < info->variant_count; i++) {
        if (variants[i].wire_index == wire_index) {
            variant = &variants[i];
            break;
        }
    }
    if (variant == NULL) {
        const uint32_t *writer_only = info->writer_only;
        for (uint64_t i = 0; writer_only != NULL && i < info->writer_only_count; i++) {
            if (writer_only[i] == wire_index) {
                ctx->status = 8;
                ctx->aux = wire_index;
                return;
            }
        }
        ctx->status = 4;
        ctx->aux = wire_index;
        return;
    }

    uint8_t *field = ctx->base + info->field_offset;
    uint8_t *scratch = ctx->scratch + variant->scratch_offset;
    uint8_t *saved_base = ctx->base;
    const uint64_t *saved_prog = ctx->prog;

    ctx->wire = wire;
    ctx->base = scratch;
    ctx->prog = (const uint64_t *)variant->payload_prog;
    PhonJITDecodeEntryFn payload_entry = (PhonJITDecodeEntryFn)variant->payload_entry;
    payload_entry(ctx);
    ctx->base = saved_base;
    ctx->prog = saved_prog;
    if (ctx->status != 0) {
        return;
    }

    PhonJITEnumInjectFn inject = (PhonJITEnumInjectFn)info->inject;
    inject((const void *)info->witness_ctx, field, variant->reader_local_index, scratch);
}

// r[impl ir.stencils]
static void phon_jit_call_enum_encode(PhonJITEncodeCtx *ctx, const PhonJITEnumInfo *info) {
    const uint8_t *field = ctx->base + info->field_offset;
    PhonJITEnumTagFn tag = (PhonJITEnumTagFn)info->tag;
    uint32_t local_index = tag((const void *)info->witness_ctx, field);
    if (local_index == UINT32_MAX) {
        ctx->status = 3;
        return;
    }

    const PhonJITEnumVariantInfo *variants = (const PhonJITEnumVariantInfo *)info->variants;
    const PhonJITEnumVariantInfo *variant = NULL;
    for (uint64_t i = 0; i < info->variant_count; i++) {
        if (variants[i].reader_local_index == local_index) {
            variant = &variants[i];
            break;
        }
    }
    if (variant == NULL) {
        ctx->status = 3;
        return;
    }

    uint8_t *out = ctx->out;
    if ((uint64_t)(ctx->out_end - out) < 4) {
        ctx->status = 1;
        return;
    }

    out[0] = (uint8_t)(variant->wire_index);
    out[1] = (uint8_t)(variant->wire_index >> 8);
    out[2] = (uint8_t)(variant->wire_index >> 16);
    out[3] = (uint8_t)(variant->wire_index >> 24);
    out += 4;
    ctx->out = out;

    uint8_t *scratch = ctx->scratch + variant->scratch_offset;
    PhonJITEnumProjectFn project = (PhonJITEnumProjectFn)info->project;
    project((const void *)info->witness_ctx, field, variant->reader_local_index, scratch);

    const uint8_t *saved_base = ctx->base;
    const uint64_t *saved_prog = ctx->prog;
    ctx->base = scratch;
    ctx->prog = (const uint64_t *)variant->payload_prog;
    PhonJITEncodeEntryFn payload_entry = (PhonJITEncodeEntryFn)variant->payload_entry;
    payload_entry(ctx);
    ctx->base = saved_base;
    ctx->prog = saved_prog;

    PhonJITEnumDestroyFn destroy = (PhonJITEnumDestroyFn)info->destroy;
    destroy((const void *)info->witness_ctx, scratch, variant->reader_local_index);
}

// r[impl ir.stencils]
static void phon_jit_call_seq_decode(PhonJITDecodeCtx *ctx, const PhonJITSeqInfo *info) {
    const uint8_t *wire = ctx->wire;
    if ((uint64_t)(ctx->wire_end - wire) < 4) {
        ctx->status = 1;
        return;
    }

    uint32_t count =
        ((uint32_t)wire[0]) |
        ((uint32_t)wire[1] << 8) |
        ((uint32_t)wire[2] << 16) |
        ((uint32_t)wire[3] << 24);
    wire += 4;

    if (info->min_wire != 0 && (uint64_t)(ctx->wire_end - wire) / info->min_wire < count) {
        ctx->status = 1;
        return;
    }

    uint64_t byte_count = 0;
    if (!phon_jit_checked_byte_count(count, info->stride, &byte_count)) {
        ctx->status = 5;
        return;
    }

    uint8_t *field = ctx->base + info->field_offset;
    uint8_t *buffer = phon_jit_alloc_temp(byte_count, info->elem_align);
    if (buffer == NULL) {
        ctx->status = 5;
        return;
    }

    ctx->wire = wire;
    uint8_t *saved_base = ctx->base;
    const uint64_t *saved_prog = ctx->prog;
    PhonJITDecodeEntryFn element_entry = (PhonJITDecodeEntryFn)info->element_entry;
    for (uint32_t i = 0; i < count; i++) {
        ctx->base = buffer + ((uint64_t)i * info->stride);
        ctx->prog = (const uint64_t *)info->element_prog;
        element_entry(ctx);
        if (ctx->status != 0) {
            ctx->base = saved_base;
            ctx->prog = saved_prog;
            free(buffer);
            return;
        }
    }
    ctx->base = saved_base;
    ctx->prog = saved_prog;

    PhonJITSeqConstructFn construct = (PhonJITSeqConstructFn)info->construct;
    construct((const void *)info->witness_ctx, field, buffer, count);
    if (info->unique != 0) {
        PhonJITSeqCountFn count_fn = (PhonJITSeqCountFn)info->count;
        if (count_fn((const void *)info->witness_ctx, field) != count) {
            ctx->status = 7;
        }
    }
    free(buffer);
}

// r[impl ir.stencils]
static void phon_jit_call_seq_encode(PhonJITEncodeCtx *ctx, const PhonJITSeqInfo *info) {
    const uint8_t *field = ctx->base + info->field_offset;
    PhonJITSeqCountFn count_fn = (PhonJITSeqCountFn)info->count;
    uint64_t count = count_fn((const void *)info->witness_ctx, field);
    if (count > UINT32_MAX) {
        ctx->status = 2;
        return;
    }

    uint8_t *out = ctx->out;
    if ((uint64_t)(ctx->out_end - out) < 4) {
        ctx->status = 1;
        return;
    }
    uint32_t n = (uint32_t)count;
    out[0] = (uint8_t)(n);
    out[1] = (uint8_t)(n >> 8);
    out[2] = (uint8_t)(n >> 16);
    out[3] = (uint8_t)(n >> 24);
    ctx->out = out + 4;

    uint64_t byte_count = 0;
    if (!phon_jit_checked_byte_count(count, info->stride, &byte_count)) {
        ctx->status = 2;
        return;
    }

    uint8_t *buffer = phon_jit_alloc_temp(byte_count, info->elem_align);
    if (buffer == NULL) {
        ctx->status = 2;
        return;
    }

    PhonJITSeqCopyElementsFn copy = (PhonJITSeqCopyElementsFn)info->copy_elements;
    PhonJITSeqDestroyElementsFn destroy = (PhonJITSeqDestroyElementsFn)info->destroy_elements;
    copy((const void *)info->witness_ctx, field, buffer);

    const uint8_t *saved_base = ctx->base;
    const uint64_t *saved_prog = ctx->prog;
    PhonJITEncodeEntryFn element_entry = (PhonJITEncodeEntryFn)info->element_entry;
    for (uint32_t i = 0; i < count; i++) {
        ctx->base = buffer + ((uint64_t)i * info->stride);
        ctx->prog = (const uint64_t *)info->element_prog;
        element_entry(ctx);
        if (ctx->status != 0) {
            ctx->base = saved_base;
            ctx->prog = saved_prog;
            destroy((const void *)info->witness_ctx, buffer, count);
            free(buffer);
            return;
        }
    }
    ctx->base = saved_base;
    ctx->prog = saved_prog;
    destroy((const void *)info->witness_ctx, buffer, count);
    free(buffer);
}

// r[impl ir.stencils]
// r[impl validate.uniqueness]
static void phon_jit_call_map_decode(PhonJITDecodeCtx *ctx, const PhonJITMapInfo *info) {
    const uint8_t *wire = ctx->wire;
    if ((uint64_t)(ctx->wire_end - wire) < 4) {
        ctx->status = 1;
        return;
    }

    uint32_t count =
        ((uint32_t)wire[0]) |
        ((uint32_t)wire[1] << 8) |
        ((uint32_t)wire[2] << 16) |
        ((uint32_t)wire[3] << 24);
    wire += 4;

    if ((uint64_t)(ctx->wire_end - wire) < count) {
        ctx->status = 1;
        return;
    }

    uint8_t *field = ctx->base + info->field_offset;
    PhonJITMapInitWithCapacityFn init = (PhonJITMapInitWithCapacityFn)info->init_with_capacity;
    init((const void *)info->witness_ctx, field, count);

    ctx->wire = wire;
    uint8_t *saved_base = ctx->base;
    const uint64_t *saved_prog = ctx->prog;
    PhonJITDecodeEntryFn key_entry = (PhonJITDecodeEntryFn)info->key_entry;
    PhonJITDecodeEntryFn value_entry = (PhonJITDecodeEntryFn)info->value_entry;
    PhonJITMapInsertFn insert = (PhonJITMapInsertFn)info->insert;

    for (uint32_t i = 0; i < count; i++) {
        uint8_t *key = phon_jit_alloc_temp(info->key_stride, info->key_align);
        uint8_t *value = phon_jit_alloc_temp(info->value_stride, info->value_align);
        if (key == NULL || value == NULL) {
            free(key);
            free(value);
            ctx->base = saved_base;
            ctx->prog = saved_prog;
            ctx->status = 5;
            return;
        }

        ctx->base = key;
        ctx->prog = (const uint64_t *)info->key_prog;
        key_entry(ctx);
        if (ctx->status != 0) {
            ctx->base = saved_base;
            ctx->prog = saved_prog;
            free(key);
            free(value);
            return;
        }

        ctx->base = value;
        ctx->prog = (const uint64_t *)info->value_prog;
        value_entry(ctx);
        if (ctx->status != 0) {
            ctx->base = saved_base;
            ctx->prog = saved_prog;
            free(key);
            free(value);
            return;
        }

        insert((const void *)info->witness_ctx, field, key, value);
        free(key);
        free(value);
    }
    ctx->base = saved_base;
    ctx->prog = saved_prog;

    PhonJITMapCountFn count_fn = (PhonJITMapCountFn)info->count;
    if (count_fn((const void *)info->witness_ctx, field) != count) {
        ctx->status = 6;
    }
}

// r[impl ir.stencils]
static void phon_jit_call_map_encode(PhonJITEncodeCtx *ctx, const PhonJITMapInfo *info) {
    const uint8_t *field = ctx->base + info->field_offset;
    PhonJITMapCountFn count_fn = (PhonJITMapCountFn)info->count;
    uint64_t count = count_fn((const void *)info->witness_ctx, field);
    if (count > UINT32_MAX) {
        ctx->status = 2;
        return;
    }

    uint8_t *out = ctx->out;
    if ((uint64_t)(ctx->out_end - out) < 4) {
        ctx->status = 1;
        return;
    }
    uint32_t n = (uint32_t)count;
    out[0] = (uint8_t)(n);
    out[1] = (uint8_t)(n >> 8);
    out[2] = (uint8_t)(n >> 16);
    out[3] = (uint8_t)(n >> 24);
    ctx->out = out + 4;

    if (count == 0) {
        return;
    }

    uint64_t key_bytes = 0;
    uint64_t value_bytes = 0;
    if (!phon_jit_checked_byte_count(count, info->key_stride, &key_bytes)
        || !phon_jit_checked_byte_count(count, info->value_stride, &value_bytes)) {
        ctx->status = 2;
        return;
    }

    uint8_t *keys = phon_jit_alloc_temp(key_bytes, info->key_align);
    uint8_t *values = phon_jit_alloc_temp(value_bytes, info->value_align);
    if (keys == NULL || values == NULL) {
        free(keys);
        free(values);
        ctx->status = 2;
        return;
    }

    PhonJITMapProjectEntriesFn project = (PhonJITMapProjectEntriesFn)info->project_entries;
    project((const void *)info->witness_ctx, field, keys, values);

    const uint8_t *saved_base = ctx->base;
    const uint64_t *saved_prog = ctx->prog;
    PhonJITEncodeEntryFn key_entry = (PhonJITEncodeEntryFn)info->key_entry;
    PhonJITEncodeEntryFn value_entry = (PhonJITEncodeEntryFn)info->value_entry;
    for (uint32_t i = 0; i < count; i++) {
        ctx->base = keys + ((uint64_t)i * info->key_stride);
        ctx->prog = (const uint64_t *)info->key_prog;
        key_entry(ctx);
        if (ctx->status != 0) {
            break;
        }

        ctx->base = values + ((uint64_t)i * info->value_stride);
        ctx->prog = (const uint64_t *)info->value_prog;
        value_entry(ctx);
        if (ctx->status != 0) {
            break;
        }
    }
    ctx->base = saved_base;
    ctx->prog = saved_prog;

    PhonJITMapDestroyEntriesFn destroy = (PhonJITMapDestroyEntriesFn)info->destroy_entries;
    destroy((const void *)info->witness_ctx, keys, values, count);
    free(keys);
    free(values);
}

// r[impl ir.stencils]
static void phon_jit_call_block_decode(PhonJITDecodeCtx *ctx, const PhonJITBlockInfo *info) {
    uint8_t *scratch = phon_jit_alloc_temp(info->scratch_size, info->scratch_align);
    if (scratch == NULL) {
        ctx->status = 5;
        return;
    }
    uint8_t *saved_base = ctx->base;
    const uint64_t *saved_prog = ctx->prog;
    uint8_t *saved_scratch = ctx->scratch;
    ctx->base = saved_base + info->field_offset;
    ctx->prog = (const uint64_t *)info->prog;
    ctx->scratch = scratch;
    PhonJITDecodeEntryFn entry = (PhonJITDecodeEntryFn)info->entry;
    entry(ctx);
    ctx->base = saved_base;
    ctx->prog = saved_prog;
    ctx->scratch = saved_scratch;
    free(scratch);
}

// r[impl ir.stencils]
static void phon_jit_call_block_encode(PhonJITEncodeCtx *ctx, const PhonJITBlockInfo *info) {
    uint8_t *scratch = phon_jit_alloc_temp(info->scratch_size, info->scratch_align);
    if (scratch == NULL) {
        ctx->status = 2;
        return;
    }
    const uint8_t *saved_base = ctx->base;
    const uint64_t *saved_prog = ctx->prog;
    uint8_t *saved_scratch = ctx->scratch;
    ctx->base = saved_base + info->field_offset;
    ctx->prog = (const uint64_t *)info->prog;
    ctx->scratch = scratch;
    PhonJITEncodeEntryFn entry = (PhonJITEncodeEntryFn)info->entry;
    entry(ctx);
    ctx->base = saved_base;
    ctx->prog = saved_prog;
    ctx->scratch = saved_scratch;
    free(scratch);
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
uintptr_t phon_jit_bytes_decode_ptr(void) {
    return (uintptr_t)&phon_jit_call_bytes_decode;
}

// r[impl ir.stencils]
uintptr_t phon_jit_bytes_encode_ptr(void) {
    return (uintptr_t)&phon_jit_call_bytes_encode;
}

// r[impl ir.stencils]
uintptr_t phon_jit_bytes_count_ptr(void) {
    return (uintptr_t)&phon_jit_bytes_count;
}

// r[impl ir.stencils]
uintptr_t phon_jit_bytes_copy_into_ptr(void) {
    return (uintptr_t)&phon_jit_bytes_copy_into;
}

// r[impl ir.stencils]
uintptr_t phon_jit_bytes_construct_ptr(void) {
    return (uintptr_t)&phon_jit_bytes_construct;
}

// r[impl ir.stencils]
uintptr_t phon_jit_enum_decode_ptr(void) {
    return (uintptr_t)&phon_jit_call_enum_decode;
}

// r[impl ir.stencils]
uintptr_t phon_jit_enum_encode_ptr(void) {
    return (uintptr_t)&phon_jit_call_enum_encode;
}

// r[impl ir.stencils]
uintptr_t phon_jit_enum_tag_ptr(void) {
    return (uintptr_t)&phon_jit_enum_tag;
}

// r[impl ir.stencils]
uintptr_t phon_jit_enum_project_ptr(void) {
    return (uintptr_t)&phon_jit_enum_project;
}

// r[impl ir.stencils]
uintptr_t phon_jit_enum_destroy_ptr(void) {
    return (uintptr_t)&phon_jit_enum_destroy;
}

// r[impl ir.stencils]
uintptr_t phon_jit_enum_inject_ptr(void) {
    return (uintptr_t)&phon_jit_enum_inject;
}

// r[impl ir.stencils]
uintptr_t phon_jit_seq_decode_ptr(void) {
    return (uintptr_t)&phon_jit_call_seq_decode;
}

// r[impl ir.stencils]
uintptr_t phon_jit_seq_encode_ptr(void) {
    return (uintptr_t)&phon_jit_call_seq_encode;
}

// r[impl ir.stencils]
uintptr_t phon_jit_seq_count_ptr(void) {
    return (uintptr_t)&phon_jit_seq_count;
}

// r[impl ir.stencils]
uintptr_t phon_jit_seq_copy_elements_ptr(void) {
    return (uintptr_t)&phon_jit_seq_copy_elements;
}

// r[impl ir.stencils]
uintptr_t phon_jit_seq_destroy_elements_ptr(void) {
    return (uintptr_t)&phon_jit_seq_destroy_elements;
}

// r[impl ir.stencils]
uintptr_t phon_jit_seq_construct_ptr(void) {
    return (uintptr_t)&phon_jit_seq_construct;
}

// r[impl ir.stencils]
uintptr_t phon_jit_dynamic_decode_ptr(void) {
    return (uintptr_t)&phon_jit_dynamic_decode;
}

// r[impl ir.stencils]
uintptr_t phon_jit_dynamic_encode_ptr(void) {
    return (uintptr_t)&phon_jit_dynamic_encode;
}

// r[impl ir.stencils]
uintptr_t phon_jit_skipwire_decode_ptr(void) {
    return (uintptr_t)&phon_jit_skipwire_decode;
}

// r[impl ir.stencils]
uintptr_t phon_jit_default_decode_ptr(void) {
    return (uintptr_t)&phon_jit_default_decode;
}

// r[impl ir.stencils]
uintptr_t phon_jit_map_decode_ptr(void) {
    return (uintptr_t)&phon_jit_call_map_decode;
}

// r[impl ir.stencils]
uintptr_t phon_jit_map_encode_ptr(void) {
    return (uintptr_t)&phon_jit_call_map_encode;
}

// r[impl ir.stencils]
uintptr_t phon_jit_map_count_ptr(void) {
    return (uintptr_t)&phon_jit_map_count;
}

// r[impl ir.stencils]
uintptr_t phon_jit_map_project_entries_ptr(void) {
    return (uintptr_t)&phon_jit_map_project_entries;
}

// r[impl ir.stencils]
uintptr_t phon_jit_map_destroy_entries_ptr(void) {
    return (uintptr_t)&phon_jit_map_destroy_entries;
}

// r[impl ir.stencils]
uintptr_t phon_jit_map_init_with_capacity_ptr(void) {
    return (uintptr_t)&phon_jit_map_init_with_capacity;
}

// r[impl ir.stencils]
uintptr_t phon_jit_map_insert_ptr(void) {
    return (uintptr_t)&phon_jit_map_insert;
}

// r[impl ir.stencils]
uintptr_t phon_jit_block_decode_ptr(void) {
    return (uintptr_t)&phon_jit_call_block_decode;
}

// r[impl ir.stencils]
uintptr_t phon_jit_block_encode_ptr(void) {
    return (uintptr_t)&phon_jit_call_block_encode;
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
const uint8_t *phon_jit_bytes_decode_bytes(void) {
    return phon_jit_bytes_decode_start;
}

// r[impl ir.stencils]
size_t phon_jit_bytes_decode_len(void) {
    return (size_t)(phon_jit_bytes_decode_end - phon_jit_bytes_decode_start);
}

// r[impl ir.stencils]
size_t phon_jit_bytes_decode_branch_offset(void) {
    return (size_t)(phon_jit_bytes_decode_next - phon_jit_bytes_decode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_bytes_encode_bytes(void) {
    return phon_jit_bytes_encode_start;
}

// r[impl ir.stencils]
size_t phon_jit_bytes_encode_len(void) {
    return (size_t)(phon_jit_bytes_encode_end - phon_jit_bytes_encode_start);
}

// r[impl ir.stencils]
size_t phon_jit_bytes_encode_branch_offset(void) {
    return (size_t)(phon_jit_bytes_encode_next - phon_jit_bytes_encode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_dynamic_decode_bytes(void) {
    return phon_jit_dynamic_decode_start;
}

// r[impl ir.stencils]
size_t phon_jit_dynamic_decode_len(void) {
    return (size_t)(phon_jit_dynamic_decode_end - phon_jit_dynamic_decode_start);
}

// r[impl ir.stencils]
size_t phon_jit_dynamic_decode_branch_offset(void) {
    return (size_t)(phon_jit_dynamic_decode_next - phon_jit_dynamic_decode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_dynamic_encode_bytes(void) {
    return phon_jit_dynamic_encode_start;
}

// r[impl ir.stencils]
size_t phon_jit_dynamic_encode_len(void) {
    return (size_t)(phon_jit_dynamic_encode_end - phon_jit_dynamic_encode_start);
}

// r[impl ir.stencils]
size_t phon_jit_dynamic_encode_branch_offset(void) {
    return (size_t)(phon_jit_dynamic_encode_next - phon_jit_dynamic_encode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_enum_decode_bytes(void) {
    return phon_jit_enum_decode_start;
}

// r[impl ir.stencils]
size_t phon_jit_enum_decode_len(void) {
    return (size_t)(phon_jit_enum_decode_end - phon_jit_enum_decode_start);
}

// r[impl ir.stencils]
size_t phon_jit_enum_decode_branch_offset(void) {
    return (size_t)(phon_jit_enum_decode_next - phon_jit_enum_decode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_enum_encode_bytes(void) {
    return phon_jit_enum_encode_start;
}

// r[impl ir.stencils]
size_t phon_jit_enum_encode_len(void) {
    return (size_t)(phon_jit_enum_encode_end - phon_jit_enum_encode_start);
}

// r[impl ir.stencils]
size_t phon_jit_enum_encode_branch_offset(void) {
    return (size_t)(phon_jit_enum_encode_next - phon_jit_enum_encode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_seq_decode_bytes(void) {
    return phon_jit_seq_decode_start;
}

// r[impl ir.stencils]
size_t phon_jit_seq_decode_len(void) {
    return (size_t)(phon_jit_seq_decode_end - phon_jit_seq_decode_start);
}

// r[impl ir.stencils]
size_t phon_jit_seq_decode_branch_offset(void) {
    return (size_t)(phon_jit_seq_decode_next - phon_jit_seq_decode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_seq_encode_bytes(void) {
    return phon_jit_seq_encode_start;
}

// r[impl ir.stencils]
size_t phon_jit_seq_encode_len(void) {
    return (size_t)(phon_jit_seq_encode_end - phon_jit_seq_encode_start);
}

// r[impl ir.stencils]
size_t phon_jit_seq_encode_branch_offset(void) {
    return (size_t)(phon_jit_seq_encode_next - phon_jit_seq_encode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_map_decode_bytes(void) {
    return phon_jit_map_decode_start;
}

// r[impl ir.stencils]
size_t phon_jit_map_decode_len(void) {
    return (size_t)(phon_jit_map_decode_end - phon_jit_map_decode_start);
}

// r[impl ir.stencils]
size_t phon_jit_map_decode_branch_offset(void) {
    return (size_t)(phon_jit_map_decode_next - phon_jit_map_decode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_map_encode_bytes(void) {
    return phon_jit_map_encode_start;
}

// r[impl ir.stencils]
size_t phon_jit_map_encode_len(void) {
    return (size_t)(phon_jit_map_encode_end - phon_jit_map_encode_start);
}

// r[impl ir.stencils]
size_t phon_jit_map_encode_branch_offset(void) {
    return (size_t)(phon_jit_map_encode_next - phon_jit_map_encode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_block_decode_bytes(void) {
    return phon_jit_block_decode_start;
}

// r[impl ir.stencils]
size_t phon_jit_block_decode_len(void) {
    return (size_t)(phon_jit_block_decode_end - phon_jit_block_decode_start);
}

// r[impl ir.stencils]
size_t phon_jit_block_decode_branch_offset(void) {
    return (size_t)(phon_jit_block_decode_next - phon_jit_block_decode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_block_encode_bytes(void) {
    return phon_jit_block_encode_start;
}

// r[impl ir.stencils]
size_t phon_jit_block_encode_len(void) {
    return (size_t)(phon_jit_block_encode_end - phon_jit_block_encode_start);
}

// r[impl ir.stencils]
size_t phon_jit_block_encode_branch_offset(void) {
    return (size_t)(phon_jit_block_encode_next - phon_jit_block_encode_start);
}

// r[impl ir.stencils]
const uint8_t *phon_jit_done_bytes(void) {
    return phon_jit_done_start;
}

// r[impl ir.stencils]
size_t phon_jit_done_len(void) {
    return (size_t)(phon_jit_done_end - phon_jit_done_start);
}

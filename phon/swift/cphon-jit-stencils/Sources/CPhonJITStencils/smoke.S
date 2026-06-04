.text
.align 2

.globl _phon_jit_smoke_start
_phon_jit_smoke_start:
    add x0, x0, x0, lsl #1
    add x0, x0, #1
    ret

.globl _phon_jit_smoke_end
_phon_jit_smoke_end:

.globl _phon_jit_scalar_decode_start
_phon_jit_scalar_decode_start:
    ldr x1, [x0, #32]
    ldp x2, x3, [x1], #16
    ldr x4, [x1], #8
    str x1, [x0, #32]

    ldr x5, [x0, #0]
    ldr x6, [x0, #8]
    sub x7, x5, x6
    sub x8, x4, #1
    and x7, x7, x8
    sub x7, x4, x7
    and x7, x7, x8
    add x9, x5, x7
    add x10, x9, x3
    ldr x11, [x0, #16]
    cmp x10, x11
    b.hi 2f

    ldr x12, [x0, #24]
    add x13, x12, x2
    mov x14, #0
1:
    cmp x14, x3
    b.hs 3f
    ldrb w15, [x9, x14]
    strb w15, [x13, x14]
    add x14, x14, #1
    b 1b
2:
    mov x15, #1
    str x15, [x0, #40]
    ret
3:
    str x10, [x0, #0]
.globl _phon_jit_scalar_decode_next
_phon_jit_scalar_decode_next:
    b _phon_jit_scalar_decode_next
.globl _phon_jit_scalar_decode_end
_phon_jit_scalar_decode_end:

.globl _phon_jit_scalar_encode_start
_phon_jit_scalar_encode_start:
    ldr x1, [x0, #8]
    ldp x2, x3, [x1], #16
    ldr x4, [x1], #8
    str x1, [x0, #8]

    ldr x5, [x0, #16]
    ldr x6, [x0, #24]
    sub x7, x5, x6
    sub x8, x4, #1
    and x7, x7, x8
    sub x7, x4, x7
    and x7, x7, x8
    add x9, x5, x7
    add x10, x9, x3
    ldr x11, [x0, #32]
    cmp x10, x11
    b.hi 4f

    mov x14, #0
1:
    cmp x14, x7
    b.hs 2f
    strb wzr, [x5, x14]
    add x14, x14, #1
    b 1b
2:
    ldr x12, [x0, #0]
    add x13, x12, x2
    mov x14, #0
3:
    cmp x14, x3
    b.hs 5f
    ldrb w15, [x13, x14]
    strb w15, [x9, x14]
    add x14, x14, #1
    b 3b
4:
    mov x15, #1
    str x15, [x0, #40]
    ret
5:
    str x10, [x0, #16]
.globl _phon_jit_scalar_encode_next
_phon_jit_scalar_encode_next:
    b _phon_jit_scalar_encode_next
.globl _phon_jit_scalar_encode_end
_phon_jit_scalar_encode_end:

.globl _phon_jit_done_start
_phon_jit_done_start:
    ret
.globl _phon_jit_done_end
_phon_jit_done_end:

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

.globl _phon_jit_option_decode_start
_phon_jit_option_decode_start:
    stp x29, x30, [sp, #-16]!
    stp x19, x20, [sp, #-16]!
    stp x21, x22, [sp, #-16]!
    stp x23, x24, [sp, #-16]!

    mov x19, x0
    ldr x1, [x19, #32]
    ldr x20, [x1], #8
    mov x21, x1
    ldr x22, [x19, #24]

    ldr x3, [x19, #0]
    ldr x4, [x19, #16]
    add x5, x3, #1
    cmp x5, x4
    b.hi 9f
    ldrb w6, [x3]
    str x5, [x19, #0]

    ldr x7, [x20, #0]
    add x23, x22, x7
    cbz x6, 1f
    cmp x6, #1
    b.eq 2f
    mov x7, #2
    str x7, [x19, #40]
    str x6, [x19, #48]
    b 10f

1:
    ldr x0, [x20, #32]
    mov x1, x23
    ldr x12, [x20, #56]
    blr x12
    b 8f

2:
    ldr x24, [x19, #56]
    ldr x7, [x20, #8]
    add x24, x24, x7
    ldr x7, [x20, #24]
    str x7, [x19, #32]
    str x24, [x19, #24]
    ldr x12, [x20, #16]
    mov x0, x19
    blr x12
    ldr x7, [x19, #40]
    cbnz x7, 7f
    str x22, [x19, #24]
    ldr x0, [x20, #32]
    mov x1, x23
    mov x2, x24
    ldr x12, [x20, #48]
    blr x12
    b 8f

7:
    str x22, [x19, #24]
    str x21, [x19, #32]
    b 10f

8:
    str x22, [x19, #24]
    str x21, [x19, #32]
    ldp x23, x24, [sp], #16
    ldp x21, x22, [sp], #16
    ldp x19, x20, [sp], #16
    ldp x29, x30, [sp], #16
.globl _phon_jit_option_decode_next
_phon_jit_option_decode_next:
    b _phon_jit_option_decode_next

9:
    mov x7, #1
    str x7, [x19, #40]
10:
    ldp x23, x24, [sp], #16
    ldp x21, x22, [sp], #16
    ldp x19, x20, [sp], #16
    ldp x29, x30, [sp], #16
    ret
.globl _phon_jit_option_decode_end
_phon_jit_option_decode_end:

.globl _phon_jit_option_encode_start
_phon_jit_option_encode_start:
    stp x29, x30, [sp, #-16]!
    stp x19, x20, [sp, #-16]!
    stp x21, x22, [sp, #-16]!
    stp x23, x24, [sp, #-16]!

    mov x19, x0
    ldr x1, [x19, #8]
    ldr x20, [x1], #8
    mov x21, x1
    ldr x22, [x19, #0]

    ldr x7, [x20, #0]
    add x23, x22, x7
    ldr x24, [x19, #48]
    ldr x7, [x20, #8]
    add x24, x24, x7

    ldr x0, [x20, #32]
    mov x1, x23
    mov x2, x24
    ldr x12, [x20, #40]
    blr x12
    and x14, x0, #1

    ldr x5, [x19, #16]
    ldr x6, [x19, #32]
    add x7, x5, #1
    cmp x7, x6
    b.hi 9f
    strb w14, [x5]
    str x7, [x19, #16]

    cbz x14, 8f
    ldr x7, [x20, #24]
    str x7, [x19, #8]
    str x24, [x19, #0]
    ldr x12, [x20, #16]
    mov x0, x19
    blr x12
    ldr x7, [x19, #40]
    cbnz x7, 10f

8:
    str x22, [x19, #0]
    str x21, [x19, #8]
    ldp x23, x24, [sp], #16
    ldp x21, x22, [sp], #16
    ldp x19, x20, [sp], #16
    ldp x29, x30, [sp], #16
.globl _phon_jit_option_encode_next
_phon_jit_option_encode_next:
    b _phon_jit_option_encode_next

9:
    mov x7, #1
    str x7, [x19, #40]
10:
    str x22, [x19, #0]
    str x21, [x19, #8]
    ldp x23, x24, [sp], #16
    ldp x21, x22, [sp], #16
    ldp x19, x20, [sp], #16
    ldp x29, x30, [sp], #16
    ret
.globl _phon_jit_option_encode_end
_phon_jit_option_encode_end:

.globl _phon_jit_done_start
_phon_jit_done_start:
    ret
.globl _phon_jit_done_end
_phon_jit_done_end:

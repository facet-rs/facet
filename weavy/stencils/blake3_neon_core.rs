#![cfg(target_arch = "aarch64")]
#![allow(unsafe_op_in_unsafe_fn)]

use core::arch::aarch64::{
    uint32x4_t, vaddq_u32, vdupq_n_u32, veorq_u32, vextq_u32, vorrq_u32, vsetq_lane_u32,
    vshlq_n_u32, vshrq_n_u32,
};

pub const BLOCK_LEN: usize = 64;
pub const OUT_LEN: usize = 32;

const CHUNK_START: u32 = 1 << 0;
const CHUNK_END: u32 = 1 << 1;
const PARENT: u32 = 1 << 2;
const ROOT: u32 = 1 << 3;

#[inline(always)]
unsafe fn read_u32_le(ptr: *const u8) -> u32 {
    (ptr as *const u32).read_unaligned()
}

#[inline(always)]
unsafe fn write_u32_le(ptr: *mut u8, value: u32) {
    (ptr as *mut u32).write_unaligned(value);
}

#[inline(always)]
unsafe fn set4(a: u32, b: u32, c: u32, d: u32) -> uint32x4_t {
    let v = vdupq_n_u32(a);
    let v = vsetq_lane_u32::<1>(b, v);
    let v = vsetq_lane_u32::<2>(c, v);
    vsetq_lane_u32::<3>(d, v)
}

#[inline(always)]
unsafe fn store_cv(ptr: *mut u8, cv: &[u32; 8]) {
    write_u32_le(ptr, cv[0]);
    write_u32_le(ptr.add(4), cv[1]);
    write_u32_le(ptr.add(8), cv[2]);
    write_u32_le(ptr.add(12), cv[3]);
    write_u32_le(ptr.add(16), cv[4]);
    write_u32_le(ptr.add(20), cv[5]);
    write_u32_le(ptr.add(24), cv[6]);
    write_u32_le(ptr.add(28), cv[7]);
}

#[inline(always)]
unsafe fn fill_iv(cv: &mut [u32; 8]) {
    let ptr = cv.as_mut_ptr();
    ptr.add(0).write_volatile(0x6A09_E667);
    ptr.add(1).write_volatile(0xBB67_AE85);
    ptr.add(2).write_volatile(0x3C6E_F372);
    ptr.add(3).write_volatile(0xA54F_F53A);
    ptr.add(4).write_volatile(0x510E_527F);
    ptr.add(5).write_volatile(0x9B05_688C);
    ptr.add(6).write_volatile(0x1F83_D9AB);
    ptr.add(7).write_volatile(0x5BE0_CD19);
}

#[inline(always)]
unsafe fn set4_from_ptr(ptr: *const u32) -> uint32x4_t {
    set4(
        ptr.add(0).read_volatile(),
        ptr.add(1).read_volatile(),
        ptr.add(2).read_volatile(),
        ptr.add(3).read_volatile(),
    )
}

#[inline(always)]
unsafe fn set4_volatile(a: u32, b: u32, c: u32, d: u32) -> uint32x4_t {
    let mut words = [0u32; 4];
    let ptr = words.as_mut_ptr();
    ptr.add(0).write_volatile(a);
    ptr.add(1).write_volatile(b);
    ptr.add(2).write_volatile(c);
    ptr.add(3).write_volatile(d);
    set4_from_ptr(words.as_ptr())
}

#[inline(always)]
unsafe fn iv_low_vec() -> uint32x4_t {
    let mut words = [0u32; 8];
    fill_iv(&mut words);
    set4_from_ptr(words.as_ptr())
}

#[inline(always)]
unsafe fn load_msg(block: *const u8) -> [u32; 16] {
    [
        read_u32_le(block),
        read_u32_le(block.add(4)),
        read_u32_le(block.add(8)),
        read_u32_le(block.add(12)),
        read_u32_le(block.add(16)),
        read_u32_le(block.add(20)),
        read_u32_le(block.add(24)),
        read_u32_le(block.add(28)),
        read_u32_le(block.add(32)),
        read_u32_le(block.add(36)),
        read_u32_le(block.add(40)),
        read_u32_le(block.add(44)),
        read_u32_le(block.add(48)),
        read_u32_le(block.add(52)),
        read_u32_le(block.add(56)),
        read_u32_le(block.add(60)),
    ]
}

#[inline(always)]
unsafe fn rot16(x: uint32x4_t) -> uint32x4_t {
    vorrq_u32(vshrq_n_u32::<16>(x), vshlq_n_u32::<16>(x))
}

#[inline(always)]
unsafe fn rot12(x: uint32x4_t) -> uint32x4_t {
    vorrq_u32(vshrq_n_u32::<12>(x), vshlq_n_u32::<20>(x))
}

#[inline(always)]
unsafe fn rot8(x: uint32x4_t) -> uint32x4_t {
    vorrq_u32(vshrq_n_u32::<8>(x), vshlq_n_u32::<24>(x))
}

#[inline(always)]
unsafe fn rot7(x: uint32x4_t) -> uint32x4_t {
    vorrq_u32(vshrq_n_u32::<7>(x), vshlq_n_u32::<25>(x))
}

#[inline(always)]
unsafe fn g4(
    a: &mut uint32x4_t,
    b: &mut uint32x4_t,
    c: &mut uint32x4_t,
    d: &mut uint32x4_t,
    x: uint32x4_t,
    y: uint32x4_t,
) {
    *a = vaddq_u32(vaddq_u32(*a, *b), x);
    *d = rot16(veorq_u32(*d, *a));
    *c = vaddq_u32(*c, *d);
    *b = rot12(veorq_u32(*b, *c));
    *a = vaddq_u32(vaddq_u32(*a, *b), y);
    *d = rot8(veorq_u32(*d, *a));
    *c = vaddq_u32(*c, *d);
    *b = rot7(veorq_u32(*b, *c));
}

macro_rules! round {
    ($a:ident, $b:ident, $c:ident, $d:ident, $msg:ident, [$a0:literal, $a1:literal, $a2:literal, $a3:literal, $a4:literal, $a5:literal, $a6:literal, $a7:literal, $a8:literal, $a9:literal, $a10:literal, $a11:literal, $a12:literal, $a13:literal, $a14:literal, $a15:literal]) => {{
        g4(
            &mut $a,
            &mut $b,
            &mut $c,
            &mut $d,
            set4($msg[$a0], $msg[$a2], $msg[$a4], $msg[$a6]),
            set4($msg[$a1], $msg[$a3], $msg[$a5], $msg[$a7]),
        );

        $b = vextq_u32::<1>($b, $b);
        $c = vextq_u32::<2>($c, $c);
        $d = vextq_u32::<3>($d, $d);
        g4(
            &mut $a,
            &mut $b,
            &mut $c,
            &mut $d,
            set4($msg[$a8], $msg[$a10], $msg[$a12], $msg[$a14]),
            set4($msg[$a9], $msg[$a11], $msg[$a13], $msg[$a15]),
        );
        $b = vextq_u32::<3>($b, $b);
        $c = vextq_u32::<2>($c, $c);
        $d = vextq_u32::<1>($d, $d);
    }};
}

#[inline(always)]
pub unsafe fn compress_in_place_neon(
    cv: &mut [u32; 8],
    block: *const u8,
    block_len: u32,
    counter: u64,
    flags: u32,
) {
    let msg = load_msg(block);
    let mut a = set4_from_ptr(cv.as_ptr());
    let mut b = set4_from_ptr(cv.as_ptr().add(4));
    let mut c = iv_low_vec();
    let mut d = set4_volatile(counter as u32, (counter >> 32) as u32, block_len, flags);

    round!(
        a,
        b,
        c,
        d,
        msg,
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
    );
    round!(
        a,
        b,
        c,
        d,
        msg,
        [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8]
    );
    round!(
        a,
        b,
        c,
        d,
        msg,
        [3, 4, 10, 12, 13, 2, 7, 14, 6, 5, 9, 0, 11, 15, 8, 1]
    );
    round!(
        a,
        b,
        c,
        d,
        msg,
        [10, 7, 12, 9, 14, 3, 13, 15, 4, 0, 11, 2, 5, 8, 1, 6]
    );
    round!(
        a,
        b,
        c,
        d,
        msg,
        [12, 13, 9, 11, 15, 10, 14, 8, 7, 2, 5, 3, 0, 1, 6, 4]
    );
    round!(
        a,
        b,
        c,
        d,
        msg,
        [9, 14, 11, 5, 8, 12, 15, 1, 13, 3, 0, 10, 2, 6, 4, 7]
    );
    round!(
        a,
        b,
        c,
        d,
        msg,
        [11, 15, 5, 0, 1, 9, 8, 6, 14, 10, 2, 12, 3, 4, 7, 13]
    );

    let lo = veorq_u32(a, c);
    let hi = veorq_u32(b, d);
    let mut lanes = [0u32; 8];
    core::arch::aarch64::vst1q_u32(lanes.as_mut_ptr(), lo);
    core::arch::aarch64::vst1q_u32(lanes.as_mut_ptr().add(4), hi);
    cv[0] = lanes[0];
    cv[1] = lanes[1];
    cv[2] = lanes[2];
    cv[3] = lanes[3];
    cv[4] = lanes[4];
    cv[5] = lanes[5];
    cv[6] = lanes[6];
    cv[7] = lanes[7];
}

#[inline(always)]
pub unsafe fn hash_small_neon(input: *const u8, len: usize, out: *mut u8) {
    let mut cv = [0u32; 8];
    fill_iv(&mut cv);
    let mut offset = 0usize;
    let mut remaining = len;

    while remaining > BLOCK_LEN {
        let flags = if offset == 0 { CHUNK_START } else { 0 };
        compress_in_place_neon(&mut cv, input.add(offset), BLOCK_LEN as u32, 0, flags);
        offset += BLOCK_LEN;
        remaining -= BLOCK_LEN;
    }

    let flags = CHUNK_END | ROOT | if offset == 0 { CHUNK_START } else { 0 };
    if remaining == BLOCK_LEN {
        compress_in_place_neon(&mut cv, input.add(offset), BLOCK_LEN as u32, 0, flags);
    } else {
        let mut block = [0u8; BLOCK_LEN];
        let mut i = 0usize;
        while i < remaining {
            block
                .as_mut_ptr()
                .add(i)
                .write_volatile(input.add(offset + i).read_volatile());
            i += 1;
        }
        compress_in_place_neon(&mut cv, block.as_ptr(), remaining as u32, 0, flags);
    }
    store_cv(out, &cv);
}

#[inline(always)]
pub unsafe fn fold_parent_neon(left: *const u8, right: *const u8, out: *mut u8) {
    let mut block = [0u8; BLOCK_LEN];
    let mut i = 0usize;
    while i < OUT_LEN {
        block
            .as_mut_ptr()
            .add(i)
            .write_volatile(left.add(i).read_volatile());
        block
            .as_mut_ptr()
            .add(OUT_LEN + i)
            .write_volatile(right.add(i).read_volatile());
        i += 1;
    }
    let mut cv = [0u32; 8];
    fill_iv(&mut cv);
    compress_in_place_neon(&mut cv, block.as_ptr(), BLOCK_LEN as u32, 0, PARENT);
    store_cv(out, &cv);
}

#[inline(always)]
pub unsafe fn hash_batch_neon(
    input: *const u8,
    len: usize,
    count: usize,
    stride: usize,
    out: *mut u8,
) {
    let mut i = 0usize;
    while i < count {
        hash_small_neon(input.add(i * stride), len, out.add(i * OUT_LEN));
        i += 1;
    }
}

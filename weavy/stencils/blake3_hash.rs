//! BLAKE3 hash stencil for the hash-as-field spike.
//!
//! Frame immediates: [input_ptr_off, len_off, scratch_ptr_off, out_ptr_off].
//! The frame stores raw pointer/length words at those offsets. `len` must be a
//! power-of-two multiple of 1024 bytes; the spike bench uses 1KiB, 64KiB, and
//! 1MiB.

#![cfg_attr(tailcall, feature(explicit_tail_calls))]
#![allow(clippy::missing_safety_doc)]
#![allow(incomplete_features)]

const BLOCK_LEN: usize = 64;
const CHUNK_LEN: usize = 1024;
const OUT_LEN: usize = 32;

const CHUNK_START: u32 = 1 << 0;
const CHUNK_END: u32 = 1 << 1;
const PARENT: u32 = 1 << 2;
const ROOT: u32 = 1 << 3;

#[repr(C)]
pub struct Ctx {
    pub prog: *const u64,
    pub frame: *mut u8,
    pub ready: *mut i64,
    pub awaited: *const i64,
    pub resume: *mut u64,
    pub await_index: *mut u64,
    pub exit: *mut i64,
}

extern "C" {
    fn weavy_cont(cx: *mut Ctx);
}

macro_rules! cont {
    ($cx:ident) => {{
        #[cfg(tailcall)]
        {
            become weavy_cont($cx);
        }
        #[cfg(not(tailcall))]
        {
            weavy_cont($cx);
        }
    }};
}

#[inline(always)]
unsafe fn read_u64(frame: *mut u8, off: u64) -> u64 {
    (frame.add(off as usize) as *const u64).read_unaligned()
}

#[inline(always)]
unsafe fn read_u32_le(ptr: *const u8) -> u32 {
    u32::from_le_bytes([*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)])
}

#[inline(always)]
unsafe fn write_u32_le(ptr: *mut u8, value: u32) {
    let bytes = value.to_le_bytes();
    *ptr = bytes[0];
    *ptr.add(1) = bytes[1];
    *ptr.add(2) = bytes[2];
    *ptr.add(3) = bytes[3];
}

#[inline(always)]
unsafe fn load_cv(ptr: *const u8) -> [u32; 8] {
    [
        read_u32_le(ptr),
        read_u32_le(ptr.add(4)),
        read_u32_le(ptr.add(8)),
        read_u32_le(ptr.add(12)),
        read_u32_le(ptr.add(16)),
        read_u32_le(ptr.add(20)),
        read_u32_le(ptr.add(24)),
        read_u32_le(ptr.add(28)),
    ]
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
fn iv() -> [u32; 8] {
    [
        0x6A09_E667,
        0xBB67_AE85,
        0x3C6E_F372,
        0xA54F_F53A,
        0x510E_527F,
        0x9B05_688C,
        0x1F83_D9AB,
        0x5BE0_CD19,
    ]
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
fn g(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize, x: u32, y: u32) {
    state[a] = state[a].wrapping_add(state[b]).wrapping_add(x);
    state[d] = (state[d] ^ state[a]).rotate_right(16);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_right(12);
    state[a] = state[a].wrapping_add(state[b]).wrapping_add(y);
    state[d] = (state[d] ^ state[a]).rotate_right(8);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_right(7);
}

macro_rules! round {
    ($state:ident, $msg:ident, [$a0:literal, $a1:literal, $a2:literal, $a3:literal, $a4:literal, $a5:literal, $a6:literal, $a7:literal, $a8:literal, $a9:literal, $a10:literal, $a11:literal, $a12:literal, $a13:literal, $a14:literal, $a15:literal]) => {{
        g(&mut $state, 0, 4, 8, 12, $msg[$a0], $msg[$a1]);
        g(&mut $state, 1, 5, 9, 13, $msg[$a2], $msg[$a3]);
        g(&mut $state, 2, 6, 10, 14, $msg[$a4], $msg[$a5]);
        g(&mut $state, 3, 7, 11, 15, $msg[$a6], $msg[$a7]);
        g(&mut $state, 0, 5, 10, 15, $msg[$a8], $msg[$a9]);
        g(&mut $state, 1, 6, 11, 12, $msg[$a10], $msg[$a11]);
        g(&mut $state, 2, 7, 8, 13, $msg[$a12], $msg[$a13]);
        g(&mut $state, 3, 4, 9, 14, $msg[$a14], $msg[$a15]);
    }};
}

#[inline(always)]
unsafe fn compress_in_place(
    cv: &mut [u32; 8],
    block: *const u8,
    block_len: u32,
    counter: u64,
    flags: u32,
) {
    let msg = load_msg(block);
    let mut state = [
        cv[0],
        cv[1],
        cv[2],
        cv[3],
        cv[4],
        cv[5],
        cv[6],
        cv[7],
        0x6A09_E667,
        0xBB67_AE85,
        0x3C6E_F372,
        0xA54F_F53A,
        counter as u32,
        (counter >> 32) as u32,
        block_len,
        flags,
    ];

    round!(
        state,
        msg,
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
    );
    round!(
        state,
        msg,
        [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8]
    );
    round!(
        state,
        msg,
        [3, 4, 10, 12, 13, 2, 7, 14, 6, 5, 9, 0, 11, 15, 8, 1]
    );
    round!(
        state,
        msg,
        [10, 7, 12, 9, 14, 3, 13, 15, 4, 0, 11, 2, 5, 8, 1, 6]
    );
    round!(
        state,
        msg,
        [12, 13, 9, 11, 15, 10, 14, 8, 7, 2, 5, 3, 0, 1, 6, 4]
    );
    round!(
        state,
        msg,
        [9, 14, 11, 5, 8, 12, 15, 1, 13, 3, 0, 10, 2, 6, 4, 7]
    );
    round!(
        state,
        msg,
        [11, 15, 5, 0, 1, 9, 8, 6, 14, 10, 2, 12, 3, 4, 7, 13]
    );

    cv[0] = state[0] ^ state[8];
    cv[1] = state[1] ^ state[9];
    cv[2] = state[2] ^ state[10];
    cv[3] = state[3] ^ state[11];
    cv[4] = state[4] ^ state[12];
    cv[5] = state[5] ^ state[13];
    cv[6] = state[6] ^ state[14];
    cv[7] = state[7] ^ state[15];
}

#[inline(always)]
unsafe fn hash_full_chunk(input: *const u8, chunk_index: u64, root: bool, out: *mut u8) {
    let mut cv = iv();
    let mut block = 0usize;
    while block < (CHUNK_LEN / BLOCK_LEN) - 1 {
        let flags = if block == 0 { CHUNK_START } else { 0 };
        compress_in_place(
            &mut cv,
            input.add(block * BLOCK_LEN),
            BLOCK_LEN as u32,
            chunk_index,
            flags,
        );
        block += 1;
    }
    let mut flags = CHUNK_END;
    if root {
        flags |= ROOT;
    }
    compress_in_place(
        &mut cv,
        input.add(block * BLOCK_LEN),
        BLOCK_LEN as u32,
        chunk_index,
        flags,
    );
    store_cv(out, &cv);
}

#[inline(always)]
unsafe fn parent_cv(left: *const u8, right: *const u8, root: bool, out: *mut u8) {
    let mut block = [0u8; 64];
    let mut i = 0usize;
    while i < OUT_LEN {
        block[i] = *left.add(i);
        block[OUT_LEN + i] = *right.add(i);
        i += 1;
    }
    let mut cv = iv();
    let mut flags = PARENT;
    if root {
        flags |= ROOT;
    }
    compress_in_place(&mut cv, block.as_ptr(), BLOCK_LEN as u32, 0, flags);
    store_cv(out, &cv);
}

#[inline(always)]
unsafe fn hash_power_two_chunks(input: *const u8, len: usize, scratch: *mut u8, out: *mut u8) {
    let chunks = len / CHUNK_LEN;
    if chunks == 1 {
        hash_full_chunk(input, 0, true, out);
        return;
    }

    let mut chunk = 0usize;
    while chunk < chunks {
        hash_full_chunk(
            input.add(chunk * CHUNK_LEN),
            chunk as u64,
            false,
            scratch.add(chunk * OUT_LEN),
        );
        chunk += 1;
    }

    let mut count = chunks;
    while count > 2 {
        let mut i = 0usize;
        while i < count / 2 {
            parent_cv(
                scratch.add(2 * i * OUT_LEN),
                scratch.add((2 * i + 1) * OUT_LEN),
                false,
                scratch.add(i * OUT_LEN),
            );
            i += 1;
        }
        count /= 2;
    }

    parent_cv(scratch, scratch.add(OUT_LEN), true, out);
}

#[no_mangle]
pub unsafe extern "C" fn weavy_blake3_hash(cx: *mut Ctx) {
    let c = &mut *cx;
    let input_ptr_off = *c.prog;
    let len_off = *c.prog.add(1);
    let scratch_ptr_off = *c.prog.add(2);
    let out_ptr_off = *c.prog.add(3);
    c.prog = c.prog.add(4);

    let input = read_u64(c.frame, input_ptr_off) as *const u8;
    let len = read_u64(c.frame, len_off) as usize;
    let scratch = read_u64(c.frame, scratch_ptr_off) as *mut u8;
    let out = read_u64(c.frame, out_ptr_off) as *mut u8;
    hash_power_two_chunks(input, len, scratch, out);
    cont!(cx);
}

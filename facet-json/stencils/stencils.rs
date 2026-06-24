//! facet-json native stencils.

#![allow(clippy::missing_safety_doc)]

const STATUS_FALLBACK: u64 = 1;

#[repr(C)]
pub struct HostCallCtx {
    pub prog: *const u64,
    pub inner: *mut JsonStatePrefix,
}

#[repr(C)]
pub struct JsonStatePrefix {
    pub input: *const u8,
    pub input_len: usize,
    pub cursor_pos: usize,
    pub status: u64,
}

extern "C" {
    fn facet_json_cont(cx: *mut HostCallCtx);
}

#[inline(always)]
unsafe fn input_byte(state: &JsonStatePrefix, pos: usize) -> Option<u8> {
    if pos < state.input_len {
        Some(*state.input.add(pos))
    } else {
        None
    }
}

#[inline(always)]
unsafe fn skip_json_whitespace(state: &JsonStatePrefix, mut pos: usize) -> Option<usize> {
    loop {
        match input_byte(state, pos) {
            Some(b' ' | b'\t' | b'\n' | b'\r') => pos += 1,
            Some(b'/') => return None,
            Some(_) => return Some(pos),
            None => return None,
        }
    }
}

#[inline(always)]
unsafe fn consume_root_byte(cx: *mut HostCallCtx, expected: u8) {
    let c = &mut *cx;
    let state = &mut *c.inner;
    let Some(pos) = skip_json_whitespace(state, state.cursor_pos) else {
        state.status = STATUS_FALLBACK;
        return;
    };
    if input_byte(state, pos) != Some(expected) {
        state.status = STATUS_FALLBACK;
        return;
    }

    state.cursor_pos = pos + 1;

    facet_json_cont(cx);
}

#[no_mangle]
pub unsafe extern "C" fn facet_json_stencil_root_object_start(cx: *mut HostCallCtx) {
    consume_root_byte(cx, b'{');
}

#[no_mangle]
pub unsafe extern "C" fn facet_json_stencil_root_array_start(cx: *mut HostCallCtx) {
    consume_root_byte(cx, b'[');
}

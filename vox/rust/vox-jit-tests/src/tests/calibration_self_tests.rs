//! Calibration self-tests (task #15).
//!
//! These tests implement `CalibrationSelfCheck` and gate whether a concrete T
//! may use the JIT fast path. They run synchronously via
//! `CalibrationRegistry::calibrate_vec_gated` / `calibrate_string_gated`.
//!
//! A type may only use the opaque fast path after ALL checks here pass.
//!
//! For Miri coverage: run with `MIRIFLAGS="-Zmiri-strict-provenance"` on the
//! calibration probe code itself (`vox-jit-cal`). The checks here validate
//! the descriptor's logical invariants; Miri validates the unsafe probe code.

use vox_jit_cal::{
    CalibrationRegistry, CalibrationResult, CalibrationSelfCheck, ContainerKind, GatedResult,
    OFFSET_ABSENT, OpaqueDescriptor, SelfCheckFailure, calibrate_vec,
};

// ---------------------------------------------------------------------------
// Canonical self-checker for Vec<T>
// ---------------------------------------------------------------------------

/// Runs the complete set of invariant checks on a `Vec<T>` descriptor.
///
/// Pass this to `calibrate_vec_gated` before enabling the fast path.
pub struct VecSelfCheck {
    pub expected_elem_size: usize,
    pub expected_elem_align: usize,
}

impl CalibrationSelfCheck for VecSelfCheck {
    fn check(&self, desc: &OpaqueDescriptor) -> Result<(), SelfCheckFailure> {
        let pw = std::mem::size_of::<usize>();

        // Offsets must be distinct.
        if desc.ptr_offset == desc.len_offset {
            return Err(SelfCheckFailure {
                check: "vec.offsets.distinct",
                reason: format!("ptr_offset == len_offset == {}", desc.ptr_offset),
            });
        }
        if desc.ptr_offset == desc.cap_offset {
            return Err(SelfCheckFailure {
                check: "vec.offsets.distinct",
                reason: format!("ptr_offset == cap_offset == {}", desc.ptr_offset),
            });
        }
        if desc.len_offset == desc.cap_offset {
            return Err(SelfCheckFailure {
                check: "vec.offsets.distinct",
                reason: format!("len_offset == cap_offset == {}", desc.len_offset),
            });
        }

        // All offsets must be word-aligned and within `size`.
        for &off in &[desc.ptr_offset, desc.len_offset, desc.cap_offset] {
            if off as usize % pw != 0 {
                return Err(SelfCheckFailure {
                    check: "vec.offsets.aligned",
                    reason: format!("offset {off} is not word-aligned (pw={pw})"),
                });
            }
            if off as usize >= desc.size {
                return Err(SelfCheckFailure {
                    check: "vec.offsets.in-range",
                    reason: format!("offset {off} >= size {}", desc.size),
                });
            }
        }

        // empty_bytes length must match size.
        if desc.empty_bytes.len() != desc.size {
            return Err(SelfCheckFailure {
                check: "vec.empty-bytes.length",
                reason: format!(
                    "empty_bytes.len()={} != size={}",
                    desc.empty_bytes.len(),
                    desc.size
                ),
            });
        }

        // size == 3 * pointer_width.
        if desc.size != 3 * pw {
            return Err(SelfCheckFailure {
                check: "vec.size",
                reason: format!("size={} != 3*pw={}", desc.size, 3 * pw),
            });
        }

        // Element metadata.
        if desc.elem_size != self.expected_elem_size {
            return Err(SelfCheckFailure {
                check: "vec.elem-size",
                reason: format!(
                    "elem_size={} != expected={}",
                    desc.elem_size, self.expected_elem_size
                ),
            });
        }
        if desc.elem_align != self.expected_elem_align {
            return Err(SelfCheckFailure {
                check: "vec.elem-align",
                reason: format!(
                    "elem_align={} != expected={}",
                    desc.elem_align, self.expected_elem_align
                ),
            });
        }

        // The len slot in empty_bytes must be zero (empty vec has len=0).
        let len_word = read_word_at(&desc.empty_bytes, desc.len_offset as usize);
        if len_word != 0 {
            return Err(SelfCheckFailure {
                check: "vec.empty-bytes.len-zero",
                reason: format!("len slot in empty_bytes is {len_word}, expected 0"),
            });
        }

        // The cap slot in empty_bytes must be zero (empty vec has cap=0).
        let cap_word = read_word_at(&desc.empty_bytes, desc.cap_offset as usize);
        if cap_word != 0 {
            return Err(SelfCheckFailure {
                check: "vec.empty-bytes.cap-zero",
                reason: format!("cap slot in empty_bytes is {cap_word}, expected 0"),
            });
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Canonical self-checker for String
// ---------------------------------------------------------------------------

pub struct StringSelfCheck;

impl CalibrationSelfCheck for StringSelfCheck {
    fn check(&self, desc: &OpaqueDescriptor) -> Result<(), SelfCheckFailure> {
        let pw = std::mem::size_of::<usize>();

        // String is always Vec<u8>-shaped: elem_size=1, elem_align=1.
        if desc.elem_size != 1 {
            return Err(SelfCheckFailure {
                check: "string.elem-size",
                reason: format!("elem_size={}, expected 1", desc.elem_size),
            });
        }
        if desc.elem_align != 1 {
            return Err(SelfCheckFailure {
                check: "string.elem-align",
                reason: format!("elem_align={}, expected 1", desc.elem_align),
            });
        }

        // size == 3 * pointer_width.
        if desc.size != 3 * pw {
            return Err(SelfCheckFailure {
                check: "string.size",
                reason: format!("size={} != 3*pw={}", desc.size, 3 * pw),
            });
        }

        // Offsets distinct.
        if desc.ptr_offset == desc.len_offset
            || desc.ptr_offset == desc.cap_offset
            || desc.len_offset == desc.cap_offset
        {
            return Err(SelfCheckFailure {
                check: "string.offsets.distinct",
                reason: format!(
                    "offsets not distinct: ptr={} len={} cap={}",
                    desc.ptr_offset, desc.len_offset, desc.cap_offset
                ),
            });
        }

        // empty_bytes must have len-slot == 0 and cap-slot == 0.
        let len_word = read_word_at(&desc.empty_bytes, desc.len_offset as usize);
        if len_word != 0 {
            return Err(SelfCheckFailure {
                check: "string.empty-bytes.len-zero",
                reason: format!("len slot in empty_bytes is {len_word}"),
            });
        }
        let cap_word = read_word_at(&desc.empty_bytes, desc.cap_offset as usize);
        if cap_word != 0 {
            return Err(SelfCheckFailure {
                check: "string.empty-bytes.cap-zero",
                reason: format!("cap slot in empty_bytes is {cap_word}"),
            });
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Canonical self-checker for Box<T>
// ---------------------------------------------------------------------------

pub struct BoxOwnedSelfCheck {
    pub expected_elem_size: usize,
    pub expected_elem_align: usize,
}

impl CalibrationSelfCheck for BoxOwnedSelfCheck {
    fn check(&self, desc: &OpaqueDescriptor) -> Result<(), SelfCheckFailure> {
        let pw = std::mem::size_of::<usize>();

        if desc.kind != ContainerKind::BoxOwned {
            return Err(SelfCheckFailure {
                check: "box_owned.kind",
                reason: format!("expected BoxOwned, got {:?}", desc.kind),
            });
        }

        // Box<T> has no length or capacity.
        if desc.len_offset != OFFSET_ABSENT {
            return Err(SelfCheckFailure {
                check: "box_owned.len-absent",
                reason: format!("len_offset={} but expected OFFSET_ABSENT", desc.len_offset),
            });
        }
        if desc.cap_offset != OFFSET_ABSENT {
            return Err(SelfCheckFailure {
                check: "box_owned.cap-absent",
                reason: format!("cap_offset={} but expected OFFSET_ABSENT", desc.cap_offset),
            });
        }

        // Box<T> is one pointer word.
        if desc.size != pw {
            return Err(SelfCheckFailure {
                check: "box_owned.size",
                reason: format!("size={} != pw={}", desc.size, pw),
            });
        }

        // ptr_offset must be 0 (the single word is the pointer).
        if desc.ptr_offset as usize != 0 {
            return Err(SelfCheckFailure {
                check: "box_owned.ptr-offset",
                reason: format!("ptr_offset={}, expected 0", desc.ptr_offset),
            });
        }

        if desc.elem_size != self.expected_elem_size {
            return Err(SelfCheckFailure {
                check: "box_owned.elem-size",
                reason: format!(
                    "elem_size={} != expected={}",
                    desc.elem_size, self.expected_elem_size
                ),
            });
        }
        if desc.elem_align != self.expected_elem_align {
            return Err(SelfCheckFailure {
                check: "box_owned.elem-align",
                reason: format!(
                    "elem_align={} != expected={}",
                    desc.elem_align, self.expected_elem_align
                ),
            });
        }

        // empty_bytes length must match size.
        if desc.empty_bytes.len() != desc.size {
            return Err(SelfCheckFailure {
                check: "box_owned.empty-bytes.length",
                reason: format!(
                    "empty_bytes.len()={} != size={}",
                    desc.empty_bytes.len(),
                    desc.size
                ),
            });
        }

        // The pointer word in empty_bytes must be non-zero (dangling sentinel, not null).
        let ptr_word = read_word_at(&desc.empty_bytes, 0);
        if ptr_word == 0 {
            return Err(SelfCheckFailure {
                check: "box_owned.empty-bytes.ptr-nonzero",
                reason: "dangling sentinel pointer in empty_bytes is null".to_string(),
            });
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Canonical self-checker for Box<[T]>
// ---------------------------------------------------------------------------

pub struct BoxSliceSelfCheck {
    pub expected_elem_size: usize,
    pub expected_elem_align: usize,
}

impl CalibrationSelfCheck for BoxSliceSelfCheck {
    fn check(&self, desc: &OpaqueDescriptor) -> Result<(), SelfCheckFailure> {
        let pw = std::mem::size_of::<usize>();

        if desc.kind != ContainerKind::BoxSlice {
            return Err(SelfCheckFailure {
                check: "box_slice.kind",
                reason: format!("expected BoxSlice, got {:?}", desc.kind),
            });
        }

        // Box<[T]> has a length but no capacity.
        if desc.len_offset == OFFSET_ABSENT {
            return Err(SelfCheckFailure {
                check: "box_slice.len-present",
                reason: "len_offset is OFFSET_ABSENT, expected a real offset".to_string(),
            });
        }
        if desc.cap_offset != OFFSET_ABSENT {
            return Err(SelfCheckFailure {
                check: "box_slice.cap-absent",
                reason: format!("cap_offset={} but expected OFFSET_ABSENT", desc.cap_offset),
            });
        }

        // Box<[T]> is a fat pointer: two words.
        if desc.size != 2 * pw {
            return Err(SelfCheckFailure {
                check: "box_slice.size",
                reason: format!("size={} != 2*pw={}", desc.size, 2 * pw),
            });
        }

        // Both ptr_offset and len_offset must be word-aligned and within size.
        for &off in &[desc.ptr_offset, desc.len_offset] {
            if off as usize % pw != 0 {
                return Err(SelfCheckFailure {
                    check: "box_slice.offsets.aligned",
                    reason: format!("offset {off} is not word-aligned (pw={pw})"),
                });
            }
            if off as usize >= desc.size {
                return Err(SelfCheckFailure {
                    check: "box_slice.offsets.in-range",
                    reason: format!("offset {off} >= size {}", desc.size),
                });
            }
        }

        // ptr and len must be at distinct offsets.
        if desc.ptr_offset == desc.len_offset {
            return Err(SelfCheckFailure {
                check: "box_slice.offsets.distinct",
                reason: format!("ptr_offset == len_offset == {}", desc.ptr_offset),
            });
        }

        if desc.elem_size != self.expected_elem_size {
            return Err(SelfCheckFailure {
                check: "box_slice.elem-size",
                reason: format!(
                    "elem_size={} != expected={}",
                    desc.elem_size, self.expected_elem_size
                ),
            });
        }
        if desc.elem_align != self.expected_elem_align {
            return Err(SelfCheckFailure {
                check: "box_slice.elem-align",
                reason: format!(
                    "elem_align={} != expected={}",
                    desc.elem_align, self.expected_elem_align
                ),
            });
        }

        // empty_bytes length must match size.
        if desc.empty_bytes.len() != desc.size {
            return Err(SelfCheckFailure {
                check: "box_slice.empty-bytes.length",
                reason: format!(
                    "empty_bytes.len()={} != size={}",
                    desc.empty_bytes.len(),
                    desc.size
                ),
            });
        }

        // The len slot in empty_bytes must be 0 (empty slice has len=0).
        let len_word = read_word_at(&desc.empty_bytes, desc.len_offset as usize);
        if len_word != 0 {
            return Err(SelfCheckFailure {
                check: "box_slice.empty-bytes.len-zero",
                reason: format!("len slot in empty_bytes is {len_word}, expected 0"),
            });
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn read_word_at(bytes: &[u8], offset: usize) -> usize {
    const PW: usize = std::mem::size_of::<usize>();
    assert!(
        offset + PW <= bytes.len(),
        "read_word_at: offset {offset} out of bounds for len {}",
        bytes.len()
    );
    // usize::from_le_bytes requires exactly size_of::<usize>() bytes.
    // try_into() on &[u8] of length PW produces [u8; PW].
    let arr: [u8; PW] = bytes[offset..offset + PW].try_into().unwrap();
    usize::from_le_bytes(arr)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

fn assert_gated_ready(result: GatedResult, label: &str) {
    match result {
        GatedResult::Ready(_) => {}
        GatedResult::CalibrationFailed { reason } => {
            panic!("{label} calibration failed: {reason}")
        }
        GatedResult::SelfCheckFailed(f) => panic!("{label} self-check failed: {f}"),
    }
}

#[test]
fn vec_u8_self_check_passes() {
    let mut reg = CalibrationRegistry::new();
    let checker = VecSelfCheck {
        expected_elem_size: 1,
        expected_elem_align: 1,
    };
    assert_gated_ready(reg.calibrate_vec_gated::<u8>(&checker), "Vec<u8>");
}

#[test]
fn vec_u32_self_check_passes() {
    let mut reg = CalibrationRegistry::new();
    let checker = VecSelfCheck {
        expected_elem_size: 4,
        expected_elem_align: 4,
    };
    assert_gated_ready(reg.calibrate_vec_gated::<u32>(&checker), "Vec<u32>");
}

#[test]
fn vec_u64_self_check_passes() {
    let mut reg = CalibrationRegistry::new();
    let checker = VecSelfCheck {
        expected_elem_size: 8,
        expected_elem_align: 8,
    };
    assert_gated_ready(reg.calibrate_vec_gated::<u64>(&checker), "Vec<u64>");
}

#[test]
fn string_self_check_passes() {
    let mut reg = CalibrationRegistry::new();
    let checker = StringSelfCheck;
    assert_gated_ready(reg.calibrate_string_gated(&checker), "String");
}

#[test]
fn self_check_wrong_elem_size_is_rejected() {
    // Deliberately wrong expected elem size should cause rejection.
    let checker = VecSelfCheck {
        expected_elem_size: 99, // wrong
        expected_elem_align: 1,
    };
    if let CalibrationResult::Ok(desc) = calibrate_vec::<u8>() {
        let result = checker.check(&desc);
        assert!(
            result.is_err(),
            "wrong elem_size expectation should fail the self-check"
        );
        let err = result.unwrap_err();
        assert_eq!(err.check, "vec.elem-size");
    }
}

#[test]
fn vec_and_string_get_independent_handles() {
    let mut reg = CalibrationRegistry::new();
    let vec_checker = VecSelfCheck {
        expected_elem_size: 1,
        expected_elem_align: 1,
    };
    let str_checker = StringSelfCheck;

    let h_vec = match reg.calibrate_vec_gated::<u8>(&vec_checker) {
        GatedResult::Ready(h) => h,
        GatedResult::CalibrationFailed { reason } => {
            panic!("Vec<u8> calibration failed: {reason}")
        }
        GatedResult::SelfCheckFailed(f) => panic!("Vec<u8> self-check failed: {f}"),
    };
    let h_str = match reg.calibrate_string_gated(&str_checker) {
        GatedResult::Ready(h) => h,
        GatedResult::CalibrationFailed { reason } => {
            panic!("String calibration failed: {reason}")
        }
        GatedResult::SelfCheckFailed(f) => panic!("String self-check failed: {f}"),
    };

    assert_ne!(h_vec, h_str, "Vec<u8> and String must get distinct handles");

    // Both descriptors must be retrievable.
    assert!(reg.get(h_vec).is_some());
    assert!(reg.get(h_str).is_some());
}

#[test]
fn descriptor_empty_bytes_round_trip_vec_u32() {
    // The empty_bytes recorded by calibration must represent a valid empty Vec<u32>:
    // reading the len and cap slots from empty_bytes must both be 0.
    if let CalibrationResult::Ok(desc) = calibrate_vec::<u32>() {
        let checker = VecSelfCheck {
            expected_elem_size: 4,
            expected_elem_align: 4,
        };
        assert!(
            checker.check(&desc).is_ok(),
            "Vec<u32> descriptor failed self-check"
        );

        // Verify that writing empty_bytes into a ManuallyDrop<Vec<u32>> slot
        // and reading it back gives an empty vec.
        use std::mem::ManuallyDrop;
        let mut buf = std::mem::MaybeUninit::<Vec<u32>>::uninit();
        let ptr = buf.as_mut_ptr() as *mut u8;
        unsafe {
            std::ptr::copy_nonoverlapping(desc.empty_bytes.as_ptr(), ptr, desc.size);
            // SAFETY: we just wrote the exact bytes of Vec::<u32>::new().
            let v = buf.assume_init();
            let wrapped = ManuallyDrop::new(v);
            assert!(wrapped.is_empty(), "empty_bytes must produce an empty vec");
            assert_eq!(wrapped.len(), 0);
            // Do not drop — ManuallyDrop prevents calling Vec::drop on
            // what is effectively a stack copy of an empty vec (no heap).
        }
    }
}

// ---------------------------------------------------------------------------
// Box<T> self-check tests
// ---------------------------------------------------------------------------

#[test]
fn box_owned_u32_self_check_passes() {
    let mut reg = CalibrationRegistry::new();
    let checker = BoxOwnedSelfCheck {
        expected_elem_size: 4,
        expected_elem_align: 4,
    };
    assert_gated_ready(reg.calibrate_box_t_gated::<u32>(&checker), "Box<u32>");
}

#[test]
fn box_owned_u8_self_check_passes() {
    let mut reg = CalibrationRegistry::new();
    let checker = BoxOwnedSelfCheck {
        expected_elem_size: 1,
        expected_elem_align: 1,
    };
    assert_gated_ready(reg.calibrate_box_t_gated::<u8>(&checker), "Box<u8>");
}

#[test]
fn box_owned_wrong_elem_size_is_rejected() {
    use vox_jit_cal::calibrate_box_t;
    let checker = BoxOwnedSelfCheck {
        expected_elem_size: 99, // wrong
        expected_elem_align: 4,
    };
    if let CalibrationResult::Ok(desc) = calibrate_box_t::<u32>() {
        let result = checker.check(&desc);
        assert!(
            result.is_err(),
            "wrong elem_size should fail the self-check"
        );
        assert_eq!(result.unwrap_err().check, "box_owned.elem-size");
    }
}

// ---------------------------------------------------------------------------
// Box<[T]> self-check tests
// ---------------------------------------------------------------------------

#[test]
fn box_slice_u32_self_check_passes() {
    let mut reg = CalibrationRegistry::new();
    let checker = BoxSliceSelfCheck {
        expected_elem_size: 4,
        expected_elem_align: 4,
    };
    assert_gated_ready(reg.calibrate_box_slice_gated::<u32>(&checker), "Box<[u32]>");
}

#[test]
fn box_slice_u8_self_check_passes() {
    let mut reg = CalibrationRegistry::new();
    let checker = BoxSliceSelfCheck {
        expected_elem_size: 1,
        expected_elem_align: 1,
    };
    assert_gated_ready(reg.calibrate_box_slice_gated::<u8>(&checker), "Box<[u8]>");
}

#[test]
fn box_slice_wrong_elem_size_is_rejected() {
    use vox_jit_cal::calibrate_box_slice;
    let checker = BoxSliceSelfCheck {
        expected_elem_size: 99, // wrong
        expected_elem_align: 4,
    };
    if let CalibrationResult::Ok(desc) = calibrate_box_slice::<u32>() {
        let result = checker.check(&desc);
        assert!(
            result.is_err(),
            "wrong elem_size should fail the self-check"
        );
        assert_eq!(result.unwrap_err().check, "box_slice.elem-size");
    }
}

#[test]
fn box_and_vec_get_independent_handles() {
    let mut reg = CalibrationRegistry::new();
    let vec_checker = VecSelfCheck {
        expected_elem_size: 4,
        expected_elem_align: 4,
    };
    let box_t_checker = BoxOwnedSelfCheck {
        expected_elem_size: 4,
        expected_elem_align: 4,
    };
    let box_slice_checker = BoxSliceSelfCheck {
        expected_elem_size: 4,
        expected_elem_align: 4,
    };

    let h_vec = match reg.calibrate_vec_gated::<u32>(&vec_checker) {
        GatedResult::Ready(h) => h,
        GatedResult::CalibrationFailed { reason } => {
            panic!("Vec<u32> calibration failed: {reason}")
        }
        GatedResult::SelfCheckFailed(f) => panic!("Vec<u32> self-check failed: {f}"),
    };
    let h_box_t = match reg.calibrate_box_t_gated::<u32>(&box_t_checker) {
        GatedResult::Ready(h) => h,
        GatedResult::CalibrationFailed { reason } => {
            panic!("Box<u32> calibration failed: {reason}")
        }
        GatedResult::SelfCheckFailed(f) => panic!("Box<u32> self-check failed: {f}"),
    };
    let h_box_slice = match reg.calibrate_box_slice_gated::<u32>(&box_slice_checker) {
        GatedResult::Ready(h) => h,
        GatedResult::CalibrationFailed { reason } => {
            panic!("Box<[u32]> calibration failed: {reason}")
        }
        GatedResult::SelfCheckFailed(f) => panic!("Box<[u32]> self-check failed: {f}"),
    };

    assert_ne!(
        h_vec, h_box_t,
        "Vec<u32> and Box<u32> must have distinct handles"
    );
    assert_ne!(
        h_vec, h_box_slice,
        "Vec<u32> and Box<[u32]> must have distinct handles"
    );
    assert_ne!(
        h_box_t, h_box_slice,
        "Box<u32> and Box<[u32]> must have distinct handles"
    );

    assert!(reg.get(h_vec).is_some());
    assert!(reg.get(h_box_t).is_some());
    assert!(reg.get(h_box_slice).is_some());
}

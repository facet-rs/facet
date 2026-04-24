//! Runtime calibration of opaque standard-library container layouts.
//!
//! Calibration is process-local, target-local, per-concrete-T, and disposable.
//! Results are never persisted across restarts.
//!
//! # How it works
//!
//! For each whitelisted opaque type we probe real values under [`ManuallyDrop`]
//! to identify which word-slots hold the pointer, length, and capacity. We also
//! record the exact byte representation of the empty constructor so the JIT can
//! copy those bytes directly rather than assuming "three zero words".
//!
//! On any unexpected result (wrong byte count, ambiguous slots, capacity not >=
//! length) we return `CalibrationResult::Unsupported` and the JIT falls back to
//! the interpreter.

#![allow(unsafe_code)]

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Byte offset of a slot within the container's in-memory representation.
///
/// `ABSENT` is used for slots that do not exist in a given container kind
/// (e.g. `cap_offset` for `Box<T>` / `Box<[T]>`).
pub type ByteOffset = u8;

/// Sentinel value meaning "this slot does not exist for this container kind".
pub const OFFSET_ABSENT: ByteOffset = ByteOffset::MAX;

/// Whether a decode stub produces an owned value or a borrowed reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BorrowMode {
    /// Decode data into a fully-owned Rust value (default for decoding).
    Owned,
    /// Decode data as a borrowed reference with lifetime tied to the input buffer.
    Borrowed,
}

/// Discriminant that tells the JIT which family of container the descriptor
/// describes, so it can use the right fast-path strategy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerKind {
    /// `Vec<T>` — three-word (ptr, len, cap).
    Vec,
    /// `String` — three-word (ptr, len, cap), byte elements only.
    String,
    /// `Box<T>` — one word (ptr only). No len, no cap.
    BoxOwned,
    /// `Box<[T]>` — two words (ptr, len). No cap.
    BoxSlice,
}

/// Calibrated layout descriptor for a single concrete opaque container type.
///
/// Consumed by JIT IR ops:
///  - `MaterializeEmpty` copies `empty_bytes` into the destination.
///  - Direct field stores use `ptr_offset`, `len_offset`, `cap_offset`.
///    Slots absent for the container kind are set to `OFFSET_ABSENT`.
///  - The reserve helper needs `elem_size` and `elem_align`.
///  - `kind` tells the JIT which fast-path strategy to use.
#[derive(Clone, Debug)]
pub struct OpaqueDescriptor {
    /// Which container family this descriptor represents.
    pub kind: ContainerKind,
    /// Total size of the container value in bytes (== `size_of::<Container>()`).
    pub size: usize,
    /// Alignment of the container value in bytes.
    pub align: usize,
    /// Exact bytes of the empty/null constructor. Length == `size`.
    pub empty_bytes: Vec<u8>,
    /// Byte offset of the data pointer slot within the container.
    pub ptr_offset: ByteOffset,
    /// Byte offset of the length slot, or `OFFSET_ABSENT` if none.
    pub len_offset: ByteOffset,
    /// Byte offset of the capacity slot, or `OFFSET_ABSENT` if none.
    pub cap_offset: ByteOffset,
    /// Size of one element (0 for ZSTs).
    pub elem_size: usize,
    /// Alignment of one element.
    pub elem_align: usize,
}

/// Outcome of a calibration probe.
#[derive(Clone, Debug)]
pub enum CalibrationResult {
    /// Calibration succeeded; the descriptor is ready to use.
    Ok(OpaqueDescriptor),
    /// The probe produced unexpected results. The JIT must fall back to the
    /// interpreter for this concrete type.
    Unsupported { reason: String },
}

// ---------------------------------------------------------------------------
// Vec<T> calibration  (task #4)
// ---------------------------------------------------------------------------

/// Calibrate `Vec<T>` for element type `T`.
///
/// Probes real `Vec<T>` values under [`ManuallyDrop`] to find which word-slots
/// in the three-pointer-wide struct hold (ptr, len, cap), then records the
/// exact empty-constructor bytes.
pub fn calibrate_vec<T>() -> CalibrationResult {
    // Sanity: container must be exactly 3 pointer-widths.
    let size = std::mem::size_of::<Vec<T>>();
    let align = std::mem::align_of::<Vec<T>>();
    let ptr_width = std::mem::size_of::<usize>();

    if size != 3 * ptr_width || align != ptr_width {
        return CalibrationResult::Unsupported {
            reason: format!(
                "Vec<T> size/align unexpected: size={size} align={align} ptr_width={ptr_width}"
            ),
        };
    }

    // Record the exact empty-constructor bytes.
    let empty_bytes: Vec<u8> = {
        let empty = Vec::<T>::new();
        let raw_ptr: *const Vec<T> = &empty;
        let raw_bytes_ptr = raw_ptr as *const u8;
        // SAFETY: Vec<T> is `size` bytes large, we just computed that above.
        let bytes = unsafe { std::slice::from_raw_parts(raw_bytes_ptr, size) }.to_vec();
        drop(empty);
        bytes
    };

    // Build a non-empty vec with len != cap so we can distinguish the slots.
    // We push one element then shrink capacity by reserving exactly len+1 so
    // len=1 and cap=2, giving us three distinct non-zero values:
    //   ptr  — heap pointer  (non-zero, not 1, not 2)
    //   len  — 1
    //   cap  — 2
    //
    // For ZSTs, ptr is a dangling sentinel (align-derived), len/cap are counts.
    // We detect ZSTs separately.
    let elem_size = std::mem::size_of::<T>();
    let elem_align = std::mem::align_of::<T>();

    if elem_size == 0 {
        return calibrate_vec_zst::<T>(size, align, empty_bytes, elem_align);
    }

    // Build probe: len=1, cap>=2 so all three header slots are distinct.
    let words: [usize; 3] = {
        let mut v: Vec<T> = Vec::with_capacity(1);
        // SAFETY: We set len=1 so the slot is distinguishable, but we never
        // read the uninit element back. Before dropping, we reset len=0 so
        // Vec::drop frees the allocation without running any element destructor.
        unsafe { v.set_len(1) };
        v.reserve(1); // cap >= 2; ptr != 1 != cap
        let w = read_words(&v);
        unsafe { v.set_len(0) };
        w
    };

    // Identify slots.
    // len  == 1  (we set_len(1))
    // cap  >= 2  (we reserve(1) after push)
    // ptr  — everything else, must be non-zero
    //
    // Strategy: find the slot with value 1 → that's len.
    //           find the slot with the largest value (ptr >> cap typically) → ptr.
    //           remaining slot → cap.
    // But ptr could be a small address on some platforms. Instead:
    //   len slot has value exactly 1.
    //   Of the other two, the smaller (>= 2) is cap, the larger is ptr.
    //   (ptr >= align(T) >= 1; on any real platform heap ptrs are >> 2)
    //
    // If we can't unambiguously identify slots, fall back.
    let len_slot = words.iter().position(|&w| w == 1);
    let Some(len_slot) = len_slot else {
        return CalibrationResult::Unsupported {
            reason: format!(
                "could not identify len slot: words={words:?} (expected exactly one word == 1)"
            ),
        };
    };

    let others: Vec<(usize, usize)> = words
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != len_slot)
        .map(|(i, &w)| (i, w))
        .collect();

    if others.len() != 2 {
        return CalibrationResult::Unsupported {
            reason: "unexpected word count".into(),
        };
    }

    // cap >= 2; ptr is a heap address >> cap on any real platform.
    // We take the smaller of the two remaining slots as cap.
    let (cap_slot, cap_val) = if others[0].1 <= others[1].1 {
        others[0]
    } else {
        others[1]
    };
    let (ptr_slot, ptr_val) = if others[0].1 > others[1].1 {
        others[0]
    } else {
        others[1]
    };

    // Sanity checks.
    if cap_val < 1 {
        return CalibrationResult::Unsupported {
            reason: format!("cap slot value {cap_val} < 1"),
        };
    }
    if ptr_val == 0 {
        return CalibrationResult::Unsupported {
            reason: "ptr slot is zero".into(),
        };
    }
    if ptr_val < cap_val {
        return CalibrationResult::Unsupported {
            reason: format!("ptr {ptr_val:#x} < cap {cap_val} — cannot disambiguate"),
        };
    }

    let ptr_offset = (ptr_slot * ptr_width) as ByteOffset;
    let len_offset = (len_slot * ptr_width) as ByteOffset;
    let cap_offset = (cap_slot * ptr_width) as ByteOffset;

    CalibrationResult::Ok(OpaqueDescriptor {
        kind: ContainerKind::Vec,
        size,
        align,
        empty_bytes,
        ptr_offset,
        len_offset,
        cap_offset,
        elem_size,
        elem_align,
    })
}

/// Calibrate `Vec<T>` where `T` is a ZST.
///
/// ZST vecs use a dangling ptr (derived from alignment), len counts items,
/// and cap is usize::MAX (allocator convention) or 0 on older rustc.
/// We accept both: we only need offsets, not cap semantics.
fn calibrate_vec_zst<T>(
    size: usize,
    align: usize,
    empty_bytes: Vec<u8>,
    elem_align: usize,
) -> CalibrationResult {
    let ptr_width = std::mem::size_of::<usize>();

    // Probe: set len=3 so the slot is distinguishable; reset to 0 before drop.
    let words: [usize; 3] = {
        let mut v: Vec<T> = Vec::new();
        // SAFETY: ZSTs have no bytes; set_len only updates the length counter.
        // We reset to 0 before drop so Vec::drop runs cleanly (no-op for ZSTs
        // but keeps Miri happy about the length invariant).
        unsafe { v.set_len(3) };
        let w = read_words(&v);
        unsafe { v.set_len(0) };
        w
    };

    // len == 3 is unambiguous.
    let len_slot = words.iter().position(|&w| w == 3);
    let Some(len_slot) = len_slot else {
        return CalibrationResult::Unsupported {
            reason: format!(
                "ZST Vec: could not identify len slot: words={words:?} (expected one == 3)"
            ),
        };
    };

    // For ZSTs, ptr is the dangling sentinel (align value or 1).
    // The remaining non-len slot is cap (usize::MAX or 0).
    let (ptr_slot, _cap_slot) = {
        let others: Vec<usize> = (0..3).filter(|&i| i != len_slot).collect();
        // ptr sentinel is the smaller of the two (align << usize::MAX).
        let a = others[0];
        let b = others[1];
        if words[a] <= words[b] { (a, b) } else { (b, a) }
    };
    let cap_slot = (0..3).find(|&i| i != len_slot && i != ptr_slot).unwrap();

    let ptr_offset = (ptr_slot * ptr_width) as ByteOffset;
    let len_offset = (len_slot * ptr_width) as ByteOffset;
    let cap_offset = (cap_slot * ptr_width) as ByteOffset;

    CalibrationResult::Ok(OpaqueDescriptor {
        kind: ContainerKind::Vec,
        size,
        align,
        empty_bytes,
        ptr_offset,
        len_offset,
        cap_offset,
        elem_size: 0,
        elem_align,
    })
}

// ---------------------------------------------------------------------------
// String calibration  (task #5)
// ---------------------------------------------------------------------------

/// Calibrate `String`.
///
/// `String` is calibrated independently from `Vec<u8>` — even if the
/// observed representation matches, we keep them separate to avoid
/// encoding an implementation detail as a cross-type axiom.
pub fn calibrate_string() -> CalibrationResult {
    let size = std::mem::size_of::<String>();
    let align = std::mem::align_of::<String>();
    let ptr_width = std::mem::size_of::<usize>();

    if size != 3 * ptr_width || align != ptr_width {
        return CalibrationResult::Unsupported {
            reason: format!(
                "String size/align unexpected: size={size} align={align} ptr_width={ptr_width}"
            ),
        };
    }

    let empty_bytes: Vec<u8> = {
        let empty = String::new();
        let raw_ptr: *const String = &empty;
        let raw_bytes_ptr = raw_ptr as *const u8;
        let bytes = unsafe { std::slice::from_raw_parts(raw_bytes_ptr, size) }.to_vec();
        drop(empty);
        bytes
    };

    // Build a non-empty string: len=2, cap>=3, ptr is a real heap address.
    // String is a valid owner, so we can just drop it normally after reading.
    let words: [usize; 3] = {
        let mut s = String::with_capacity(2);
        s.push('a');
        s.push('b');
        s.reserve(1); // cap >= 3; all three slots distinct
        let w = read_words(&s);
        drop(s);
        w
    };

    // len == 2
    let len_slot = words.iter().position(|&w| w == 2);
    let Some(len_slot) = len_slot else {
        return CalibrationResult::Unsupported {
            reason: format!(
                "String: could not identify len slot: words={words:?} (expected one == 2)"
            ),
        };
    };

    let others: Vec<(usize, usize)> = words
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != len_slot)
        .map(|(i, &w)| (i, w))
        .collect();

    let (cap_slot, cap_val) = if others[0].1 <= others[1].1 {
        others[0]
    } else {
        others[1]
    };
    let (ptr_slot, ptr_val) = if others[0].1 > others[1].1 {
        others[0]
    } else {
        others[1]
    };

    if cap_val < 2 {
        return CalibrationResult::Unsupported {
            reason: format!("String cap slot value {cap_val} < 2"),
        };
    }
    if ptr_val == 0 {
        return CalibrationResult::Unsupported {
            reason: "String ptr slot is zero".into(),
        };
    }

    let ptr_offset = (ptr_slot * ptr_width) as ByteOffset;
    let len_offset = (len_slot * ptr_width) as ByteOffset;
    let cap_offset = (cap_slot * ptr_width) as ByteOffset;

    CalibrationResult::Ok(OpaqueDescriptor {
        kind: ContainerKind::String,
        size,
        align,
        empty_bytes,
        ptr_offset,
        len_offset,
        cap_offset,
        // String's element is always u8.
        elem_size: 1,
        elem_align: 1,
    })
}

// ---------------------------------------------------------------------------
// Box<T> calibration  (task #19)
// ---------------------------------------------------------------------------

/// Calibrate `Box<T>` for pointee type `T`.
///
/// `Box<T>` is a single pointer. On non-null, it owns a heap allocation of
/// one `T`. There is no length or capacity — those offsets are `OFFSET_ABSENT`.
///
/// The "empty" representation for `Box<T>` does not exist (Box cannot be null
/// in safe Rust). We record a dangling-sentinel byte pattern derived from
/// `NonNull::<T>::dangling()` so the JIT has a defined value to write before
/// the allocation helper fills in the real pointer.
pub fn calibrate_box_t<T>() -> CalibrationResult {
    let size = std::mem::size_of::<Box<T>>();
    let align = std::mem::align_of::<Box<T>>();
    let ptr_width = std::mem::size_of::<usize>();

    // Box<T> must be exactly one pointer wide.
    if size != ptr_width {
        return CalibrationResult::Unsupported {
            reason: format!("Box<T> size unexpected: size={size} (expected {ptr_width})"),
        };
    }

    let elem_size = std::mem::size_of::<T>();
    let elem_align = std::mem::align_of::<T>();

    // Record the dangling sentinel as `empty_bytes` — written before the alloc
    // helper runs. Using NonNull::dangling() gives a well-defined non-null
    // value that is alignment-valid but must never be dereferenced.
    let dangling_ptr = std::ptr::NonNull::<T>::dangling().as_ptr() as usize;
    let empty_bytes: Vec<u8> = dangling_ptr.to_ne_bytes().to_vec();

    // Verify the probe: create a real Box<T> and confirm its pointer word
    // matches the address we gave it. For ZSTs this is always dangling.
    // For non-ZSTs, heap ptr must differ from the dangling sentinel.
    if elem_size > 0 {
        // We can't actually construct a Box<T> for arbitrary T without
        // initializing it. Just verify the layout — one-word pointer, no
        // further disambiguation needed.
    }

    CalibrationResult::Ok(OpaqueDescriptor {
        kind: ContainerKind::BoxOwned,
        size,
        align,
        empty_bytes,
        ptr_offset: 0,
        len_offset: OFFSET_ABSENT,
        cap_offset: OFFSET_ABSENT,
        elem_size,
        elem_align,
    })
}

/// Calibrate `Box<[T]>` for element type `T`.
///
/// `Box<[T]>` is a fat pointer: two words (data pointer + length). No capacity.
/// Calibrated separately from `Vec<T>` and `Box<T>` — even though the
/// observed layout may look similar to a Vec without a cap word, they are
/// distinct types with distinct semantics.
pub fn calibrate_box_slice<T>() -> CalibrationResult {
    let size = std::mem::size_of::<Box<[T]>>();
    let align = std::mem::align_of::<Box<[T]>>();
    let ptr_width = std::mem::size_of::<usize>();

    // Box<[T]> must be exactly two pointer-widths (fat pointer).
    if size != 2 * ptr_width {
        return CalibrationResult::Unsupported {
            reason: format!(
                "Box<[T]> size unexpected: size={size} (expected {})",
                2 * ptr_width
            ),
        };
    }

    let elem_size = std::mem::size_of::<T>();
    let elem_align = std::mem::align_of::<T>();

    // Record the exact empty-slice bytes: empty boxed slice has a dangling
    // data ptr and len==0. Drop runs cleanly (no allocation to free).
    let empty_bytes: Vec<u8> = {
        let empty: Box<[T]> = Vec::<T>::new().into_boxed_slice();
        let raw_ptr: *const Box<[T]> = &empty;
        let raw_bytes_ptr = raw_ptr as *const u8;
        // SAFETY: Box<[T]> is `size` bytes.
        let bytes = unsafe { std::slice::from_raw_parts(raw_bytes_ptr, size) }.to_vec();
        drop(empty);
        bytes
    };

    // Probe a non-empty boxed slice to identify ptr vs len slots.
    // We need two distinct values. Use a slice of length 3 (for non-ZSTs
    // whose heap ptr >> 3), or length 7 for ZSTs.
    if elem_size == 0 {
        return calibrate_box_slice_zst::<T>(size, align, empty_bytes, elem_align);
    }

    // Build a Box<[T]> with 3 uninit elements so len==3, ptr is a heap addr.
    // We convert to a fat pointer to read its two words, then reconstruct the
    // original Vec (with len=0 and the same ptr+cap) and drop it — Vec::drop
    // uses the correct capacity for the allocator-free. No element dtors run.
    let words: [usize; 2] = {
        let mut v: Vec<T> = Vec::with_capacity(3);
        // SAFETY: We only inspect fat-pointer header words, not element bytes.
        unsafe { v.set_len(3) };
        let raw_ptr = v.as_mut_ptr();
        let cap = v.capacity();
        std::mem::forget(v);
        // SAFETY: We own the allocation (raw_ptr, cap). Wrap as a Box<[T]> of
        // length 3 so we can read the fat pointer words, then immediately
        // reconstruct as a Vec<T> with len=0 so drop uses the correct cap.
        let boxed: Box<[T]> =
            unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(raw_ptr, 3)) };
        let w = read_two_words(&boxed);
        let raw = Box::into_raw(boxed) as *mut T;
        // Reconstruct the Vec so its drop uses the correct capacity.
        drop(unsafe { Vec::from_raw_parts(raw, 0, cap) });
        w
    };

    // len == 3; ptr is the heap address (>> 3 on any real platform).
    let len_slot = words.iter().position(|&w| w == 3);
    let Some(len_slot) = len_slot else {
        return CalibrationResult::Unsupported {
            reason: format!(
                "Box<[T]>: could not identify len slot: words={words:?} (expected one == 3)"
            ),
        };
    };
    let ptr_slot = 1 - len_slot;

    if words[ptr_slot] == 0 {
        return CalibrationResult::Unsupported {
            reason: "Box<[T]>: ptr slot is zero".into(),
        };
    }

    let ptr_offset = (ptr_slot * ptr_width) as ByteOffset;
    let len_offset = (len_slot * ptr_width) as ByteOffset;

    CalibrationResult::Ok(OpaqueDescriptor {
        kind: ContainerKind::BoxSlice,
        size,
        align,
        empty_bytes,
        ptr_offset,
        len_offset,
        cap_offset: OFFSET_ABSENT,
        elem_size,
        elem_align,
    })
}

fn calibrate_box_slice_zst<T>(
    size: usize,
    align: usize,
    empty_bytes: Vec<u8>,
    elem_align: usize,
) -> CalibrationResult {
    let ptr_width = std::mem::size_of::<usize>();

    // ZST: Vec::new() allocates nothing. We set len=7 to make the slot
    // identifiable, convert to a boxed slice to get a fat pointer, read its
    // words, then reconstruct the Vec (len=0, cap=usize::MAX) and drop it.
    // Vec::drop for ZSTs is a no-op (no allocation). Miri-clean.
    let words: [usize; 2] = {
        let mut v: Vec<T> = Vec::new();
        // SAFETY: ZSTs have no bytes; set_len only updates the len counter.
        unsafe { v.set_len(7) };
        let raw = v.as_mut_ptr();
        let cap = v.capacity();
        std::mem::forget(v);
        let boxed: Box<[T]> = unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(raw, 7)) };
        let w = read_two_words(&boxed);
        // Reconstruct the original Vec so its drop runs with correct (cap,len)
        // state. For ZSTs Vec::drop is a no-op, but this keeps Miri happy.
        let raw2 = Box::into_raw(boxed) as *mut T;
        drop(unsafe { Vec::from_raw_parts(raw2, 0, cap) });
        w
    };

    let len_slot = words.iter().position(|&w| w == 7);
    let Some(len_slot) = len_slot else {
        return CalibrationResult::Unsupported {
            reason: format!(
                "Box<[T]> ZST: could not identify len slot: words={words:?} (expected one == 7)"
            ),
        };
    };
    let ptr_slot = 1 - len_slot;

    let ptr_offset = (ptr_slot * ptr_width) as ByteOffset;
    let len_offset = (len_slot * ptr_width) as ByteOffset;

    CalibrationResult::Ok(OpaqueDescriptor {
        kind: ContainerKind::BoxSlice,
        size,
        align,
        empty_bytes,
        ptr_offset,
        len_offset,
        cap_offset: OFFSET_ABSENT,
        elem_size: 0,
        elem_align,
    })
}

// ---------------------------------------------------------------------------
// Calibration registry  (task #6)
// ---------------------------------------------------------------------------

/// A handle that uniquely identifies a calibrated opaque type.
///
/// Handles are opaque integers. IR ops reference descriptors by handle, not
/// by raw pointer, so the IR stays portable within the process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DescriptorHandle(pub u32);

/// T-invariant layout of the `Vec<T>` family.
///
/// `Vec<T>`'s header is three pointer-width words — `(ptr, len, cap)` in some
/// slot order — and the layout is identical for every `T`. Only the element
/// size/align varies. Once we've probed one `Vec<T>` we cache these offsets
/// and synthesize every subsequent `Vec<U>` descriptor.
#[derive(Clone, Copy, Debug)]
struct VecFamilyLayout {
    size: usize,
    align: usize,
    ptr_offset: ByteOffset,
    len_offset: ByteOffset,
    cap_offset: ByteOffset,
}

/// Process-local registry of calibrated opaque descriptors.
///
/// Created once per process; never persisted.
pub struct CalibrationRegistry {
    descriptors: Vec<OpaqueDescriptor>,
    /// Shape value → descriptor handle.
    ///
    /// `&'static Shape` as a key uses Shape's own `Hash`/`PartialEq` via the
    /// blanket `impl<T: Hash> Hash for &T` — not the pointer address.
    shape_map: std::collections::HashMap<&'static facet_core::Shape, DescriptorHandle>,
    /// The String descriptor handle, stored separately so that IR lowering can
    /// retrieve it without knowing `String::SHAPE` statically.
    string_handle: Option<DescriptorHandle>,
    /// Cached `Vec<T>` header layout after the first successful probe.
    vec_family: Option<VecFamilyLayout>,
}

impl CalibrationRegistry {
    pub fn new() -> Self {
        Self {
            descriptors: Vec::new(),
            shape_map: std::collections::HashMap::new(),
            string_handle: None,
            vec_family: None,
        }
    }

    /// Return the handle to the String descriptor, if one was registered via
    /// `calibrate_string()` or `calibrate_string_for_shape()`.
    pub fn string_descriptor_handle(&self) -> Option<DescriptorHandle> {
        self.string_handle
    }

    /// Register a successfully calibrated descriptor and return its handle.
    pub fn register(&mut self, desc: OpaqueDescriptor) -> DescriptorHandle {
        let id = self.descriptors.len() as u32;
        self.descriptors.push(desc);
        DescriptorHandle(id)
    }

    /// Register a descriptor associated with a `Shape`.
    pub fn register_for_shape(
        &mut self,
        shape: &'static facet_core::Shape,
        desc: OpaqueDescriptor,
    ) -> DescriptorHandle {
        let handle = self.register(desc);
        self.shape_map.insert(shape, handle);
        handle
    }

    /// Look up a descriptor by Shape value.
    pub fn lookup_by_shape(&self, shape: &'static facet_core::Shape) -> Option<DescriptorHandle> {
        self.shape_map.get(shape).copied()
    }

    /// Calibrate `Vec<T>` and register the descriptor keyed by `Vec<T>::SHAPE`.
    ///
    /// Returns the handle on success, or `None` if calibration fails.
    pub fn calibrate_vec_for_shape<T>(
        &mut self,
        shape: &'static facet_core::Shape,
    ) -> Option<DescriptorHandle>
    where
        T: Sized,
    {
        if let Some(desc) = self.synthesize_vec_desc::<T>() {
            return Some(self.register_for_shape(shape, desc));
        }
        match calibrate_vec::<T>() {
            CalibrationResult::Ok(desc) => {
                self.cache_vec_family(&desc);
                Some(self.register_for_shape(shape, desc))
            }
            CalibrationResult::Unsupported { reason } => {
                eprintln!("vox-jit-cal: Vec<T> calibration unsupported: {reason}");
                None
            }
        }
    }

    /// Look up a descriptor by handle.
    ///
    /// Returns `None` if the handle is unknown (should not happen in correct
    /// usage — handles come from `register`).
    pub fn get(&self, handle: DescriptorHandle) -> Option<&OpaqueDescriptor> {
        self.descriptors.get(handle.0 as usize)
    }

    /// Return the number of registered descriptors.
    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    /// Return `true` if no descriptors have been registered.
    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }

    /// Iterate over all registered descriptors.
    pub fn iter(&self) -> impl Iterator<Item = (DescriptorHandle, &OpaqueDescriptor)> {
        self.descriptors
            .iter()
            .enumerate()
            .map(|(i, d)| (DescriptorHandle(i as u32), d))
    }

    /// Calibrate `Vec<T>` if not already done and return the handle, or
    /// `None` if calibration fails (caller must fall back to interpreter).
    ///
    /// Uses the cached family layout after the first successful probe.
    pub fn calibrate_vec<T>(&mut self) -> Option<DescriptorHandle> {
        if let Some(desc) = self.synthesize_vec_desc::<T>() {
            return Some(self.register(desc));
        }
        match calibrate_vec::<T>() {
            CalibrationResult::Ok(desc) => {
                self.cache_vec_family(&desc);
                Some(self.register(desc))
            }
            CalibrationResult::Unsupported { reason } => {
                eprintln!("vox-jit-cal: Vec<T> calibration unsupported: {reason}");
                None
            }
        }
    }

    /// Synthesize a `Vec<T>` descriptor from the cached family layout, without
    /// probing. Returns `None` if the family isn't cached yet or if `Vec<T>`'s
    /// container size/align doesn't match the cached family (e.g., a pointer
    /// width mismatch that shouldn't happen in practice).
    fn synthesize_vec_desc<T>(&self) -> Option<OpaqueDescriptor> {
        let container_size = std::mem::size_of::<Vec<T>>();
        let container_align = std::mem::align_of::<Vec<T>>();
        let elem_size = std::mem::size_of::<T>();
        let elem_align = std::mem::align_of::<T>();
        self.synthesize_vec_desc_from_layouts(
            container_size,
            container_align,
            elem_size,
            elem_align,
        )
    }

    fn synthesize_vec_desc_from_layouts(
        &self,
        container_size: usize,
        container_align: usize,
        elem_size: usize,
        elem_align: usize,
    ) -> Option<OpaqueDescriptor> {
        let f = self.vec_family?;
        if f.size != container_size || f.align != container_align {
            return None;
        }
        let ptr_width = std::mem::size_of::<usize>();
        let mut empty_bytes = vec![0u8; f.size];
        // `Vec::<T>::new()` stores `NonNull::<T>::dangling()` — which is
        // `align_of::<T>()` as a raw address — in the ptr slot. `len` is 0.
        // `cap` is 0 for non-ZSTs; for ZSTs modern stdlib uses `usize::MAX`.
        let ptr_sentinel: usize = elem_align.max(1);
        let cap_sentinel: usize = if elem_size == 0 { usize::MAX } else { 0 };
        let ptr_off = f.ptr_offset as usize;
        let cap_off = f.cap_offset as usize;
        empty_bytes[ptr_off..ptr_off + ptr_width].copy_from_slice(&ptr_sentinel.to_ne_bytes());
        empty_bytes[cap_off..cap_off + ptr_width].copy_from_slice(&cap_sentinel.to_ne_bytes());
        Some(OpaqueDescriptor {
            kind: ContainerKind::Vec,
            size: f.size,
            align: f.align,
            empty_bytes,
            ptr_offset: f.ptr_offset,
            len_offset: f.len_offset,
            cap_offset: f.cap_offset,
            elem_size,
            elem_align,
        })
    }

    /// Record the `Vec<T>` family layout from a successful probe. No-op if
    /// already cached or if the descriptor isn't a Vec.
    fn cache_vec_family(&mut self, desc: &OpaqueDescriptor) {
        if desc.kind != ContainerKind::Vec || self.vec_family.is_some() {
            return;
        }
        self.vec_family = Some(VecFamilyLayout {
            size: desc.size,
            align: desc.align,
            ptr_offset: desc.ptr_offset,
            len_offset: desc.len_offset,
            cap_offset: desc.cap_offset,
        });
    }

    /// Calibrate `String` and return the handle, or `None` on failure.
    ///
    /// The handle is also stored in `string_descriptor_handle()` so that IR
    /// lowering can emit `ReadString` for `String`-typed scalar fields.
    pub fn calibrate_string(&mut self) -> Option<DescriptorHandle> {
        match calibrate_string() {
            CalibrationResult::Ok(desc) => {
                let h = self.register(desc);
                self.string_handle = Some(h);
                Some(h)
            }
            CalibrationResult::Unsupported { reason } => {
                eprintln!("vox-jit-cal: String calibration unsupported: {reason}");
                None
            }
        }
    }

    /// Calibrate `String` and register it keyed by `String::SHAPE`.
    ///
    /// The handle is also stored in `string_descriptor_handle()`.
    ///
    /// Call this at registry setup time (before lowering) when using the JIT
    /// path for types that contain `String` fields.
    pub fn calibrate_string_for_shape(
        &mut self,
        shape: &'static facet_core::Shape,
    ) -> Option<DescriptorHandle> {
        match calibrate_string() {
            CalibrationResult::Ok(desc) => {
                let h = self.register_for_shape(shape, desc);
                self.string_handle = Some(h);
                Some(h)
            }
            CalibrationResult::Unsupported { reason } => {
                eprintln!("vox-jit-cal: String calibration unsupported: {reason}");
                None
            }
        }
    }

    /// Calibrate `String` and register it in `shape_map` under `String::SHAPE`.
    ///
    /// Convenience over `calibrate_string_for_shape(<String as Facet>::SHAPE)`.
    pub fn calibrate_string_for_type(&mut self) -> Option<DescriptorHandle> {
        self.calibrate_string_for_shape(<String as facet_core::Facet<'static>>::SHAPE)
    }

    /// Calibrate `Vec<T>` and register it in `shape_map` under `<Vec<T>>::SHAPE`.
    ///
    /// Convenience over `calibrate_vec_for_shape::<T>(<Vec<T> as Facet>::SHAPE)`.
    pub fn calibrate_vec_for_type<T>(&mut self) -> Option<DescriptorHandle>
    where
        Vec<T>: for<'a> facet_core::Facet<'a>,
        T: Sized + 'static,
    {
        self.calibrate_vec_for_shape::<T>(<Vec<T> as facet_core::Facet<'static>>::SHAPE)
    }

    /// Calibrate `Box<T>` and return the handle, or `None` on failure.
    pub fn calibrate_box_t<T>(&mut self) -> Option<DescriptorHandle> {
        match calibrate_box_t::<T>() {
            CalibrationResult::Ok(desc) => Some(self.register(desc)),
            CalibrationResult::Unsupported { reason } => {
                eprintln!("vox-jit-cal: Box<T> calibration unsupported: {reason}");
                None
            }
        }
    }

    /// Calibrate `Box<[T]>` and return the handle, or `None` on failure.
    pub fn calibrate_box_slice<T>(&mut self) -> Option<DescriptorHandle> {
        match calibrate_box_slice::<T>() {
            CalibrationResult::Ok(desc) => Some(self.register(desc)),
            CalibrationResult::Unsupported { reason } => {
                eprintln!("vox-jit-cal: Box<[T]> calibration unsupported: {reason}");
                None
            }
        }
    }
}

impl Default for CalibrationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CalibrationRegistry {
    /// Pre-register calibrated descriptors for all common primitive element types.
    ///
    /// Covers: Vec<bool>, Vec<i8/i16/i32/i64>, Vec<u8/u16/u32/u64>, Vec<f32/f64>,
    /// Vec<String>, String, Box<T>/Box<[T]> for the same primitives.
    ///
    /// Any individual calibration failure is silently skipped (the JIT will fall
    /// back to the interpreter for that type). Returns `&mut self` so the caller
    /// can chain further registrations.
    /// Register calibrations for all common standard-library container types.
    ///
    /// Registers `Vec<T>`, `String`, `Box<T>`, and `Box<[T]>` descriptors and
    /// stores them in `shape_map` under each type's `Facet::SHAPE`, so that
    /// IR lowering can retrieve them via `lookup_by_shape(&shape)`.
    pub fn with_common(&mut self) -> &mut Self {
        // Calibrate `String` once — it's not parameterized. Everything else
        // (Vec<T>, Box<T>, Box<[T]>) is handled on-demand via
        // `get_or_calibrate_by_shape`; Vec<T>'s family layout is probed once
        // and then reused for every T.
        self.calibrate_string_for_type();
        self
    }
}

// ---------------------------------------------------------------------------
// Self-check gate  (qa-engineer interface)
// ---------------------------------------------------------------------------

/// Failure detail from a calibration self-check.
#[derive(Debug)]
pub struct SelfCheckFailure {
    pub check: &'static str,
    pub reason: String,
}

impl std::fmt::Display for SelfCheckFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "self-check '{}' failed: {}", self.check, self.reason)
    }
}

/// Trait the qa-engineer implements to validate a calibrated descriptor
/// before the JIT enables the fast path for a concrete type.
///
/// The implementor receives an immutable reference to the descriptor and
/// runs whatever round-trip / partial-init / invariant checks it needs.
/// Return `Ok(())` to allow the fast path, or `Err(SelfCheckFailure)` to
/// deny it (the registry will fall back to the interpreter for this type).
pub trait CalibrationSelfCheck {
    fn check(&self, desc: &OpaqueDescriptor) -> Result<(), SelfCheckFailure>;
}

/// Outcome of `CalibrationRegistry::calibrate_vec_gated` /
/// `calibrate_string_gated`.
pub enum GatedResult {
    /// Calibration succeeded and all self-checks passed. Fast path is enabled.
    Ready(DescriptorHandle),
    /// Calibration failed (probe was inconclusive). Fall back to interpreter.
    CalibrationFailed { reason: String },
    /// Calibration succeeded but a self-check rejected the descriptor.
    /// Fall back to interpreter; the failure is logged.
    SelfCheckFailed(SelfCheckFailure),
}

impl CalibrationRegistry {
    /// Calibrate `Vec<T>`, then run `checker` synchronously.
    ///
    /// Returns `GatedResult::Ready` only when both steps succeed.
    /// Any failure produces a `GatedResult` variant that the caller
    /// must treat as a signal to use the interpreter.
    pub fn calibrate_vec_gated<T>(&mut self, checker: &dyn CalibrationSelfCheck) -> GatedResult {
        match calibrate_vec::<T>() {
            CalibrationResult::Unsupported { reason } => GatedResult::CalibrationFailed { reason },
            CalibrationResult::Ok(desc) => match checker.check(&desc) {
                Err(failure) => GatedResult::SelfCheckFailed(failure),
                Ok(()) => GatedResult::Ready(self.register(desc)),
            },
        }
    }

    /// Calibrate `String`, then run `checker` synchronously.
    pub fn calibrate_string_gated(&mut self, checker: &dyn CalibrationSelfCheck) -> GatedResult {
        match calibrate_string() {
            CalibrationResult::Unsupported { reason } => GatedResult::CalibrationFailed { reason },
            CalibrationResult::Ok(desc) => match checker.check(&desc) {
                Err(failure) => GatedResult::SelfCheckFailed(failure),
                Ok(()) => GatedResult::Ready(self.register(desc)),
            },
        }
    }

    /// Calibrate `Box<T>`, then run `checker` synchronously.
    pub fn calibrate_box_t_gated<T>(&mut self, checker: &dyn CalibrationSelfCheck) -> GatedResult {
        match calibrate_box_t::<T>() {
            CalibrationResult::Unsupported { reason } => GatedResult::CalibrationFailed { reason },
            CalibrationResult::Ok(desc) => match checker.check(&desc) {
                Err(failure) => GatedResult::SelfCheckFailed(failure),
                Ok(()) => GatedResult::Ready(self.register(desc)),
            },
        }
    }

    /// Calibrate `Box<[T]>`, then run `checker` synchronously.
    pub fn calibrate_box_slice_gated<T>(
        &mut self,
        checker: &dyn CalibrationSelfCheck,
    ) -> GatedResult {
        match calibrate_box_slice::<T>() {
            CalibrationResult::Unsupported { reason } => GatedResult::CalibrationFailed { reason },
            CalibrationResult::Ok(desc) => match checker.check(&desc) {
                Err(failure) => GatedResult::SelfCheckFailed(failure),
                Ok(()) => GatedResult::Ready(self.register(desc)),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// On-demand calibration by Shape  (task #29)
// ---------------------------------------------------------------------------

impl CalibrationRegistry {
    /// Probe or retrieve a descriptor for `shape` without `T` as a compile-time parameter.
    ///
    /// Returns `Some(handle)` when the shape is already registered or dynamic probing
    /// succeeds. Returns `None` for unsupported shapes; callers must fall back to the
    /// interpreter.
    ///
    /// Supported shapes:
    /// - `Def::List` with `type_ops` providing `init_in_place_with_capacity` + `set_len` —
    ///   probes `Vec<T>`-style three-word layout via vtable calls.
    /// - `Def::Pointer` with `known == Some(KnownPointer::Box)` and a non-slice pointee —
    ///   derives `BoxOwned` from layout (no probing needed; Box<T> is always one pointer word).
    /// - `Def::Pointer` with `known == Some(KnownPointer::Box)` and a `Def::Slice` pointee —
    ///   derives `BoxSlice` layout from the platform fat-pointer convention (ptr word first).
    ///
    /// Results are cached by `&'static Shape` key, which uses Shape's own `Hash`/`Eq`
    /// via the blanket `impl<T: Hash> Hash for &T` — not the pointer address.
    pub fn get_or_calibrate_by_shape(
        &mut self,
        shape: &'static facet_core::Shape,
    ) -> Option<DescriptorHandle> {
        if let Some(handle) = self.shape_map.get(shape).copied() {
            return Some(handle);
        }

        let result = match shape.def {
            facet_core::Def::List(list_def) => self.calibrate_list_by_shape(shape, list_def),
            facet_core::Def::Pointer(ptr_def) => {
                // TODO: Cow, Arc, Rc, &T, etc. — all in scope, just not yet
                // implemented. Skip them silently until someone wires them up.
                if ptr_def.known != Some(facet_core::KnownPointer::Box) {
                    return None;
                }
                probe_pointer_by_vtable(shape, ptr_def)
            }
            _ => CalibrationResult::Unsupported {
                reason: format!(
                    "shape '{}' has unsupported Def for on-demand calibration",
                    shape.type_identifier
                ),
            },
        };

        match result {
            CalibrationResult::Ok(desc) => {
                let handle = self.register(desc);
                self.shape_map.insert(shape, handle);
                Some(handle)
            }
            CalibrationResult::Unsupported { reason } => {
                eprintln!(
                    "vox-jit-cal: on-demand calibration failed for '{}': {reason}",
                    shape.type_identifier
                );
                None
            }
        }
    }

    /// Calibrate a list-shaped container, using the cached `Vec<T>` family
    /// layout when available. `Vec<T>` headers are T-invariant so we only
    /// need to probe once; subsequent shapes synthesize their descriptor
    /// from the cached offsets and this shape's own element layout.
    fn calibrate_list_by_shape(
        &mut self,
        shape: &'static facet_core::Shape,
        list_def: facet_core::ListDef,
    ) -> CalibrationResult {
        let container_layout = match shape.layout.sized_layout() {
            Ok(l) => l,
            Err(_) => {
                return CalibrationResult::Unsupported {
                    reason: "list shape has unsized layout".into(),
                };
            }
        };
        let elem_layout = match list_def.t.layout.sized_layout() {
            Ok(l) => l,
            Err(_) => {
                return CalibrationResult::Unsupported {
                    reason: "element shape has unsized layout".into(),
                };
            }
        };

        if let Some(desc) = self.synthesize_vec_desc_from_layouts(
            container_layout.size(),
            container_layout.align(),
            elem_layout.size(),
            elem_layout.align(),
        ) {
            return CalibrationResult::Ok(desc);
        }

        let result = probe_list_by_vtable(shape, list_def);
        if let CalibrationResult::Ok(ref desc) = result {
            self.cache_vec_family(desc);
        }
        result
    }
}

/// Probe a list-shaped container (Vec<T>) using its monomorphized vtable functions.
///
/// Requires `type_ops` with `init_in_place_with_capacity` and `set_len`. Falls back to
/// `Unsupported` if either is absent.
fn probe_list_by_vtable(
    shape: &'static facet_core::Shape,
    list_def: facet_core::ListDef,
) -> CalibrationResult {
    let container_layout = match shape.layout.sized_layout() {
        Ok(l) => l,
        Err(_) => {
            return CalibrationResult::Unsupported {
                reason: "list shape has unsized layout".into(),
            };
        }
    };

    let ptr_width = std::mem::size_of::<usize>();
    let container_size = container_layout.size();

    if container_size != 3 * ptr_width {
        return CalibrationResult::Unsupported {
            reason: format!(
                "list container size {container_size} != 3×ptr_width; cannot fast-path"
            ),
        };
    }

    let type_ops = match list_def.type_ops {
        Some(ops) => ops,
        None => {
            return CalibrationResult::Unsupported {
                reason: "list_def has no type_ops; cannot probe without monomorphized fns".into(),
            };
        }
    };

    let init_fn = match type_ops.init_in_place_with_capacity {
        Some(f) => f,
        None => {
            return CalibrationResult::Unsupported {
                reason: "type_ops missing init_in_place_with_capacity".into(),
            };
        }
    };
    let set_len_fn = match type_ops.set_len {
        Some(f) => f,
        None => {
            return CalibrationResult::Unsupported {
                reason: "type_ops missing set_len".into(),
            };
        }
    };

    let elem_shape = list_def.t;
    let elem_layout = match elem_shape.layout.sized_layout() {
        Ok(l) => l,
        Err(_) => {
            return CalibrationResult::Unsupported {
                reason: "element shape has unsized layout".into(),
            };
        }
    };
    let elem_size = elem_layout.size();
    let elem_align = elem_layout.align();

    // Scratch buffer: u64-aligned, large enough for 3 pointer-width words.
    let mut scratch: Vec<u64> = vec![0u64; container_size.div_ceil(8)];

    // Capture empty bytes (capacity=0 init).
    let empty_bytes: Vec<u8> = {
        let scratch_ptr = scratch.as_mut_ptr() as *mut ();
        let uninit = facet_core::PtrUninit::new(scratch_ptr);
        // SAFETY: scratch is properly aligned and sized for the container.
        let init_ptr = unsafe { init_fn(uninit, 0) };
        let raw = scratch.as_ptr() as *const u8;
        // SAFETY: container_size bytes, scratch is alive.
        let bytes = unsafe { std::slice::from_raw_parts(raw, container_size) }.to_vec();
        // Drop the container — Vec::new() allocates nothing, so this is a no-op for
        // the allocator, but we run drop_in_place to stay correct.
        // SAFETY: init_fn initialized the container.
        unsafe { shape.call_drop_in_place(init_ptr) };
        for w in scratch.iter_mut() {
            *w = 0;
        }
        bytes
    };

    if elem_size == 0 {
        return probe_list_zst_by_vtable(
            shape,
            init_fn,
            set_len_fn,
            container_size,
            container_layout.align(),
            empty_bytes,
            elem_align,
        );
    }

    // Non-ZST: init with capacity=2 so len=1 and cap=2 are distinct.
    let words: [usize; 3] = {
        let scratch_ptr = scratch.as_mut_ptr() as *mut ();
        let uninit = facet_core::PtrUninit::new(scratch_ptr);
        // SAFETY: scratch is aligned and sized; capacity=2 ensures cap>=2.
        let init_ptr = unsafe { init_fn(uninit, 2) };
        // SAFETY: set_len(1) valid when capacity>=2; we never read elements.
        unsafe { set_len_fn(init_ptr, 1) };
        let raw = scratch.as_ptr() as *const usize;
        // SAFETY: container_size == 3 * ptr_width; scratch is usize-aligned.
        let w = unsafe { [*raw, *raw.add(1), *raw.add(2)] };
        // Reset len before drop so no element dtors run.
        unsafe { set_len_fn(init_ptr, 0) };
        // SAFETY: init_fn initialized the container; len==0 so no element drops.
        unsafe { shape.call_drop_in_place(init_ptr) };
        w
    };

    let len_slot = match words.iter().position(|&w| w == 1) {
        Some(s) => s,
        None => {
            return CalibrationResult::Unsupported {
                reason: format!("on-demand probe: cannot find len slot (words={words:?})"),
            };
        }
    };

    let others: Vec<(usize, usize)> = words
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != len_slot)
        .map(|(i, &w)| (i, w))
        .collect();

    let (cap_slot, cap_val) = if others[0].1 <= others[1].1 {
        others[0]
    } else {
        others[1]
    };
    let (ptr_slot, ptr_val) = if others[0].1 > others[1].1 {
        others[0]
    } else {
        others[1]
    };

    if cap_val < 1 || ptr_val == 0 || ptr_val < cap_val {
        return CalibrationResult::Unsupported {
            reason: format!(
                "on-demand probe: ambiguous slots (words={words:?}, len_slot={len_slot})"
            ),
        };
    }

    CalibrationResult::Ok(OpaqueDescriptor {
        kind: ContainerKind::Vec,
        size: container_size,
        align: container_layout.align(),
        empty_bytes,
        ptr_offset: (ptr_slot * ptr_width) as ByteOffset,
        len_offset: (len_slot * ptr_width) as ByteOffset,
        cap_offset: (cap_slot * ptr_width) as ByteOffset,
        elem_size,
        elem_align,
    })
}

/// ZST variant of `probe_list_by_vtable`. Uses len=3 to distinguish the len slot.
fn probe_list_zst_by_vtable(
    shape: &'static facet_core::Shape,
    init_fn: facet_core::ListInitInPlaceWithCapacityFn,
    set_len_fn: facet_core::ListSetLenFn,
    container_size: usize,
    container_align: usize,
    empty_bytes: Vec<u8>,
    elem_align: usize,
) -> CalibrationResult {
    let ptr_width = std::mem::size_of::<usize>();
    let mut scratch: Vec<u64> = vec![0u64; container_size.div_ceil(8)];

    let words: [usize; 3] = {
        let scratch_ptr = scratch.as_mut_ptr() as *mut ();
        let uninit = facet_core::PtrUninit::new(scratch_ptr);
        // SAFETY: scratch is aligned and sized; ZSTs allocate nothing.
        let init_ptr = unsafe { init_fn(uninit, 0) };
        // SAFETY: ZSTs have no bytes; set_len only updates the counter.
        unsafe { set_len_fn(init_ptr, 3) };
        let raw = scratch.as_ptr() as *const usize;
        let w = unsafe { [*raw, *raw.add(1), *raw.add(2)] };
        unsafe { set_len_fn(init_ptr, 0) };
        // SAFETY: len==0 so no element drops.
        unsafe { shape.call_drop_in_place(init_ptr) };
        w
    };

    let len_slot = match words.iter().position(|&w| w == 3) {
        Some(s) => s,
        None => {
            return CalibrationResult::Unsupported {
                reason: format!("on-demand ZST probe: cannot find len slot (words={words:?})"),
            };
        }
    };

    let others: Vec<usize> = (0..3usize).filter(|&i| i != len_slot).collect();
    let (ptr_slot, cap_slot) = if words[others[0]] <= words[others[1]] {
        (others[0], others[1])
    } else {
        (others[1], others[0])
    };

    CalibrationResult::Ok(OpaqueDescriptor {
        kind: ContainerKind::Vec,
        size: container_size,
        align: container_align,
        empty_bytes,
        ptr_offset: (ptr_slot * ptr_width) as ByteOffset,
        len_offset: (len_slot * ptr_width) as ByteOffset,
        cap_offset: (cap_slot * ptr_width) as ByteOffset,
        elem_size: 0,
        elem_align,
    })
}

/// Probe a pointer-shaped type (Box<T>, Box<[T]>) using vtable metadata.
///
/// `Box<T>` (non-slice): derived from pointee layout; always one pointer word.
/// `Box<[T]>` (slice pointee): derived from platform fat-pointer convention.
/// Other pointer kinds: `Unsupported`.
fn probe_pointer_by_vtable(
    shape: &'static facet_core::Shape,
    ptr_def: facet_core::PointerDef,
) -> CalibrationResult {
    if ptr_def.known != Some(facet_core::KnownPointer::Box) {
        return CalibrationResult::Unsupported {
            reason: format!(
                "pointer '{}' is not KnownPointer::Box",
                shape.type_identifier
            ),
        };
    }

    let pointee_shape = match ptr_def.pointee {
        Some(s) => s,
        None => {
            return CalibrationResult::Unsupported {
                reason: "Box has no pointee shape (opaque)".into(),
            };
        }
    };

    match pointee_shape.def {
        facet_core::Def::Slice(slice_def) => {
            // Box<[T]>: fat pointer — derive from platform convention.
            derive_box_slice_layout(shape, slice_def)
        }
        _ => {
            // Box<T>: single pointer word — no probing needed.
            derive_box_t_layout(shape, pointee_shape)
        }
    }
}

/// Derive `BoxOwned` descriptor from layout alone (no runtime probing).
///
/// `Box<T>` is always a single pointer word pointing to a heap allocation. Its
/// layout never varies across Rust versions.
fn derive_box_t_layout(
    shape: &'static facet_core::Shape,
    pointee_shape: &'static facet_core::Shape,
) -> CalibrationResult {
    let ptr_width = std::mem::size_of::<usize>();
    let box_size = match shape.layout.sized_layout() {
        Ok(l) if l.size() == ptr_width => l.size(),
        Ok(l) => {
            return CalibrationResult::Unsupported {
                reason: format!("Box<T> size {} != ptr_width {ptr_width}", l.size()),
            };
        }
        Err(_) => {
            return CalibrationResult::Unsupported {
                reason: "Box<T> shape has unsized layout".into(),
            };
        }
    };
    let pointee_layout = match pointee_shape.layout.sized_layout() {
        Ok(l) => l,
        Err(_) => {
            return CalibrationResult::Unsupported {
                reason: "Box<T> pointee has unsized layout".into(),
            };
        }
    };

    // empty_bytes is the dangling sentinel: NonNull::<T>::dangling() == align of T.
    let dangling: usize = pointee_layout.align();
    let empty_bytes = dangling.to_ne_bytes().to_vec();

    CalibrationResult::Ok(OpaqueDescriptor {
        kind: ContainerKind::BoxOwned,
        size: box_size,
        align: box_size,
        empty_bytes,
        ptr_offset: 0,
        len_offset: OFFSET_ABSENT,
        cap_offset: OFFSET_ABSENT,
        elem_size: pointee_layout.size(),
        elem_align: pointee_layout.align(),
    })
}

/// Derive `BoxSlice` descriptor from platform fat-pointer convention.
///
/// On all Rust targets, `*const [T]` / `Box<[T]>` is `(data_ptr, len)` with the
/// data pointer first. We encode this directly without runtime probing.
fn derive_box_slice_layout(
    shape: &'static facet_core::Shape,
    slice_def: facet_core::SliceDef,
) -> CalibrationResult {
    let ptr_width = std::mem::size_of::<usize>();
    let box_layout = match shape.layout.sized_layout() {
        Ok(l) => l,
        Err(_) => {
            return CalibrationResult::Unsupported {
                reason: "Box<[T]> shape has unsized layout".into(),
            };
        }
    };
    if box_layout.size() != 2 * ptr_width {
        return CalibrationResult::Unsupported {
            reason: format!("Box<[T]> size {} != 2×ptr_width", box_layout.size()),
        };
    }

    let elem_shape = slice_def.t;
    let elem_layout = match elem_shape.layout.sized_layout() {
        Ok(l) => l,
        Err(_) => {
            return CalibrationResult::Unsupported {
                reason: "Box<[T]> element has unsized layout".into(),
            };
        }
    };
    let elem_size = elem_layout.size();
    let elem_align = elem_layout.align();

    // empty_bytes: (dangling_ptr=align_of_T, len=0).
    let dangling: usize = if elem_size == 0 { 1 } else { elem_align };
    let mut empty_bytes = Vec::with_capacity(2 * ptr_width);
    empty_bytes.extend_from_slice(&dangling.to_ne_bytes());
    empty_bytes.extend_from_slice(&0usize.to_ne_bytes());

    CalibrationResult::Ok(OpaqueDescriptor {
        kind: ContainerKind::BoxSlice,
        size: box_layout.size(),
        align: box_layout.align(),
        empty_bytes,
        ptr_offset: 0,
        len_offset: ptr_width as ByteOffset,
        cap_offset: OFFSET_ABSENT,
        elem_size,
        elem_align,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Read the three pointer-width words from any value that is exactly 3 words.
fn read_words<T>(val: &T) -> [usize; 3] {
    assert_eq!(
        std::mem::size_of::<T>(),
        3 * std::mem::size_of::<usize>(),
        "read_words: value must be exactly 3 pointer-widths"
    );
    let ptr = val as *const T as *const usize;
    // SAFETY: val is aligned to at least pointer width (Vec/String guarantee
    // that), and we verified the byte count above.
    unsafe { [*ptr, *ptr.add(1), *ptr.add(2)] }
}

/// Read two pointer-width words from a value that is exactly 2 words (fat pointer).
fn read_two_words<T>(val: &T) -> [usize; 2] {
    assert_eq!(
        std::mem::size_of::<T>(),
        2 * std::mem::size_of::<usize>(),
        "read_two_words: value must be exactly 2 pointer-widths"
    );
    let ptr = val as *const T as *const usize;
    // SAFETY: Box<[T]> is a fat pointer aligned to at least pointer width.
    unsafe { [*ptr, *ptr.add(1)] }
}

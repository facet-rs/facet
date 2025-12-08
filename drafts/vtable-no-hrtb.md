# Eliminating HRTB from VTable for Faster Compile Times

## Problem

Compile times increased after the vtable refactor, even though LLVM IR decreased.
The culprit: Higher-Ranked Trait Bounds (HRTB) like `for<'a>` on every vtable field.

```rust
// Current: HRTB on every field
pub struct VTable<S: VTableStyle> {
    pub display: Option<for<'a> unsafe fn(S::Receiver<'a>, &mut fmt::Formatter<'_>) -> S::Output<fmt::Result>>,
    // ... 13 more fields with for<'a>
}
```

The `for<'a>` triggers expensive trait resolution during type checking - multiplied by 14 fields, multiplied by every type that has a vtable.

## Key Insight

These are `unsafe fn` pointers. The caller is *already* responsible for lifetime safety. The `'a` in `OxRef<'a>` provides no real safety guarantee at the vtable level - it just makes the compiler work harder.

## Solution: Unlifetimed OxPtr

Instead of `OxRef<'a>` and `OxMut<'a>` in vtable signatures, use unlifetimed equivalents:

```rust
/// Unlifetimed shaped pointer for vtable use.
/// Safety is the caller's responsibility.
#[derive(Copy, Clone)]
pub struct OxPtr {
    pub ptr: Ptr,  // or *const ()
    pub shape: &'static Shape,
}

/// Unlifetimed mutable shaped pointer for vtable use.
#[derive(Copy, Clone)]
pub struct OxPtrMut {
    pub ptr: Ptr,  // or *mut ()
    pub shape: &'static Shape,
}
```

## New VTable Definitions

No more `VTableStyle` trait, no more HRTB:

```rust
/// VTable for concrete types (scalars, String, user-defined structs/enums)
pub struct VTableDirect {
    pub display: Option<unsafe fn(*const (), &mut fmt::Formatter<'_>) -> fmt::Result>,
    pub debug: Option<unsafe fn(*const (), &mut fmt::Formatter<'_>) -> fmt::Result>,
    pub hash: Option<unsafe fn(*const (), &mut HashProxy<'_>)>,
    pub drop_in_place: Option<unsafe fn(*mut ())>,
    pub invariants: Option<unsafe fn(*const ()) -> Result<(), String>>,
    pub default_in_place: Option<unsafe fn(*mut ())>,
    pub clone_into: Option<unsafe fn(*const (), *mut ())>,
    pub parse: Option<unsafe fn(&str, *mut ()) -> Result<(), ParseError>>,
    pub try_from: Option<unsafe fn(*const (), *mut ()) -> Result<(), String>>,
    pub try_into_inner: Option<unsafe fn(*mut ()) -> Result<Ptr, String>>,
    pub try_borrow_inner: Option<unsafe fn(*const ()) -> Result<Ptr, String>>,
    pub partial_eq: Option<unsafe fn(*const (), *const ()) -> bool>,
    pub partial_cmp: Option<unsafe fn(*const (), *const ()) -> Option<Ordering>>,
    pub cmp: Option<unsafe fn(*const (), *const ()) -> Ordering>,
}

/// VTable for generic containers (Vec<T>, Option<T>, Arc<T>)
/// Uses OxPtr to access inner type's shape at runtime.
pub struct VTableIndirect {
    pub display: Option<unsafe fn(OxPtr, &mut fmt::Formatter<'_>) -> Option<fmt::Result>>,
    pub debug: Option<unsafe fn(OxPtr, &mut fmt::Formatter<'_>) -> Option<fmt::Result>>,
    pub hash: Option<unsafe fn(OxPtr, &mut HashProxy<'_>) -> Option<()>>,
    pub drop_in_place: Option<unsafe fn(OxPtrMut) -> Option<()>>,
    pub invariants: Option<unsafe fn(OxPtr) -> Option<Result<(), String>>>,
    pub default_in_place: Option<unsafe fn(OxPtrMut) -> Option<()>>,
    pub clone_into: Option<unsafe fn(OxPtr, OxPtrMut) -> Option<()>>,
    pub parse: Option<unsafe fn(&str, OxPtrMut) -> Option<Result<(), ParseError>>>,
    pub try_from: Option<unsafe fn(OxPtr, OxPtrMut) -> Option<Result<(), String>>>,
    pub try_into_inner: Option<unsafe fn(OxPtrMut) -> Option<Result<Ptr, String>>>,
    pub try_borrow_inner: Option<unsafe fn(OxPtr) -> Option<Result<Ptr, String>>>,
    pub partial_eq: Option<unsafe fn(OxPtr, OxPtr) -> Option<bool>>,
    pub partial_cmp: Option<unsafe fn(OxPtr, OxPtr) -> Option<Option<Ordering>>>,
    pub cmp: Option<unsafe fn(OxPtr, OxPtr) -> Option<Ordering>>,
}
```

## Conversions

```rust
impl<'a> From<OxRef<'a>> for OxPtr {
    fn from(ox: OxRef<'a>) -> Self {
        OxPtr { ptr: ox.ptr.into(), shape: ox.shape }
    }
}

impl<'a> From<OxMut<'a>> for OxPtrMut {
    fn from(ox: OxMut<'a>) -> Self {
        OxPtrMut { ptr: ox.ptr.into(), shape: ox.shape }
    }
}
```

## Call Sites

The `Shape::call_*` methods handle the conversion:

```rust
impl Shape {
    pub unsafe fn call_debug(
        &'static self,
        ptr: PtrConst<'_>,
        f: &mut fmt::Formatter<'_>,
    ) -> Option<fmt::Result> {
        match self.vtable {
            VTableErased::Direct(vt) => {
                vt.debug.map(|func| unsafe { func(ptr.as_byte_ptr() as *const (), f) })
            }
            VTableErased::Indirect(vt) => {
                let ox = OxPtr { ptr: ptr.into(), shape: self };
                vt.debug.and_then(|func| unsafe { func(ox, f) })
            }
        }
    }
}
```

## What This Eliminates

1. **HRTB (`for<'a>`)** - Gone from all 14 vtable fields × 2 vtable types
2. **`VTableStyle` trait** - No more associated type projections
3. **Generic `VTable<S>`** - Two concrete structs instead

## What This Preserves

1. **Lifetime safety in public APIs** - `OxRef<'a>`/`OxMut<'a>` still exist for `Peek`, `Partial`, etc.
2. **Binary size benefits** - Same code sharing, same vtable structure
3. **`VTableErased` dispatch** - Still works, just simpler types inside

## Migration

1. Add `OxPtr`/`OxPtrMut` types
2. Change `VTableDirect` and `VTableIndirect` to use concrete types (no generics)
3. Remove `VTableStyle` trait
4. Update `vtable_direct!` and `vtable_indirect!` macros
5. Update `Shape::call_*` methods
6. Keep `OxRef`/`OxMut` for public API, convert at vtable boundary

## Expected Impact

- Significant reduction in type-checking time (no HRTB resolution)
- Simpler error messages
- No change to runtime behavior
- No change to binary size

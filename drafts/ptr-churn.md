# Pointer Type Refactoring Plan

## Problem

The current pointer types have lifetimes that are constantly fabricated/worked around in vtable code. The lifetime tracking is a lie - these are raw pointers and safety is the caller's responsibility.

Current types:
- `Ptr` - raw internal type, no lifetime
- `PtrConst<'a>` - wraps Ptr, has lifetime
- `PtrMut<'a>` - wraps Ptr, has lifetime
- `PtrUninit<'a>` - wraps Ptr, has lifetime
- `OxPtr` - (Ptr, &'static Shape), no lifetime
- `OxPtrMut` - (Ptr, &'static Shape), no lifetime
- `OxRef<'a>` - (PtrConst<'a>, &'static Shape)
- `OxMut<'a>` - (PtrMut<'a>, &'static Shape)

## Proposed Design

Remove lifetimes from pointer types. Keep const/mut distinction via exposed methods.

New types:
- `PtrMut` - the raw pointer (renamed from `Ptr`), can do everything
- `PtrConst` - wraps `PtrMut`, hides mutation methods
- `PtrUninit` - wraps `PtrMut`, for uninitialized memory
- `OxPtrConst` - (PtrConst, &'static Shape) - renamed from `OxPtr`
- `OxPtrMut` - (PtrMut, &'static Shape)
- `OxPtrUninit` - (PtrUninit, &'static Shape) - for initializing values via vtables

Keep the lifetimed versions for safe public APIs:
- `OxRef<'a>` - safe lifetimed reference with shape
- `OxMut<'a>` - safe lifetimed mutable reference with shape

## Changes

### Step 1: Rename `Ptr` to `PtrMut`

In `facet-core/src/types/ptr/mod.rs`:
- Rename `struct Ptr` to `struct PtrMut`
- Update all internal references

### Step 2: Create new `PtrConst`

- `PtrConst` wraps `PtrMut` (not the other way around)
- Exposes only read methods: `get`, `as_byte_ptr`, `as_ptr`, `read`, `field`
- Hides: `as_mut`, `as_mut_byte_ptr`, `put`, `replace`, `drop_in_place`
- Add `as_mut()` that returns `PtrMut` (unsafe, for when you know you have exclusive access)

### Step 3: Update `PtrUninit`

- Remove lifetime parameter
- Still wraps `PtrMut` internally
- `assume_init()` returns `PtrMut`
- `put()` returns `PtrMut`

### Step 4: Rename `OxPtr` to `OxPtrConst`

In `facet-core/src/types/builtins.rs`:
- Rename `OxPtr` to `OxPtrConst`
- Change internal `ptr: Ptr` to `ptr: PtrConst`
- Update all usages across codebase

### Step 5: Update `OxPtrMut`

- Change internal `ptr: Ptr` to `ptr: PtrMut`
- `as_const()` returns `OxPtrConst`
- `as_uninit()` returns `OxPtrUninit`

### Step 6: Add `OxPtrUninit`

New type in `facet-core/src/types/builtins.rs`:
- Contains `(PtrUninit, &'static Shape)`
- `assume_init()` returns `OxPtrMut`
- `put<T>(value: T)` returns `OxPtrMut`
- Used for vtable functions that initialize memory (e.g., `default_in_place`)

### Step 7: Keep `OxRef<'a>` and `OxMut<'a>`

These stay as safe wrappers with real lifetimes for public APIs.
- `OxRef<'a>` contains `(PtrConst, &'static Shape, PhantomData<&'a ()>)`
- `OxMut<'a>` contains `(PtrMut, &'static Shape, PhantomData<&'a mut ()>)`

The lifetime is tracked via PhantomData for borrow checking in safe code,
but the underlying pointers are unlifetimed.

### Step 8: Update VTable signatures

All vtable functions use unlifetimed types:
- `OxPtrConst` for read-only receivers
- `OxPtrMut` for mutable receivers
- `OxPtrUninit` for initialization targets
- `PtrConst`, `PtrMut`, `PtrUninit` for parameters/returns

### Step 9: Fix all usages

- Update imports across crate
- Fix all call sites
- Remove fabricated lifetime gymnastics

## Migration Path

1. Do the rename/restructure in ptr/mod.rs
2. Update builtins.rs
3. Fix lib.rs exports
4. Fix each impls/ file
5. Fix other crates (facet-reflect, etc.)

## Benefits

- No more fabricating lifetimes in vtable code
- Honest about these being raw pointers
- Const/mut distinction is about API surface, not lifetimes
- Simpler mental model
- Less boilerplate conversions

//! Regression tests for the lifetime-widening use-after-free.
//!
//! `ShapeBuilder` defaults to `VarianceDesc::BIVARIANT`, and `Bivariant` was doing
//! double duty as both the lattice identity ("this type has no lifetimes") and the
//! placeholder for "nobody declared a variance for this type". Because
//! `Variance::can_grow()` is `true` for `Bivariant`, every lifetime-carrying shape
//! that forgot to call `.variance(..)` let `Peek::try_grow_lifetime::<'static>()`
//! succeed — handing safe code a `'static` handle to a borrow that is about to die.
//!
//! Each test below is a Miri-confirmed use-after-free that this crate used to
//! permit. They are written at the `Peek` level (rather than as full UAFs) so they
//! run under the normal test harness: `try_grow_lifetime` returning `None` is the
//! exact gate that stops the exploit.
//!
//! The variance *values* themselves are covered exhaustively by the table in
//! `facet-core/tests/integration/variance_no_bogus_grow.rs`.

use std::borrow::Cow;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;

use facet_reflect::Peek;

/// `Cow<'a, T>`: the original report.
///
/// `Cow`'s `Borrowed` arm holds `&'a B`, so the lifetime lives in `Cow` itself and
/// is invisible to any dep on `B` — `Cow<'a, str>`'s `B` is `str`, which is
/// bivariant. Declaring `base: Bivariant, deps: [covariant(T)]` therefore *looks*
/// correct and reproduces the use-after-free exactly. The base must be `Covariant`.
#[test]
fn cow_cannot_grow_to_static() {
    let owned = String::from("this string dies at end of scope");
    let cow: Cow<'_, str> = Cow::Borrowed(owned.as_str());

    let peek = Peek::new(&cow);
    assert!(
        peek.try_grow_lifetime::<'static>().is_none(),
        "Cow<'a, str> must not widen to 'static: the Borrowed arm holds &'a str, \
         so the widened value is a dangling reference"
    );
}

/// Owned `Cow`s are the same *shape*, so they are refused too. Variance is a
/// property of the type, not of the runtime discriminant — and reflection cannot
/// know which arm is live at the time it answers.
#[test]
fn owned_cow_is_refused_too_because_variance_is_per_shape() {
    let cow: Cow<'_, str> = Cow::Owned(String::from("owned"));
    let peek = Peek::new(&cow);
    assert!(peek.try_grow_lifetime::<'static>().is_none());
}

/// Contamination: containers propagate their element's variance faithfully, so a
/// wrong leaf answer silently becomes a wrong answer for everything holding it.
/// These were all `Bivariant` before the fix *because* `Cow` was.
#[test]
fn containers_of_cow_cannot_grow_to_static() {
    let owned = String::from("payload");
    let cow: Cow<'_, str> = Cow::Borrowed(owned.as_str());

    let v = vec![cow.clone()];
    assert!(
        Peek::new(&v).try_grow_lifetime::<'static>().is_none(),
        "Vec<Cow<'a, str>>"
    );

    let o = Some(cow.clone());
    assert!(
        Peek::new(&o).try_grow_lifetime::<'static>().is_none(),
        "Option<Cow<'a, str>>"
    );

    let b = Box::new(cow.clone());
    assert!(
        Peek::new(&b).try_grow_lifetime::<'static>().is_none(),
        "Box<Cow<'a, str>>"
    );

    let t = (1u32, cow.clone());
    assert!(
        Peek::new(&t).try_grow_lifetime::<'static>().is_none(),
        "(u32, Cow<'a, str>)"
    );
}

/// The `[U]` arms of the smart pointers are *separate impls* from their `<T>`
/// siblings, and every one of them was missing its variance declaration while the
/// `<T>` version had it. Sibling-of-`Cow` hole.
#[test]
fn boxed_and_refcounted_slices_cannot_grow_to_static() {
    let owned = String::from("sibling payload that dies at end of scope");
    let s: &str = owned.as_str();

    let boxed: Box<[&str]> = vec![s].into_boxed_slice();
    assert!(
        Peek::new(&boxed).try_grow_lifetime::<'static>().is_none(),
        "Box<[&'a str]>"
    );

    let arc: Arc<[&str]> = vec![s].into();
    assert!(
        Peek::new(&arc).try_grow_lifetime::<'static>().is_none(),
        "Arc<[&'a str]>"
    );

    let rc: Rc<[&str]> = vec![s].into();
    assert!(
        Peek::new(&rc).try_grow_lifetime::<'static>().is_none(),
        "Rc<[&'a str]>"
    );

    let arc_weak = Arc::downgrade(&arc);
    assert!(
        Peek::new(&arc_weak)
            .try_grow_lifetime::<'static>()
            .is_none(),
        "sync::Weak<[&'a str]>"
    );

    let rc_weak = Rc::downgrade(&rc);
    assert!(
        Peek::new(&rc_weak).try_grow_lifetime::<'static>().is_none(),
        "rc::Weak<[&'a str]>"
    );
}

/// `NonNull<T>` was judged "not exploitable — dereferencing a raw pointer needs
/// `unsafe`". That reasoning does not hold here: `NonNull`'s `Def::Pointer`
/// installs a `borrow_fn`, and [`PeekPointer::borrow_inner`] is a **safe** method
/// that calls it. facet-reflect performs the dereference on the caller's behalf,
/// so `NonNull::from(&r)` + grow + `borrow_inner` + `get::<&'static str>()` is a
/// complete use-after-free in `#![forbid(unsafe_code)]` user code.
///
/// [`PeekPointer::borrow_inner`]: facet_reflect::PeekPointer::borrow_inner
#[test]
fn nonnull_cannot_grow_to_static() {
    let owned = String::from("nonnull payload");
    let r: &str = owned.as_str();
    let nn: NonNull<&str> = NonNull::from(&r);

    let peek = Peek::new(&nn);
    assert!(
        peek.try_grow_lifetime::<'static>().is_none(),
        "NonNull<&'a str> must not widen: PeekPointer::borrow_inner() is safe and \
         will dereference it"
    );
}

/// `*const T` has no safe dereference path through reflection (it is `Def::Scalar`
/// with no `try_borrow_inner`), so this one is defence in depth rather than a
/// closed exploit. It still must not claim to be lifetime-free.
#[test]
fn const_ptr_of_reference_cannot_grow_to_static() {
    let owned = String::from("raw payload");
    let r: &str = owned.as_str();
    let p: *const &str = &r;

    assert!(
        Peek::new(&p).try_grow_lifetime::<'static>().is_none(),
        "*const &'a str propagates T's variance and must not widen"
    );
}

/// The canonical hand-rolled-borrow layout: all the real data sits behind a raw
/// pointer, and the lifetime is carried *only* by `PhantomData`. If `PhantomData`
/// claims to be lifetime-free then every field of this struct does, the field walk
/// concludes `Bivariant`, and `grow_lifetime` produces a `Slice<'static>` whose
/// safe accessors return dangling references.
#[test]
fn phantom_data_carries_the_lifetime_and_blocks_growth() {
    use facet::Facet;
    use std::marker::PhantomData;

    #[derive(Facet)]
    struct Slice<'a> {
        ptr: *const u8,
        len: usize,
        _marker: PhantomData<&'a [u8]>,
    }

    let owned: [u8; 3] = [1, 2, 3];
    let s = Slice {
        ptr: owned.as_ptr(),
        len: owned.len(),
        _marker: PhantomData,
    };

    assert!(
        Peek::new(&s).try_grow_lifetime::<'static>().is_none(),
        "a struct whose lifetime is carried only by PhantomData must not widen"
    );
}

/// Bare `PhantomData<&'a T>` on its own must not widen either.
#[test]
fn bare_phantom_data_cannot_grow_to_static() {
    use std::marker::PhantomData;

    let pd: PhantomData<&str> = PhantomData;
    assert!(Peek::new(&pd).try_grow_lifetime::<'static>().is_none());
}

/// Lock guards hold `&'a Lock<T>`, so — exactly like `Cow` — their base must be
/// `Covariant`, not `Bivariant`. All six guard impls had the `Cow` mistake.
#[test]
fn lock_guards_cannot_grow_to_static() {
    use std::sync::{Mutex, RwLock};

    let m = Mutex::new(42u32);
    let g = m.lock().unwrap();
    assert!(
        Peek::new(&g).try_grow_lifetime::<'static>().is_none(),
        "MutexGuard<'a, u32> holds &'a Mutex<u32>"
    );
    drop(g);

    let rw = RwLock::new(42u32);
    let r = rw.read().unwrap();
    assert!(
        Peek::new(&r).try_grow_lifetime::<'static>().is_none(),
        "RwLockReadGuard<'a, u32> holds &'a RwLock<u32>"
    );
    drop(r);

    // The write lock is only read from — we want the *guard type*, not the access.
    #[allow(clippy::readonly_write_lock)]
    let w = rw.write().unwrap();
    assert!(
        Peek::new(&w).try_grow_lifetime::<'static>().is_none(),
        "RwLockWriteGuard<'a, u32> holds &'a RwLock<u32>"
    );
}

/// The fix must not degenerate into "refuse everything". Genuinely lifetime-free
/// values still widen, because for them it is genuinely safe.
#[test]
fn genuinely_lifetime_free_values_still_grow() {
    let n = 42u32;
    assert!(Peek::new(&n).try_grow_lifetime::<'static>().is_some());

    let s = String::from("owned, no borrows");
    assert!(Peek::new(&s).try_grow_lifetime::<'static>().is_some());

    let v = vec![1u32, 2, 3];
    assert!(Peek::new(&v).try_grow_lifetime::<'static>().is_some());

    let b: Box<[u32]> = vec![1, 2, 3].into_boxed_slice();
    assert!(Peek::new(&b).try_grow_lifetime::<'static>().is_some());

    let p: *const u32 = &n;
    assert!(Peek::new(&p).try_grow_lifetime::<'static>().is_some());
}

/// Shrinking is the *other* direction and stays available for covariant types —
/// the fix must not have collapsed everything to `Invariant`.
#[test]
fn covariant_values_can_still_shrink() {
    let owned = String::from("payload");
    let cow: Cow<'_, str> = Cow::Borrowed(owned.as_str());
    assert!(
        Peek::new(&cow).try_shrink_lifetime().is_some(),
        "Cow<'a, str> is Covariant, so shrinking is still allowed"
    );

    let s: &str = owned.as_str();
    assert!(Peek::new(&s).try_shrink_lifetime().is_some());
}

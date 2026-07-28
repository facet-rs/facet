//! Table-driven guard: **no lifetime-carrying shape may report `can_grow()`**.
//!
//! # Why this file exists
//!
//! `ShapeBuilder`'s default is `VarianceDesc::BIVARIANT`. `Bivariant` is the
//! *identity element* of the variance lattice — but it was also doing duty as the
//! placeholder for "nobody declared a variance for this type". Those are not the
//! same claim. `Variance::can_grow()` returns `true` for `Bivariant`, so any shape
//! that carried a lifetime but forgot `.variance(..)` reported "I have no lifetime
//! parameters at all", `Peek::try_grow_lifetime::<'static>()` succeeded, and safe
//! code walked away with a dangling reference.
//!
//! `Shape::computed_variance` only repairs this for `UserType::Struct`/`Enum` (by
//! walking fields when `deps` is empty); every other `Type`/`Def` falls through to
//! `_ => desc.base`.
//!
//! # Why this is a *table*, and not spot checks
//!
//! A hand-written `VarianceDesc` is exactly as easy to get wrong as the default
//! was, and getting it wrong is silently unsound. The canonical trap is `Cow`:
//! writing `base: Bivariant, deps: [covariant(T)]` *looks* right and reproduces the
//! use-after-free verbatim, because `Cow<'a, str>`'s `T` is `str`, which is
//! bivariant — the `&'a B` in the `Borrowed` arm is only visible in the `base`.
//!
//! The same mistake was sitting in the lock guards: `MutexGuard<'a, T>` holds
//! `&'a Mutex<T>` but declared `base: Bivariant`.
//!
//! So: every lifetime-capable shape goes in the table below and the table is
//! asserted wholesale. A wrong `VarianceDesc` cannot pass silently.
//!
//! # The rule
//!
//! `can_grow()` means "this value may be relabelled with a *longer* lifetime, up
//! to and including `'static`". That is only ever true for `Contravariant` (things
//! that *consume* a lifetime, e.g. `fn(&'a T)`) and for `Bivariant` (things that
//! genuinely have no lifetime at all). A type that *holds* borrowed data is
//! `Covariant` or `Invariant`, and must answer `false`.

#![cfg(feature = "std")]

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use facet_core::{Facet, Shape, Variance};

/// `indexmap`'s and `iddqd`'s hasher parameters have no usable default here, so
/// spell one out.
#[cfg(any(feature = "indexmap", feature = "iddqd"))]
type Rs = std::collections::hash_map::RandomState;

/// One row of the table: a human-readable label, the shape, and the variance we
/// expect it to compute.
struct Row {
    label: &'static str,
    shape: &'static Shape,
    expected: Variance,
}

macro_rules! rows {
    ($($ty:ty => $expected:expr),* $(,)?) => {
        vec![$(Row {
            label: stringify!($ty),
            shape: <$ty as Facet>::SHAPE,
            expected: $expected,
        }),*]
    };
}

/// Shapes that carry a lifetime (directly, or through a parameter that does).
/// Every one of these MUST answer `can_grow() == false`.
fn lifetime_carrying_rows() -> Vec<Row> {
    use Variance::{Covariant, Invariant};

    #[allow(unused_mut)]
    let mut rows = rows![
        // ---------------------------------------------------------------------
        // References and slices. Already declared before this fix; regression
        // guards so the baseline cannot rot.
        // ---------------------------------------------------------------------
        &'static str => Covariant,
        &'static u32 => Covariant,
        &'static mut u32 => Covariant,
        &'static mut &'static u32 => Invariant,
        &'static [&'static str] => Covariant,

        // ---------------------------------------------------------------------
        // THE BUG. `Cow`'s `Borrowed` arm holds `&'a B`, so the *base* must be
        // `Covariant`. A `base: Bivariant, deps: [covariant(T)]` declaration
        // reproduces the original use-after-free exactly, because `str` is
        // bivariant and so contributes nothing.
        // ---------------------------------------------------------------------
        Cow<'static, str> => Covariant,
        Cow<'static, [u8]> => Covariant,

        // ---------------------------------------------------------------------
        // Contamination. Containers whose *correct* covariant deps faithfully
        // propagate whatever the leaf says. These were `Bivariant` before the fix
        // purely because `Cow` was — which is why spot-checking `Cow` alone would
        // not have been enough.
        // ---------------------------------------------------------------------
        Vec<Cow<'static, str>> => Covariant,
        Option<Cow<'static, str>> => Covariant,
        Box<Cow<'static, str>> => Covariant,
        Result<Cow<'static, str>, u32> => Covariant,
        (u32, Cow<'static, str>) => Covariant,
        HashMap<u32, Cow<'static, str>> => Covariant,
        Vec<Vec<Cow<'static, str>>> => Covariant,

        // ---------------------------------------------------------------------
        // The `[U]` arms of the smart pointers. Each is a *separate impl* from its
        // `<T>` sibling, and every one of them had been missed.
        // ---------------------------------------------------------------------
        Box<[&'static str]> => Covariant,
        Arc<[&'static str]> => Covariant,
        std::sync::Weak<[&'static str]> => Covariant,
        Rc<[&'static str]> => Covariant,
        std::rc::Weak<[&'static str]> => Covariant,
        // ...and the already-correct sized siblings, as regression guards.
        Box<&'static str> => Covariant,
        Arc<&'static str> => Covariant,
        std::sync::Weak<&'static str> => Covariant,
        Rc<&'static str> => Covariant,
        std::rc::Weak<&'static str> => Covariant,

        // ---------------------------------------------------------------------
        // Raw pointers. `*const T` is covariant with respect to `T`; it carried no
        // declaration at all, so it answered `Bivariant` for every `T`.
        // ---------------------------------------------------------------------
        *const &'static str => Covariant,
        *mut u32 => Invariant,
        *mut &'static str => Invariant,

        // ---------------------------------------------------------------------
        // `NonNull<T>` is covariant with respect to `T`. This one *is* reachable
        // from 100% safe code: its `Def::Pointer` installs a `borrow_fn`, and
        // `PeekPointer::borrow_inner()` is a safe method that calls it. So the
        // "you'd need `unsafe` to dereference it" argument does not hold —
        // facet-reflect performs the dereference for you.
        // ---------------------------------------------------------------------
        std::ptr::NonNull<&'static str> => Covariant,

        // ---------------------------------------------------------------------
        // `PhantomData<T>` is declared `Invariant`. Its impl is `T: ?Sized + 'a`
        // with no `T: Facet` bound, so there is no `T::SHAPE` to depend on and the
        // only sound constant is `Invariant`. See the impl for the full argument.
        // ---------------------------------------------------------------------
        std::marker::PhantomData<&'static str> => Invariant,
        std::marker::PhantomData<u32> => Invariant,

        // ---------------------------------------------------------------------
        // Lock guards. Same mistake as `Cow`: each holds `&'a Lock<T>`, so the
        // base must be `Covariant`, but all six declared `base: Bivariant`.
        // ---------------------------------------------------------------------
        std::sync::MutexGuard<'static, u32> => Covariant,
        std::sync::RwLockReadGuard<'static, u32> => Covariant,
        std::sync::RwLockWriteGuard<'static, u32> => Covariant,
        std::sync::MutexGuard<'static, &'static str> => Invariant,
        std::sync::RwLockReadGuard<'static, &'static str> => Covariant,
        std::sync::RwLockWriteGuard<'static, &'static str> => Invariant,

        // Locks themselves give out `&mut`, so their contents are invariant.
        std::sync::Mutex<&'static str> => Invariant,
        std::sync::RwLock<&'static str> => Invariant,
        std::sync::OnceLock<&'static str> => Invariant,

        // ---------------------------------------------------------------------
        // The opaque wrappers, holding borrowed payloads. Both are lifetime
        // *boundaries* — reflection cannot see through them — so both declare
        // `Invariant` unconditionally, and owned payloads land there too (which is
        // why they are not in the bivariant table below).
        // `OpaqueBorrow<'facet, T>` is what the derive emits for
        // `#[facet(opaque)]` fields, so it is the one that meets borrowed data in
        // practice. Invariance is only half the guard here: the other half is shape
        // identity, which is issue #1563 — see
        // facet-reflect/tests/poke/opaque_lifetime_laundering.rs.
        // ---------------------------------------------------------------------
        facet_core::Opaque<&'static str> => Invariant,
        facet_core::OpaqueBorrow<'static, &'static str> => Invariant,

        // ---------------------------------------------------------------------
        // Collections that were already declared — regression guards.
        // ---------------------------------------------------------------------
        Vec<&'static str> => Covariant,
        BTreeMap<&'static str, u32> => Covariant,
        BTreeMap<u32, &'static str> => Covariant,
        BTreeSet<&'static str> => Covariant,
        HashMap<&'static str, u32> => Covariant,
        HashMap<u32, &'static str> => Covariant,
        HashSet<&'static str> => Covariant,
        [&'static str; 3] => Covariant,
        Option<&'static str> => Covariant,
    ];

    #[cfg(feature = "indexmap")]
    rows.extend(rows![
        indexmap::IndexMap<&'static str, u32, Rs> => Covariant,
        indexmap::IndexMap<u32, &'static str, Rs> => Covariant,
        indexmap::IndexSet<&'static str, Rs> => Covariant,
        indexmap::IndexMap<u32, Cow<'static, str>, Rs> => Covariant,
    ]);

    #[cfg(feature = "lock_api")]
    rows.extend(rows![
        parking_lot::Mutex<&'static str> => Invariant,
        parking_lot::RwLock<&'static str> => Invariant,
        parking_lot::MutexGuard<'static, u32> => Covariant,
        parking_lot::RwLockReadGuard<'static, u32> => Covariant,
        parking_lot::RwLockWriteGuard<'static, u32> => Covariant,
        parking_lot::RwLockReadGuard<'static, &'static str> => Covariant,
        parking_lot::RwLockWriteGuard<'static, &'static str> => Invariant,
    ]);

    #[cfg(feature = "yoke")]
    rows.extend(rows![
        // Yoke is self-referential; it is unconditionally invariant.
        yoke::Yoke<&'static str, Box<[u8]>> => Invariant,
    ]);

    #[cfg(feature = "smallvec")]
    rows.extend(rows![
        smallvec::SmallVec<[&'static str; 4]> => Covariant,
        smallvec::SmallVec<[Cow<'static, str>; 4]> => Covariant,
    ]);

    rows
}

/// Shapes that genuinely have no lifetime parameter. These are *allowed* to be
/// `Bivariant`, and asserting so keeps the fix from degenerating into "mark
/// everything `Invariant`", which would be sound and worthless.
fn lifetime_free_rows() -> Vec<Row> {
    use Variance::Bivariant;

    #[allow(unused_mut)]
    let mut rows = rows![
        u32 => Bivariant,
        bool => Bivariant,
        String => Bivariant,
        Vec<u32> => Bivariant,
        Option<u32> => Bivariant,
        Box<u32> => Bivariant,
        Box<[u32]> => Bivariant,
        Arc<[u32]> => Bivariant,
        Rc<[u32]> => Bivariant,
        std::sync::Weak<[u32]> => Bivariant,
        std::rc::Weak<[u32]> => Bivariant,
        *const u32 => Bivariant,
        std::ptr::NonNull<u32> => Bivariant,
        HashMap<String, u32> => Bivariant,
        // The shapes whose struct/enum field walk already got this right without an
        // explicit declaration — they must keep working.
        std::ops::Range<u32> => Bivariant,
        (u32, bool) => Bivariant,
        (u32, bool, String) => Bivariant,
    ];

    #[cfg(feature = "num-complex")]
    rows.extend(rows![
        num_complex::Complex<f64> => Bivariant,
        num_complex::Complex<u32> => Bivariant,
    ]);

    #[cfg(feature = "indexmap")]
    rows.extend(rows![
        indexmap::IndexMap<String, u32, Rs> => Bivariant,
        indexmap::IndexSet<String, Rs> => Bivariant,
    ]);

    #[cfg(feature = "iddqd")]
    rows.extend(rows![
        // All four iddqd maps bound their item type `T: 'static`, so they cannot
        // hold a borrow and genuinely are bivariant — they are not exploitable.
        // They now *declare* that instead of inheriting it from the `ShapeBuilder`
        // default, which would become wrong the moment that bound is relaxed.
        iddqd::IdHashMap<iddqd_fixtures::Item, Rs> => Bivariant,
        iddqd::IdOrdMap<iddqd_fixtures::Item> => Bivariant,
        iddqd::BiHashMap<iddqd_fixtures::BiItem, Rs> => Bivariant,
        iddqd::TriHashMap<iddqd_fixtures::TriItem, Rs> => Bivariant,
    ]);

    rows
}

/// Item types for the four iddqd maps. Each map extracts its keys from the item,
/// so we cannot use a bare `String`.
#[cfg(feature = "iddqd")]
mod iddqd_fixtures {
    use facet::Facet;

    #[derive(Facet)]
    pub struct Item {
        pub key: String,
        pub value: u32,
    }

    impl iddqd::IdHashItem for Item {
        type Key<'k> = &'k str;
        fn key(&self) -> Self::Key<'_> {
            &self.key
        }
        iddqd::id_upcast!();
    }

    impl iddqd::IdOrdItem for Item {
        type Key<'k> = &'k str;
        fn key(&self) -> Self::Key<'_> {
            &self.key
        }
        iddqd::id_upcast!();
    }

    #[derive(Facet)]
    pub struct BiItem {
        pub k1: String,
        pub k2: u32,
    }

    impl iddqd::BiHashItem for BiItem {
        type K1<'k> = &'k str;
        type K2<'k> = u32;
        fn key1(&self) -> Self::K1<'_> {
            &self.k1
        }
        fn key2(&self) -> Self::K2<'_> {
            self.k2
        }
        iddqd::bi_upcast!();
    }

    #[derive(Facet)]
    pub struct TriItem {
        pub k1: String,
        pub k2: u32,
        pub k3: u64,
    }

    impl iddqd::TriHashItem for TriItem {
        type K1<'k> = &'k str;
        type K2<'k> = u32;
        type K3<'k> = u64;
        fn key1(&self) -> Self::K1<'_> {
            &self.k1
        }
        fn key2(&self) -> Self::K2<'_> {
            self.k2
        }
        fn key3(&self) -> Self::K3<'_> {
            self.k3
        }
        iddqd::tri_upcast!();
    }
}

// -----------------------------------------------------------------------------
// The assertions
// -----------------------------------------------------------------------------

/// The load-bearing one. Nothing that holds borrowed data may claim it can be
/// relabelled `'static`.
#[test]
fn no_lifetime_carrying_shape_can_grow() {
    let failures: Vec<String> = lifetime_carrying_rows()
        .into_iter()
        .filter_map(|row| {
            let actual = row.shape.computed_variance();
            actual.can_grow().then(|| {
                format!(
                    "  {} => {actual:?} (can_grow() == true; expected {:?})",
                    row.label, row.expected
                )
            })
        })
        .collect();

    assert!(
        failures.is_empty(),
        "these lifetime-carrying shapes report can_grow() == true, so \
         Peek::try_grow_lifetime::<'static>() will hand out dangling references to \
         safe code:\n{}",
        failures.join("\n")
    );
}

/// The exact-value check. `can_grow() == false` alone would be satisfied by
/// blanket-`Invariant`; this pins down that we picked the *right* variance and did
/// not simply hammer everything to the bottom of the lattice.
#[test]
fn lifetime_carrying_shapes_have_exact_expected_variance() {
    let failures: Vec<String> = lifetime_carrying_rows()
        .into_iter()
        .filter_map(|row| {
            let actual = row.shape.computed_variance();
            (actual != row.expected).then(|| {
                format!(
                    "  {}: expected {:?}, got {actual:?}",
                    row.label, row.expected
                )
            })
        })
        .collect();

    assert!(
        failures.is_empty(),
        "variance mismatches:\n{}",
        failures.join("\n")
    );
}

/// Lifetime-free shapes must stay `Bivariant`, and therefore keep `can_grow()`.
#[test]
fn lifetime_free_shapes_stay_bivariant() {
    let failures: Vec<String> = lifetime_free_rows()
        .into_iter()
        .filter_map(|row| {
            let actual = row.shape.computed_variance();
            (actual != Variance::Bivariant).then(|| format!("  {}: got {actual:?}", row.label))
        })
        .collect();

    assert!(
        failures.is_empty(),
        "these shapes have no lifetime parameters and should be Bivariant:\n{}",
        failures.join("\n")
    );
}

/// The two tables make contradictory claims, so nothing may appear in both. A
/// shape listed twice would be asserted `Bivariant` *and* asserted not-`can_grow`,
/// which is impossible — the contradiction should surface here rather than as a
/// confusing failure in one of the tests above.
#[test]
fn lifetime_free_and_carrying_are_disjoint() {
    let carrying: Vec<&str> = lifetime_carrying_rows().iter().map(|r| r.label).collect();
    let overlap: Vec<&str> = lifetime_free_rows()
        .iter()
        .map(|r| r.label)
        .filter(|l| carrying.contains(l))
        .collect();

    assert!(
        overlap.is_empty(),
        "a shape cannot be both lifetime-carrying and lifetime-free: {overlap:?}"
    );
}

/// Spell out the lattice contract the tables above depend on.
#[test]
fn can_grow_is_only_true_for_contravariant_and_bivariant() {
    assert!(Variance::Bivariant.can_grow());
    assert!(Variance::Contravariant.can_grow());
    assert!(!Variance::Covariant.can_grow());
    assert!(!Variance::Invariant.can_grow());
}

/// The tables must be big enough to be worth having.
#[test]
fn tables_are_populated() {
    assert!(
        lifetime_carrying_rows().len() >= 40,
        "lifetime-carrying table shrank unexpectedly: {}",
        lifetime_carrying_rows().len()
    );
    assert!(lifetime_free_rows().len() >= 15);
}

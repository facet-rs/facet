//! Table-driven guard: **no safe path yields an opaque value with a longer
//! `'facet` than the value actually has.**
//!
//! # Why this file exists
//!
//! `ConstTypeId` erases lifetimes by design — `const_typeid.rs` transmutes a
//! `&dyn NonStaticAny` to `&(dyn NonStaticAny + 'static)`. So `Opaque<&'a mut String>`
//! and `Opaque<&'static mut String>` are *one and the same* `ShapeId`, and every
//! typed accessor in reflection (`Poke::get`/`get_mut`/`set`, `Peek::get`,
//! `Partial::set`) is a bare `self.shape != T::SHAPE` comparison.
//!
//! What actually keeps those accessors honest is the id **together with** the
//! `T: Facet<'facet>` bound:
//!
//! * `OpaqueBorrow<'x, T>` implements `Facet<'facet>` only for `'facet == 'x`, so
//!   the bound pins the requested lifetime to the one the value has. Asking a
//!   `Poke<'_, 'a>` for `OpaqueBorrow<'static, _>` is a *compile* error — that is
//!   `compile_tests/fixtures/opaque_borrow_insufficient_lifetime.rs`.
//! * `Opaque<T>` implements `Facet<'facet>` for **every** `'facet` (it is bounded
//!   `T: 'static` instead). The bound is therefore vacuous, and the id is the only
//!   thing left standing.
//!
//! Issue #1563 (dtolnay) was exactly this, entered through `Opaque(&mut s)`. It was
//! fixed in bdf1dcfa6 by requiring `T: 'static` on `Opaque`. Then cd254d928 (#2087)
//! added `OpaqueBorrow<'facet, T: 'facet>` — correct in itself — but gave it
//! `Opaque<T>`'s shape identity "for compatibility":
//!
//! ```ignore
//! Shape::builder_for_sized::<Opaque<T>>("Opaque")   // <-- inside OpaqueBorrow's impl
//! ```
//!
//! That handed the vacuous-bound type the borrowed type's id. Both are
//! `#[repr(transparent)]` over `T`, so the layouts match and safe code could turn
//! `&'a mut String` into `&'static mut String`: `free(): double free detected`
//! natively, `pointer not dereferenceable: alloc has been freed` under Miri.
//!
//! # Why this is a *table*, and not spot checks
//!
//! The fixture that guarded #1563 (`opaque_insufficient_lifetime.rs`) tested the one
//! entry point from the bug report, `Opaque(&mut s)`. It still passed with the hole
//! wide open, because the hole was in a *different* type — and it is behind
//! `slow-tests`, so it does not run by default. A guard that names one door does not
//! guard the building.
//!
//! So: every opaque wrapper shape goes in the table below and the table is asserted
//! wholesale (pairwise-distinct ids, invariant variance), and every typed accessor
//! that reads `self.shape != T::SHAPE` gets its own laundering attempt. Adding a
//! third opaque wrapper without adding a row is the mistake this file exists to
//! catch, and at least these tests run on a plain `cargo test`.

#![cfg(feature = "std")]

use facet::{Facet, Opaque, OpaqueBorrow, Shape, Type, UserType, Variance};
use facet_reflect::{Peek, Poke};
use facet_testhelpers::test;

/// A type that deliberately does not derive `Facet` — the reason one reaches for
/// `#[facet(opaque)]` in the first place.
#[derive(Debug, PartialEq)]
struct NotFacet(u64);

/// A struct whose opaque field is genuinely borrowed. This is the shape the derive
/// emits for `#[facet(opaque)]`, and therefore the shape a real exploit would meet.
#[derive(Facet)]
struct BorrowedOpaqueField<'a> {
    #[facet(opaque)]
    inner: &'a NotFacet,
}

// -----------------------------------------------------------------------------
// The table
// -----------------------------------------------------------------------------

struct Row {
    label: &'static str,
    shape: &'static Shape,
}

macro_rules! rows {
    ($($ty:ty),* $(,)?) => {
        vec![$(Row {
            label: stringify!($ty),
            shape: <$ty as Facet>::SHAPE,
        }),*]
    };
}

/// Every opaque wrapper shape reflection can hand out. Each `Opaque<T>` row is
/// paired with its `OpaqueBorrow<'_, T>` twin, because that pairing is precisely
/// what was collapsed.
fn opaque_wrapper_rows() -> Vec<Row> {
    rows![
        // The exploit pair from the reproducer: same layout, same `T`, and — before
        // this fix — the same `ShapeId`.
        Opaque<&'static mut String>,
        OpaqueBorrow<'static, &'static mut String>,
        // Shared references. `&'a T` laundered to `&'static T` is a read-only UAF,
        // which is no less undefined.
        Opaque<&'static str>,
        OpaqueBorrow<'static, &'static str>,
        Opaque<&'static NotFacet>,
        OpaqueBorrow<'static, &'static NotFacet>,
        // Owned payloads. These cannot launder anything on their own, but they must
        // still not collide with each other: a `Poke` over `Opaque<String>` must not
        // accept `OpaqueBorrow<'_, String>` either, or the confusion just moves.
        Opaque<String>,
        OpaqueBorrow<'static, String>,
        Opaque<u64>,
        OpaqueBorrow<'static, u64>,
        Opaque<NotFacet>,
        OpaqueBorrow<'static, NotFacet>,
    ]
}

/// The load-bearing one. Two opaque wrappers that are not the same Rust type must
/// not answer to the same `ShapeId`, or every `self.shape != T::SHAPE` check in
/// reflection silently becomes a no-op between them.
#[test]
fn opaque_wrapper_shapes_are_pairwise_distinct() {
    let rows = opaque_wrapper_rows();

    let mut collisions = Vec::new();
    for (i, a) in rows.iter().enumerate() {
        for b in &rows[i + 1..] {
            if a.shape == b.shape {
                collisions.push(format!("  {} == {}", a.label, b.label));
            }
        }
    }

    assert!(
        collisions.is_empty(),
        "these opaque wrappers share a ShapeId, so `Poke::get_mut`/`set`, `Peek::get` \
         and `Partial::set` will happily reinterpret one as the other — and since the \
         `Facet<'facet>` bound on `Opaque<T>` is satisfiable for every `'facet`, that \
         is a lifetime-laundering hole (issue #1563):\n{}",
        collisions.join("\n")
    );
}

/// `Opaque` and `OpaqueBorrow` are both declared `INVARIANT`, so no opaque wrapper
/// may be relabelled with a longer lifetime through `Peek::try_grow_lifetime`. This
/// is the *other* road to the same destination, and it is cheap to keep shut.
#[test]
fn opaque_wrappers_are_invariant_and_cannot_grow() {
    let failures: Vec<String> = opaque_wrapper_rows()
        .into_iter()
        .filter_map(|row| {
            let actual = row.shape.computed_variance();
            (actual != Variance::Invariant || actual.can_grow()).then(|| {
                format!(
                    "  {}: got {actual:?} (can_grow: {})",
                    row.label,
                    actual.can_grow()
                )
            })
        })
        .collect();

    assert!(
        failures.is_empty(),
        "opaque wrappers are lifetime boundaries and must be Invariant, so that \
         `Peek::try_grow_lifetime::<'static>()` refuses them:\n{}",
        failures.join("\n")
    );
}

// -----------------------------------------------------------------------------
// The laundering attempts, one per typed accessor
// -----------------------------------------------------------------------------

/// Every accessor in `facet-reflect` that gates on `self.shape != T::SHAPE` must
/// reject the request. Asserted wholesale: one accessor forgetting the check is
/// exactly as fatal as all of them forgetting it.
///
/// This is the reproducer from the audit, minus the actual use-after-free — with the
/// hole open, every one of these `is_ok()`.
#[test]
fn no_accessor_launders_opaque_borrow_into_opaque() {
    let mut failures = Vec::new();

    let mut victim = String::from("i am a stack local");
    let mut wrapper: OpaqueBorrow<'_, &mut String> = OpaqueBorrow::new(&mut victim);

    {
        let mut poke = Poke::new(&mut wrapper);

        // `Opaque<&'static mut String>: Facet<'a>` holds for *any* `'a`, so this
        // compiles. Only the ShapeId can stop it.
        if poke.get::<Opaque<&'static mut String>>().is_ok() {
            failures.push("Poke::get::<Opaque<&'static mut String>>");
        }
        if poke.get_mut::<Opaque<&'static mut String>>().is_ok() {
            failures.push("Poke::get_mut::<Opaque<&'static mut String>>");
        }
    }

    // `set` is the same check, and strictly worse: it writes. Exercised on a shared
    // borrow so the replacement value can be a `'static` literal — a `&'static mut`
    // would have to come from `Box::leak`, which Miri's leak checker rejects.
    let borrowed = String::from("i am also a stack local");
    let mut shared: OpaqueBorrow<'_, &str> = OpaqueBorrow::new(&borrowed);
    {
        let mut poke = Poke::new(&mut shared);
        if poke.set(Opaque::<&'static str>("laundered")).is_ok() {
            failures.push("Poke::set(Opaque<&'static str>) over OpaqueBorrow<'_, &str>");
        }
        if poke.get_mut::<Opaque<&'static str>>().is_ok() {
            failures.push("Poke::get_mut::<Opaque<&'static str>> over OpaqueBorrow<'_, &str>");
        }
    }

    {
        let peek = Peek::new(&wrapper);
        if peek.get::<Opaque<&'static mut String>>().is_ok() {
            failures.push("Peek::get::<Opaque<&'static mut String>>");
        }
        // Invariance must also refuse the relabelling road.
        if peek.try_grow_lifetime::<'static>().is_some() {
            failures.push("Peek::try_grow_lifetime::<'static>");
        }
    }

    assert!(
        failures.is_empty(),
        "these safe accessors handed out a `&'static mut String` borrowed from a \
         stack local (issue #1563, reopened by #2087):\n{}",
        failures
            .iter()
            .map(|f| format!("  {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // The borrows are still exactly as long as they started.
    assert_eq!(wrapper.0, "i am a stack local");
    assert_eq!(shared.0, "i am also a stack local");
}

/// The mirror image: an `Opaque<T>` value must not answer to `OpaqueBorrow<'_, T>`
/// either. Nothing unsound follows from this direction today, but it is the same
/// collision, and a "compatibility" alias reintroduced in either direction would
/// bring the hole back.
#[test]
fn no_accessor_launders_opaque_into_opaque_borrow() {
    let mut failures = Vec::new();
    let mut wrapper = Opaque(NotFacet(35));

    {
        let mut poke = Poke::new(&mut wrapper);
        if poke.get::<OpaqueBorrow<'_, NotFacet>>().is_ok() {
            failures.push("Poke::get::<OpaqueBorrow<'_, NotFacet>>");
        }
        if poke.get_mut::<OpaqueBorrow<'_, NotFacet>>().is_ok() {
            failures.push("Poke::get_mut::<OpaqueBorrow<'_, NotFacet>>");
        }
        if poke.set(OpaqueBorrow::new(NotFacet(36))).is_ok() {
            failures.push("Poke::set(OpaqueBorrow<'_, NotFacet>)");
        }
    }

    if Peek::new(&wrapper)
        .get::<OpaqueBorrow<'_, NotFacet>>()
        .is_ok()
    {
        failures.push("Peek::get::<OpaqueBorrow<'_, NotFacet>>");
    }

    assert!(
        failures.is_empty(),
        "`Opaque<T>` and `OpaqueBorrow<'_, T>` are different types and must not be \
         interchangeable through reflection:\n{}",
        failures
            .iter()
            .map(|f| format!("  {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    assert_eq!(wrapper.0, NotFacet(35));
}

// -----------------------------------------------------------------------------
// The derive-generated door
// -----------------------------------------------------------------------------

/// `#[facet(opaque)]` fields are the only reason `OpaqueBorrow` exists, so this is
/// where a real exploit would enter: build the struct with a short borrow, then walk
/// to the field through `Peek` and ask for the `'static` flavour.
#[test]
fn derived_opaque_field_does_not_launder_its_borrow() {
    let victim = NotFacet(35);
    let container = BorrowedOpaqueField { inner: &victim };

    let field = Peek::new(&container)
        .into_struct()
        .expect("BorrowedOpaqueField is a struct")
        .field_by_name("inner")
        .expect("field `inner` exists");

    // The derive must emit `OpaqueBorrow`, not `Opaque` — `Opaque<&'a NotFacet>`
    // would not even satisfy `T: 'static`, which is why #2087 introduced the type.
    assert_eq!(
        field.shape(),
        <OpaqueBorrow<'_, &NotFacet> as Facet>::SHAPE,
        "derive should emit OpaqueBorrow for `#[facet(opaque)]` fields"
    );
    assert_ne!(
        field.shape(),
        <Opaque<&'static NotFacet> as Facet>::SHAPE,
        "the derived field shape must not be interchangeable with Opaque<&'static _>"
    );

    let mut failures = Vec::new();
    if field.get::<Opaque<&'static NotFacet>>().is_ok() {
        failures.push("Peek::get::<Opaque<&'static NotFacet>> on a derived opaque field");
    }
    if field.try_grow_lifetime::<'static>().is_some() {
        failures.push("Peek::try_grow_lifetime::<'static>() on a derived opaque field");
    }

    assert!(
        failures.is_empty(),
        "a `#[facet(opaque)]` field holding a `&'a NotFacet` was handed out as \
         `&'static NotFacet`:\n{}",
        failures
            .iter()
            .map(|f| format!("  {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // Sanity: the field really is the opaque wrapper, and it really is invariant.
    assert!(matches!(field.shape().ty, Type::User(UserType::Opaque)));
    assert!(!field.shape().computed_variance().can_grow());
}

/// The table must be big enough to be worth having, and must stay paired: every
/// `Opaque<T>` row needs its `OpaqueBorrow<'_, T>` twin, or the collision it exists
/// to detect has nothing to collide with.
#[test]
fn table_is_populated_and_paired() {
    let rows = opaque_wrapper_rows();
    assert!(
        rows.len() >= 12 && rows.len().is_multiple_of(2),
        "opaque wrapper table should be an even, non-trivial number of rows: {}",
        rows.len()
    );
    for pair in rows.chunks(2) {
        assert!(
            pair[0].label.starts_with("Opaque<") && pair[1].label.starts_with("OpaqueBorrow<"),
            "rows must be laid out as (Opaque<T>, OpaqueBorrow<'_, T>) pairs, got ({}, {})",
            pair[0].label,
            pair[1].label
        );
    }
}

/// A rejection must be able to *name* what it rejected.
///
/// The shapes were already distinct (that is the soundness fix above), but both
/// carried `type_identifier: "Opaque"`, so the correct refusal to interchange
/// them rendered as `Wrong shape: expected Opaque, but got Opaque` — true, and
/// useless. Distinct ids are what keeps this sound; distinct *names* are what
/// makes it debuggable, and nothing else in the type system enforces the latter.
#[test]
fn opaque_wrappers_are_distinguishable_by_name() {
    let mut wrapper: OpaqueBorrow<'_, u64> = OpaqueBorrow::new(35);
    let mut poke = Poke::new(&mut wrapper);

    // `Opaque<u64>` is not `Debug`, so no `expect_err` here.
    let Err(err) = poke.get_mut::<Opaque<u64>>() else {
        panic!("Opaque<u64> must not be accepted for an OpaqueBorrow<'_, u64>");
    };
    let rendered = format!("{err}");

    assert!(
        !rendered.contains("expected Opaque, but got Opaque"),
        "the rejection does not say which shape is which: {rendered}"
    );
    assert!(
        rendered.contains("OpaqueBorrow"),
        "the rejection should name OpaqueBorrow so the reader can act on it: {rendered}"
    );
}

//! Table-driven guard: **no safe path may put non-`Sync` data behind `Attr`'s
//! `unsafe impl Sync`.**
//!
//! # Why this file exists
//!
//! `Attr` stores its payload as an `OxRef<'static>` — a type-erased
//! pointer-plus-`Shape`. The compiler therefore cannot see what the payload
//! *is*, and cannot check the one thing `Attr`'s hand-written
//! `unsafe impl Send + Sync` depends on: that the pointed-to type is `Sync`.
//! Only the construction site knows that, so every construction site must
//! either prove it (a `T: Sync` bound, or a concrete `Sync` type) or be
//! `unsafe` and push the obligation onto its caller.
//!
//! facet-rs/facet#1573 (dtolnay) is exactly this hole: an `Attr` over a
//! `&'static Rc<i32>` is `Sync`, so 200 threads can `Rc::clone` through it
//! concurrently and race the non-atomic refcount. Miri calls it
//! "Data race detected between (1) non-atomic write ... and (2) non-atomic
//! read"; natively it corrupts the count or aborts with
//! "unsafe precondition(s) violated: hint::assert_unchecked".
//!
//! The fix in `1524e017a` added `T: Sync` to `Attr::new` — the one entry point
//! the bug report named. But all three fields were `pub` on that same day, so
//!
//! ```ignore
//! let attr = Attr { ns: None, key: "", data: OxRef::from_ref(rc) };
//! ```
//!
//! reached the identical race from a crate with `#![forbid(unsafe_code)]`. The
//! bound was never the whole invariant; *the set of construction sites* is. So
//! the fields are private now, and this file asserts the set.
//!
//! # Why this is a *table*, and not a spot check
//!
//! A test that pins `Attr::new`'s bound would have passed continuously while
//! #1573 was open — the hole was in a constructor nobody had thought to test.
//! The only test that catches that class of bug is one that enumerates *every*
//! way to build an `Attr` and fails when a new one appears. Hence
//! [`CONSTRUCTORS`]: adding an unlisted `-> Self` to `attr.rs` fails
//! [`every_attr_constructor_is_accounted_for`], and re-`pub`-ing a field fails
//! [`attr_fields_are_private`], whether or not anyone remembers this issue.
//!
//! The source-level tests read `attr.rs` through `include_str!`, so they are
//! compiled in — no filesystem, no working directory, and they run under Miri.

#![cfg(feature = "std")]

use std::sync::Mutex;
use std::thread;

use facet_core::{Attr, Facet, OxRef, PtrConst, Shape};

/// The source of the module under audit, baked into the test binary.
const ATTR_RS: &str = include_str!("../../src/types/attr.rs");

// -----------------------------------------------------------------------------
// Type-level assertions
// -----------------------------------------------------------------------------

/// `static_assertions::assert_not_impl_all!`, inlined. Two blanket impls
/// overlap exactly when `$x` implements every `$t`, so the method call is
/// ambiguous — and therefore a compile error — precisely in that case.
macro_rules! assert_not_impl {
    ($x:ty: $($t:path),+ $(,)?) => {
        const _: fn() = || {
            trait AmbiguousIfImpl<A> {
                fn some_item() {}
            }
            impl<T: ?Sized> AmbiguousIfImpl<()> for T {}
            impl<T: ?Sized $(+ $t)*> AmbiguousIfImpl<u8> for T {}
            let _ = <$x as AmbiguousIfImpl<_>>::some_item;
        };
    };
}

/// The promise the rest of this file is about. If these ever stop holding, the
/// `unsafe impl`s were removed and everything below can be deleted with them.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}
    assert_send_sync::<Attr>();
};

// `OxRef` is a raw pointer plus a `&'static Shape`, and it deliberately has no
// `unsafe impl` of its own. That is *why* `OxRef::from_ref` is allowed to be
// safe and unbounded: an `OxRef` over non-`Sync` data cannot itself reach
// another thread, so it harms nobody until something with an `unsafe impl Sync`
// — i.e. `Attr` — wraps it. Push a `Sync` bound down onto `from_ref` and you
// break every legitimate non-`'static`, non-`Sync` reflection use for a
// guarantee that belongs one layer up. If these assertions start failing,
// `OxRef` itself became shareable and `from_ref` *does* then need the bound.
assert_not_impl!(OxRef<'static>: Send);
assert_not_impl!(OxRef<'static>: Sync);

// -----------------------------------------------------------------------------
// The table
// -----------------------------------------------------------------------------

/// How a constructor discharges the "payload is `Sync`" obligation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gate {
    /// The signature is `unsafe fn`: the caller asserts it.
    UnsafeFn,
    /// The signature carries a `Sync` bound: the compiler checks it.
    SyncBound,
    /// The signature takes a concrete type that is statically known `Sync`, so
    /// neither of the above is needed. The named type is asserted `Sync` in
    /// [`concretely_gated_constructors_take_sync_types`].
    ConcreteSyncType(&'static str),
}

/// One row: a function in `attr.rs` that yields an `Attr`, and its gate.
struct Ctor {
    name: &'static str,
    gate: Gate,
    /// Why this gate is the right one. Prose, for the next reader.
    why: &'static str,
}

/// **Every** way to obtain an `Attr` from outside `attr.rs`. Adding a function
/// that returns `Self` without adding it here is a test failure, by design —
/// that is the check #1573 needed and did not have.
const CONSTRUCTORS: &[Ctor] = &[
    Ctor {
        name: "from_raw_parts",
        gate: Gate::UnsafeFn,
        why: "takes an already-erased OxRef, so nothing is left to check; the \
              caller asserts Sync. This is the derive macro's entry point.",
    },
    Ctor {
        name: "new",
        gate: Gate::SyncBound,
        why: "the `T: Sync` bound added by 1524e017a for #1573.",
    },
    Ctor {
        name: "new_shape",
        gate: Gate::ConcreteSyncType("Shape"),
        why: "delegates to `new` with `&'static Shape`; `Shape` is Sync, so the \
              bound is discharged at the delegation rather than restated.",
    },
];

// -----------------------------------------------------------------------------
// Source-level assertions
// -----------------------------------------------------------------------------

/// A `fn` declaration in `attr.rs`: its name and its signature text (everything
/// from the item's first line up to and including the `{` that opens the body).
struct FnDecl {
    name: String,
    signature: String,
}

/// Collect every `fn` declared in `attr.rs`. Signatures may span lines, so
/// accumulate until the body-opening brace; nothing in a Rust signature
/// contains a `{`, so the first one always ends it.
fn declared_fns(src: &str) -> Vec<FnDecl> {
    let mut out = Vec::new();
    let mut lines = src.lines();

    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        // Skip doc comments and ordinary comments, which may mention `fn`.
        if trimmed.starts_with("//") {
            continue;
        }
        let Some(after_fn) = find_fn_keyword(trimmed) else {
            continue;
        };
        let name: String = after_fn
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }

        let mut signature = trimmed.to_string();
        while !signature.contains('{') {
            match lines.next() {
                Some(next) => {
                    signature.push(' ');
                    signature.push_str(next.trim());
                }
                None => break,
            }
        }
        out.push(FnDecl { name, signature });
    }
    out
}

/// The text following the `fn` keyword, if this line declares one.
fn find_fn_keyword(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix("fn ") {
        return Some(rest);
    }
    line.find(" fn ").map(|i| &line[i + 4..])
}

/// Whether a signature hands back an `Attr`, in any wrapper.
///
/// Matching on `-> Self` alone would miss `-> Option<Self>` and
/// `-> Result<Self, _>`, which are just as much constructors, so look at the
/// whole return type. Parameters come before the return type, so the text after
/// the *last* arrow is the return type even if a parameter is a closure with an
/// arrow of its own.
fn returns_attr(signature: &str) -> bool {
    match signature.rsplit_once("->") {
        Some((_, ret)) => ret.contains("Self") || ret.contains("Attr"),
        None => false,
    }
}

/// Functions that hand back an `Attr`.
fn constructor_fns(src: &str) -> Vec<FnDecl> {
    declared_fns(src)
        .into_iter()
        .filter(|f| returns_attr(&f.signature))
        .collect()
}

/// The load-bearing one. A struct literal is a safe, unchecked constructor that
/// bypasses every bound in [`CONSTRUCTORS`]; #1573 stayed exploitable for
/// exactly that reason. No field of `Attr` may be visible outside `attr.rs`.
#[test]
fn attr_fields_are_private() {
    let body = ATTR_RS
        .split_once("pub struct Attr {")
        .expect("attr.rs no longer declares `pub struct Attr`")
        .1
        .split_once("\n}")
        .expect("unterminated `struct Attr` body")
        .0;

    let public: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("pub ") || l.starts_with("pub("))
        .collect();

    assert!(
        public.is_empty(),
        "`Attr` has visible fields, so `Attr {{ .. }}` is a safe constructor \
         that skips every `Sync` check in this file — this is facet-rs/facet#1573 \
         verbatim:\n  {}",
        public.join("\n  ")
    );
}

/// No constructor may sit outside the table. This is what makes the table a
/// guard and not documentation: a new safe `-> Self` fails here on the day it
/// is written, not on the day someone races it.
#[test]
fn every_attr_constructor_is_accounted_for() {
    let unlisted: Vec<String> = constructor_fns(ATTR_RS)
        .into_iter()
        .filter(|f| !CONSTRUCTORS.iter().any(|c| c.name == f.name))
        .map(|f| f.signature)
        .collect();

    assert!(
        unlisted.is_empty(),
        "these functions build an `Attr` but are not in CONSTRUCTORS, so nothing \
         checks that their payload is `Sync`. Add them to the table with the gate \
         that makes them sound (or make them `unsafe`):\n  {}",
        unlisted.join("\n  ")
    );
}

/// ...and the table may not outlive the code, or it would silently stop
/// guarding anything.
#[test]
fn every_table_entry_still_exists() {
    let found = constructor_fns(ATTR_RS);
    let missing: Vec<&str> = CONSTRUCTORS
        .iter()
        .filter(|c| !found.iter().any(|f| f.name == c.name))
        .map(|c| c.name)
        .collect();

    assert!(
        missing.is_empty(),
        "CONSTRUCTORS lists functions that `attr.rs` no longer declares — the \
         table has rotted and is guarding less than it claims: {missing:?}"
    );
}

/// Each row's claimed gate must actually be present in the source. A row that
/// says `SyncBound` about a signature with no `Sync` in it is worse than no row
/// at all: it reads as a discharged obligation.
#[test]
fn every_constructor_matches_its_claimed_gate() {
    let found = constructor_fns(ATTR_RS);
    let failures: Vec<String> = CONSTRUCTORS
        .iter()
        .filter_map(|c| {
            let decl = found.iter().find(|f| f.name == c.name)?;
            let ok = match c.gate {
                Gate::UnsafeFn => decl.signature.contains("unsafe fn"),
                Gate::SyncBound => decl.signature.contains("Sync"),
                // Checked by type, not by text — see the companion test.
                Gate::ConcreteSyncType(_) => true,
            };
            (!ok).then(|| {
                format!(
                    "  {}: claims {:?} ({}) but its signature is `{}`",
                    c.name, c.gate, c.why, decl.signature
                )
            })
        })
        .collect();

    assert!(
        failures.is_empty(),
        "constructor gates do not match the source:\n{}",
        failures.join("\n")
    );
}

/// The `ConcreteSyncType` rows are the ones the compiler is *not* checking at
/// the constructor, so check the types here instead.
#[test]
fn concretely_gated_constructors_take_sync_types() {
    fn assert_sync<T: Sync + ?Sized>() {}

    for c in CONSTRUCTORS {
        if let Gate::ConcreteSyncType(ty) = c.gate {
            match ty {
                "Shape" => assert_sync::<Shape>(),
                other => panic!(
                    "CONSTRUCTORS names `{other}` as a statically-Sync payload type, \
                     but this test has no assertion for it — add one, do not \
                     assume it"
                ),
            }
        }
    }
}

/// Everything above rests on `returns_attr` telling constructors from
/// accessors. If it silently stopped recognising a spelling, every table test
/// would start passing vacuously — so exercise it against the spellings that
/// matter, including the wrappers `attr.rs` does not currently use.
#[test]
fn constructor_detection_recognises_every_spelling() {
    for sig in [
        "pub const unsafe fn f(data: OxRef<'static>) -> Self {",
        "pub fn f() -> Attr {",
        "pub fn f() -> Option<Self> {",
        "pub fn f() -> Result<Self, Error> {",
        "pub fn f() -> ShapeAttribute {",
        "pub fn f(g: impl Fn(u8) -> bool) -> Self {",
    ] {
        assert!(returns_attr(sig), "should be seen as a constructor: {sig}");
    }

    for sig in [
        "pub const fn ns(&self) -> Option<&'static str> {",
        "pub const fn key(&self) -> &'static str {",
        "pub const fn data(&self) -> OxRef<'static> {",
        "pub const fn is_builtin(&self) -> bool {",
        "pub fn get_as<T: Facet<'static>>(&self) -> Option<&T> {",
        "fn eq(&self, other: &Self) -> bool {",
        "fn hash<H: Hasher>(&self, state: &mut H) {",
    ] {
        assert!(!returns_attr(sig), "should not be a constructor: {sig}");
    }
}

/// The scan must find something, or the tests above are asserting over an empty
/// set.
#[test]
fn the_scan_is_not_vacuous() {
    assert_eq!(
        constructor_fns(ATTR_RS).len(),
        CONSTRUCTORS.len(),
        "expected exactly the {} constructors in CONSTRUCTORS",
        CONSTRUCTORS.len()
    );
    assert!(
        declared_fns(ATTR_RS).len() > CONSTRUCTORS.len(),
        "the fn scan found no non-constructor methods, which means it is broken"
    );
}

// -----------------------------------------------------------------------------
// Behavioural assertions — the happy path still works
// -----------------------------------------------------------------------------

/// Privatising the fields must not cost readers anything: the accessors have to
/// round-trip what the constructors put in.
#[test]
fn accessors_round_trip() {
    static DATA: u64 = 7;

    let attr = Attr::new(Some("orm"), "primary_key", &DATA);
    assert_eq!(attr.ns(), Some("orm"));
    assert_eq!(attr.key(), "primary_key");
    assert!(!attr.is_builtin());
    assert_eq!(attr.get_as::<u64>(), Some(&7));

    let builtin = Attr::new(None, "sensitive", &DATA);
    assert_eq!(builtin.ns(), None);
    assert!(builtin.is_builtin());

    // The unsafe entry point the derive macro uses, exercised end to end.
    // SAFETY: `DATA` is a `&'static u64`, which is `Sync`, and the shape
    // matches the pointer.
    let raw = unsafe {
        Attr::from_raw_parts(
            Some("orm"),
            "column",
            OxRef::new(
                PtrConst::new_sized(&DATA as *const u64),
                <u64 as Facet>::SHAPE,
            ),
        )
    };
    assert_eq!(raw.ns(), Some("orm"));
    assert_eq!(raw.key(), "column");
    assert_eq!(raw.data().shape(), <u64 as Facet>::SHAPE);
    assert_eq!(raw.get_as::<u64>(), Some(&7));
    assert!(raw.get_as::<i8>().is_none(), "shape mismatch must be None");
}

/// The #1573 access pattern with a payload that is genuinely `Sync`: many
/// threads mutating shared interior state *through* one shared `Attr`. This is
/// the reproducer's exact shape — the reproducer used `Rc`, whose refcount is a
/// plain `Cell`; here it is a `Mutex`, which is `Sync` and so has every right to
/// be there. It must stay clean under Miri, so a failure here means the race is
/// back rather than that the test is itself racy.
#[test]
fn sharing_sync_payload_across_threads_is_clean() {
    static COUNTER: Mutex<usize> = Mutex::new(0);

    let attr = Attr::new(None, "counter", &COUNTER);
    let threads = 4;
    let bumps = 25;

    thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                for _ in 0..bumps {
                    *attr
                        .get_as::<Mutex<usize>>()
                        .unwrap()
                        .lock()
                        .expect("counter mutex poisoned") += 1;
                }
            });
        }
    });

    assert_eq!(*COUNTER.lock().unwrap(), threads * bumps);
}

/// The counter-example to "then just put `Sync` on `OxRef::from_ref` too".
///
/// Reflecting over a non-`Sync`, non-`'static` local is an ordinary thing to ask
/// a reflection library for, and it is sound: `OxRef` is `!Send + !Sync` (see
/// the assertions at the top of this file), so the borrow cannot leave this
/// thread no matter what the payload is. A `Sync` bound on `from_ref` would
/// forbid this to close a hole that is not in `from_ref` — the hole is in
/// whoever asserts `Sync` over the erased payload, and the only thing in
/// facet-core that does that is `Attr`.
#[test]
fn oxref_over_non_sync_local_is_legitimate() {
    use std::rc::Rc;

    let rc = Rc::new(42_i32);
    let ox = OxRef::from_ref(&rc);

    assert_eq!(ox.shape(), <Rc<i32> as Facet>::SHAPE);
    assert_eq!(format!("{ox:?}"), "42");
    assert_eq!(
        Rc::strong_count(&rc),
        1,
        "reflection must not touch the count"
    );
}

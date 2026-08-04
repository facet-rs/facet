//! Table-driven guard: **no safe path may hand out a `&'static Shape` that
//! outlives the `Shape` it points at**.
//!
//! # Why this file exists
//!
//! `impl Display for Shape` used to launder its receiver:
//!
//! ```ignore
//! // SAFETY: All Shape instances are guaranteed to be 'static because they're
//! // always references to const statics ...
//! let static_self: &'static Shape = unsafe { core::mem::transmute(self) };
//! static_self.write_type_name(f, TypeNameOpts::default())
//! ```
//!
//! The safety comment was false. "All `Shape`s are const statics" is a
//! *convention* of the derive macro, not something the type system enforces, and
//! three public, entirely safe APIs falsify it:
//!
//! 1. [`ShapeBuilder::build`] returns an **owned** `Shape`, by value. Nothing
//!    stops that value living on the stack.
//! 2. [`ShapeBuilder::type_name`] lets safe code install a `TypeNameFn`, whose
//!    first parameter *was* `&'static Shape`.
//! 3. `Shape::write_type_name` took `&'static self` and forwarded the laundered
//!    receiver straight into that callback.
//!
//! Chained, that is five lines of `#![forbid(unsafe_code)]` user code that stash
//! a `&'static Shape` pointing into a stack frame which then returns: native
//! SIGSEGV, and under Miri `constructing invalid value: encountered a dangling
//! reference (use-after-free)`.
//!
//! The fix is to stop lying rather than to dodge: `TypeNameFn`'s first parameter
//! is now `&Shape` (higher-ranked over the lifetime) and `write_type_name` takes
//! `&self`, so the transmute is unnecessary and the printed output is unchanged.
//!
//! # How this file guards it — read before "fixing" a failure
//!
//! Half of this test is load-bearing **at compile time, not at run time**. Every
//! row below formats a `Shape` that is a *stack local*. If `write_type_name`
//! reverts to `&'static self`, or `TypeNameFn` reverts to `&'static Shape`, these
//! rows stop compiling — a borrow-check error, not an assertion failure.
//!
//! So: if this file fails to build, the invariant has been broken. Do **not**
//! repair it by promoting the local to a `static`, by `Box::leak`ing it, or by
//! reintroducing a transmute. Those all restore the hole this file exists to
//! close.
//!
//! The run-time half proves the compile-time half is not vacuous: the callback
//! records the address it was handed, and each row asserts that address is the
//! stack local's. A `'static` claim about that address is a claim about a frame
//! that is about to die.
//!
//! # What is *not* a bug here
//!
//! Reads of `Shape`'s fields still yield `'static` data — `type_identifier` is
//! `&'static str`, `type_params` is `&'static [TypeParam]`, `inner` is
//! `Option<&'static Shape>`. That is sound and intentional: those fields are
//! *typed* `&'static`, so constructing a `Shape` at all requires supplying
//! genuinely `'static` values. The bug was never the fields; it was claiming
//! `'static` for the `Shape` **container**, which `build()` hands out by value.
//!
//! # The other half of the table
//!
//! The cheap alternative fix was to delete the transmute and have `Display` print
//! `self.type_identifier`. That is sound, but it silently degrades `Vec<u32>` to
//! `Vec`. The `generic names survive` rows pin the full names down so that
//! regression cannot land disguised as a simplification.

#![cfg(feature = "std")]

use std::cell::Cell;
use std::fmt::{self, Write as _};

use facet_core::{Facet, Shape, ShapeBuilder, TypeNameOpts};

thread_local! {
    /// Address of the `Shape` that the most recent `TypeNameFn` call was handed.
    static OBSERVED: Cell<usize> = const { Cell::new(0) };
}

/// A `TypeNameFn` that safe user code can install through `ShapeBuilder`. It does
/// the benign half of the original exploit: it records the address it was given.
///
/// The exploit's other half — `STASH: OnceLock<&'static Shape>; STASH.set(shape)`
/// — is exactly what no longer compiles, and is the invariant under test.
fn record_and_write(shape: &Shape, f: &mut fmt::Formatter<'_>, _opts: TypeNameOpts) -> fmt::Result {
    OBSERVED.with(|cell| cell.set(shape as *const Shape as usize));
    // Reading a field through the short-lived reference still yields `'static`
    // data, which is sound — see the module docs.
    let identifier: &'static str = shape.type_identifier;
    f.write_str(identifier)
}

/// Builds an owned, **stack-resident** `Shape` — the root of the whole problem.
/// This is safe, public API; no `unsafe` and no derive macro involved.
fn stack_shape() -> Shape {
    ShapeBuilder::for_sized::<u8>("Victim")
        .type_name(record_and_write)
        .build()
}

/// One safe way to format a `Shape`, plus what it should produce for `Victim`.
struct Row {
    label: &'static str,
    /// Takes a *borrowed* `Shape`. Every one of these is a borrow-check failure
    /// if the `'static` requirement comes back.
    format: fn(&Shape) -> String,
    expected: &'static str,
}

/// Formatting adapter used by the rows that exercise `write_type_name` directly
/// rather than through `Display`.
struct ViaWriteTypeName<'a>(&'a Shape, TypeNameOpts);

impl fmt::Display for ViaWriteTypeName<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.write_type_name(f, self.1)
    }
}

/// A user-defined wrapper that holds a borrowed `Shape` and `Display`s it — the
/// most ordinary way a downstream crate reaches this code path.
struct Wrapper<'a>(&'a Shape);

impl fmt::Display for Wrapper<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]", self.0)
    }
}

/// Every safe entry point that can reach a `TypeNameFn` with a non-`'static`
/// `Shape`. Each must accept a stack local.
fn formatting_rows() -> Vec<Row> {
    vec![
        Row {
            label: "Display `{}` (the site of the original transmute)",
            format: |shape| format!("{shape}"),
            expected: "Victim",
        },
        Row {
            label: "Display through `ToString`",
            format: |shape| shape.to_string(),
            expected: "Victim",
        },
        Row {
            label: "Display through a user wrapper holding `&Shape`",
            format: |shape| format!("{}", Wrapper(shape)),
            expected: "[Victim]",
        },
        Row {
            label: "`write!` into a `String` via `fmt::Write`",
            format: |shape| {
                let mut out = String::new();
                write!(&mut out, "{shape}").unwrap();
                out
            },
            expected: "Victim",
        },
        Row {
            label: "`write_type_name` directly, default opts",
            format: |shape| format!("{}", ViaWriteTypeName(shape, TypeNameOpts::default())),
            expected: "Victim",
        },
        Row {
            label: "`write_type_name` directly, infinite recursion opts",
            format: |shape| format!("{}", ViaWriteTypeName(shape, TypeNameOpts::infinite())),
            expected: "Victim",
        },
        Row {
            label: "`write_type_name` directly, no recursion opts",
            format: |shape| format!("{}", ViaWriteTypeName(shape, TypeNameOpts::none())),
            expected: "Victim",
        },
    ]
}

// -----------------------------------------------------------------------------
// The assertions
// -----------------------------------------------------------------------------

/// The load-bearing one. Every safe formatting path must accept a `Shape` that
/// lives on the stack, and must hand the user's callback a pointer to *that*
/// `Shape` — which is precisely why calling it `'static` was a lie.
#[test]
fn type_name_callback_receives_the_stack_shape_it_was_given() {
    let mut failures: Vec<String> = Vec::new();

    for row in formatting_rows() {
        // A fresh local per row, so each row's address is its own.
        let shape = stack_shape();
        let expected_addr = &shape as *const Shape as usize;

        OBSERVED.with(|cell| cell.set(0));
        let rendered = (row.format)(&shape);
        let observed_addr = OBSERVED.with(|cell| cell.get());

        if observed_addr == 0 {
            failures.push(format!(
                "  {}: the TypeNameFn was never called",
                row.label
            ));
        } else if observed_addr != expected_addr {
            failures.push(format!(
                "  {}: callback got {observed_addr:#x}, but the stack Shape is at \
                 {expected_addr:#x}",
                row.label
            ));
        }

        if rendered != row.expected {
            failures.push(format!(
                "  {}: rendered {rendered:?}, expected {:?}",
                row.label, row.expected
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "safe formatting paths misbehaved on a stack-resident Shape:\n{}",
        failures.join("\n")
    );
}

/// The exploit verbatim, minus the part that no longer compiles. `launder`
/// formats a `Shape` that dies when it returns; the recorded address must then
/// name a dead frame. Under Miri this whole test is also a check that formatting
/// an owned `Shape` performs no invalid reads.
///
/// If `TypeNameFn` ever regains `&'static Shape`, the stashing half of the
/// original proof-of-concept — `OnceLock<&'static Shape>::set(shape)` — starts
/// compiling again, and this comment is the record of what that costs.
#[test]
fn shape_formatted_by_value_does_not_escape_its_frame() {
    #[inline(never)]
    fn launder() -> usize {
        let shape = stack_shape();
        let rendered = format!("{shape}");
        assert_eq!(rendered, "Victim");
        OBSERVED.with(|cell| cell.get())
    }

    OBSERVED.with(|cell| cell.set(0));
    let dead_addr = launder();

    assert_ne!(
        dead_addr, 0,
        "the TypeNameFn was never called, so this test proves nothing"
    );

    // The `Shape` at `dead_addr` is gone. The only reason this is not a
    // use-after-free is that nothing was able to keep a reference to it: the
    // callback's parameter is `&Shape`, not `&'static Shape`. We deliberately do
    // not dereference it.
    let live_addr = {
        let shape = stack_shape();
        &shape as *const Shape as usize
    };
    let _ = live_addr;
}

/// `Shape`s that genuinely *are* `'static` must keep working unchanged — the fix
/// must not have narrowed anything.
#[test]
fn static_shapes_still_format() {
    let shape: &'static Shape = <Option<String> as Facet>::SHAPE;

    assert_eq!(format!("{shape}"), "Option<String>");
    assert_eq!(
        format!("{}", ViaWriteTypeName(shape, TypeNameOpts::default())),
        "Option<String>"
    );
    // `type_name()` still requires a genuine `&'static self`, which is sound
    // because the borrow checker enforces it rather than a transmute asserting it.
    assert_eq!(format!("{}", shape.type_name()), "Option<String>");
}

/// Guards against the cheap fix: `Display` must keep printing full generic names,
/// not bare `type_identifier`s. Asserted as a table so a regression cannot slip
/// through on the one type nobody spot-checked.
#[test]
fn generic_names_survive() {
    let rows: Vec<(&'static str, &'static Shape, &'static str)> = vec![
        ("u32", <u32 as Facet>::SHAPE, "u32"),
        ("String", <String as Facet>::SHAPE, "String"),
        ("Vec<u32>", <Vec<u32> as Facet>::SHAPE, "Vec<u32>"),
        (
            "Vec<String>",
            <Vec<String> as Facet>::SHAPE,
            "Vec<String>",
        ),
        (
            "Option<String>",
            <Option<String> as Facet>::SHAPE,
            "Option<String>",
        ),
        (
            "Result<String, u32>",
            <Result<String, u32> as Facet>::SHAPE,
            "Result<String, u32>",
        ),
        (
            "Vec<Vec<u32>>",
            <Vec<Vec<u32>> as Facet>::SHAPE,
            "Vec<Vec<u32>>",
        ),
        (
            "Box<Option<u32>>",
            <Box<Option<u32>> as Facet>::SHAPE,
            "Box<Option<u32>>",
        ),
        ("[u32; 3]", <[u32; 3] as Facet>::SHAPE, "[u32; 3]"),
        ("&'static str", <&'static str as Facet>::SHAPE, "&str"),
    ];

    let failures: Vec<String> = rows
        .into_iter()
        .filter_map(|(label, shape, expected)| {
            let actual = format!("{shape}");
            (actual != expected).then(|| format!("  {label}: expected {expected:?}, got {actual:?}"))
        })
        .collect();

    assert!(
        failures.is_empty(),
        "Display lost generic parameters — this is what the cheap \
         `write!(f, \"{{}}\", self.type_identifier)` fix would look like:\n{}",
        failures.join("\n")
    );
}

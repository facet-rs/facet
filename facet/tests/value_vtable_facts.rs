use std::alloc::Layout;
use std::fmt::{self, Debug, Write};
use std::rc::Rc;
use std::{cmp::Ordering, collections::BTreeSet};

use facet_core::{Facet, PtrConst, PtrMut, PtrUninit, Shape};
use facet_macros::Facet;

/// A no-op writer used to probe whether Debug/Display are actually implemented.
struct NoopWriter;

impl Write for NoopWriter {
    fn write_str(&mut self, _s: &str) -> fmt::Result {
        Ok(())
    }
}

/// Helper to check if Debug/Display is actually implemented by trying to format.
fn probe_debug(shape: &'static Shape, ptr: PtrConst) -> bool {
    struct DebugProbe(&'static Shape, PtrConst);

    impl Debug for DebugProbe {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match unsafe { self.0.call_debug(self.1, f) } {
                Some(result) => result,
                None => Err(fmt::Error),
            }
        }
    }

    let mut writer = NoopWriter;
    write!(writer, "{:?}", DebugProbe(shape, ptr)).is_ok()
}

fn probe_display(shape: &'static Shape, ptr: PtrConst) -> bool {
    struct DisplayProbe(&'static Shape, PtrConst);

    impl fmt::Display for DisplayProbe {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match unsafe { self.0.call_display(self.1, f) } {
                Some(result) => result,
                None => Err(fmt::Error),
            }
        }
    }

    let mut writer = NoopWriter;
    write!(writer, "{}", DisplayProbe(shape, ptr)).is_ok()
}
use facet_testhelpers::test;
use owo_colors::{OwoColorize, Style};

const REMARKABLE: Style = Style::new().blue();

struct BoxPtrUninit {
    ptr: PtrUninit,
    layout: Layout,
    shape: &'static Shape,
}

impl BoxPtrUninit {
    // This has a `?Sized` bound to be usable in generic contexts.
    // This will panic when `T` is not `Sized`.
    fn new_sized<'a, T: Facet<'a> + ?Sized>() -> Self {
        let layout = T::SHAPE.layout.sized_layout().expect("T must be Sized");

        let ptr = if layout.size() == 0 {
            core::ptr::without_provenance_mut(layout.align())
        } else {
            // SAFETY: size is non-zero
            unsafe { std::alloc::alloc(layout) }
        };

        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout)
        }

        let ptr = PtrUninit::new(ptr);
        Self {
            ptr,
            layout,
            shape: T::SHAPE,
        }
    }

    unsafe fn assume_init(self) -> BoxPtrMut {
        let r = BoxPtrMut {
            ptr: unsafe { self.ptr.assume_init() },
            layout: self.layout,
            shape: self.shape,
        };
        core::mem::forget(self);
        r
    }
}

impl Drop for BoxPtrUninit {
    fn drop(&mut self) {
        if self.layout.size() > 0 {
            unsafe { std::alloc::dealloc(self.ptr.as_mut_byte_ptr(), self.layout) };
        }
    }
}

struct BoxPtrMut {
    ptr: PtrMut,
    layout: Layout,
    shape: &'static Shape,
}

impl Drop for BoxPtrMut {
    fn drop(&mut self) {
        // Some types (like references) don't have drop_in_place, which is fine
        let _ = unsafe { self.shape.call_drop_in_place(self.ptr) };
        if self.layout.size() > 0 {
            unsafe { std::alloc::dealloc(self.ptr.as_mut_byte_ptr(), self.layout) };
        }
    }
}

struct VTableValueView<'a, T: ?Sized>(&'a T);

impl<'f, 'a, T: Facet<'f> + ?Sized> Display for VTableValueView<'a, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let ptr = PtrConst::new(core::ptr::from_ref(self.0));
        match unsafe { T::SHAPE.call_display(ptr, f) } {
            Some(result) => result,
            None => write!(f, "???"),
        }
    }
}

impl<'f, 'a, T: Facet<'f> + ?Sized> Debug for VTableValueView<'a, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let ptr = PtrConst::new(core::ptr::from_ref(self.0));
        match unsafe { T::SHAPE.call_debug(ptr, f) } {
            Some(result) => result,
            None => write!(f, "???"),
        }
    }
}

unsafe fn debug(shape: &'static Shape, ptr: PtrConst) -> impl Debug {
    struct Debugger(&'static Shape, PtrConst);

    impl Debug for Debugger {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match unsafe { self.0.call_debug(self.1, f) } {
                Some(result) => result,
                None => write!(f, "???"),
            }
        }
    }

    Debugger(shape, ptr)
}

fn ord_str(ordering: Option<Ordering>) -> &'static str {
    match ordering {
        Some(Ordering::Less) => "<",
        Some(Ordering::Equal) => "==",
        Some(Ordering::Greater) => ">",
        None => "??",
    }
}

fn collect_facts<'f, 'a, T>(val1: &'a T, val2: &'a T) -> BTreeSet<Fact>
where
    T: Facet<'f> + ?Sized,
{
    let mut facts: BTreeSet<Fact> = BTreeSet::new();
    let shape = T::SHAPE;

    // For VTableIndirect (auto_traits), the function pointers exist but may return None
    // at runtime. We need to actually try calling them to know if the trait is supported.
    let ptr1 = PtrConst::new(core::ptr::from_ref(val1));
    let ptr2 = PtrConst::new(core::ptr::from_ref(val2));

    // Check which traits are actually available by trying to call them
    // For Debug/Display we need to use a probe that goes through the Formatter API
    let has_debug = probe_debug(shape, ptr1);
    let has_display = probe_display(shape, ptr1);
    let has_partial_eq = unsafe { shape.call_partial_eq(ptr1, ptr2) }.is_some();
    let has_ord = unsafe { shape.call_cmp(ptr1, ptr2) }.is_some();
    let has_partial_ord = unsafe { shape.call_partial_cmp(ptr1, ptr2) }.is_some();

    // For default and clone, we need to allocate - check via type_ops flags first
    let has_default = shape
        .type_ops
        .map(|ops| ops.has_default_in_place())
        .unwrap_or(false)
        && {
            let ptr = BoxPtrUninit::new_sized::<T>();
            let result = unsafe { shape.call_default_in_place(ptr.ptr.assume_init()) };
            if result.is_some() {
                // Drop the allocated value
                let _ = unsafe { ptr.assume_init() };
                true
            } else {
                false
            }
        };

    let has_clone = shape
        .type_ops
        .map(|ops| ops.has_clone_into())
        .unwrap_or(false)
        && {
            let ptr = BoxPtrUninit::new_sized::<T>();
            let result = unsafe { shape.call_clone_into(ptr1, ptr.ptr.assume_init()) };
            if result.is_some() {
                // Drop the allocated value
                let _ = unsafe { ptr.assume_init() };
                true
            } else {
                false
            }
        };

    let traits = [
        ("Debug", has_debug),
        ("Display", has_display),
        ("Default", has_default),
        ("PartialEq", has_partial_eq),
        ("Ord", has_ord),
        ("PartialOrd", has_partial_ord),
        ("Clone", has_clone),
    ];
    let trait_str = traits
        .iter()
        .filter_map(|(name, has_impl)| {
            if *has_impl {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(" + ");
    eprintln!("{} {}", trait_str, "======".yellow());

    let l = VTableValueView(val1);
    let r = VTableValueView(val2);

    // Format display representation
    if has_display {
        facts.insert(Fact::Display);
        eprintln!(
            "Display:    {}",
            format_args!("{} vs {}", l.style(REMARKABLE), r.style(REMARKABLE))
        );
    }

    // Format debug representation
    if has_debug {
        facts.insert(Fact::Debug);
        eprintln!(
            "Debug:      {}",
            format_args!("{:?} vs {:?}", l.style(REMARKABLE), r.style(REMARKABLE))
        );
    }

    // Test equality
    if has_partial_eq {
        let eq_result = unsafe { shape.call_partial_eq(ptr1, ptr2) }.unwrap();
        facts.insert(Fact::PartialEqAnd { l_eq_r: eq_result });
        let eq_str = format!(
            "{:?} {} {:?}",
            l.style(REMARKABLE),
            if eq_result { "==" } else { "!=" }.yellow(),
            r.style(REMARKABLE),
        );
        eprintln!("Equality:   {eq_str}");
    }

    // Test ordering
    if has_ord {
        let cmp_result = unsafe { shape.call_cmp(ptr1, ptr2) }.unwrap();
        facts.insert(Fact::OrdAnd {
            l_ord_r: cmp_result,
        });
        let cmp_str = format!(
            "{:?} {} {:?}",
            l.style(REMARKABLE),
            ord_str(Some(cmp_result)).yellow(),
            r.style(REMARKABLE),
        );
        eprintln!("PartialOrd: {cmp_str}");
    }

    if has_partial_ord {
        let cmp_result = unsafe { shape.call_partial_cmp(ptr1, ptr2) }.unwrap();
        facts.insert(Fact::PartialOrdAnd {
            l_ord_r: cmp_result,
        });
        let cmp_str = format!(
            "{:?} {} {:?}",
            l.style(REMARKABLE),
            ord_str(cmp_result).yellow(),
            r.style(REMARKABLE),
        );
        eprintln!("Ord:        {cmp_str}");
    }

    // Test default_in_place
    if has_default {
        facts.insert(Fact::Default);

        let ptr = BoxPtrUninit::new_sized::<T>();
        unsafe { shape.call_default_in_place(ptr.ptr.assume_init()) };
        let ptr = unsafe { ptr.assume_init() };
        let debug = unsafe { debug(shape, ptr.ptr.as_const()) };
        eprintln!(
            "Default:    {}",
            format_args!("{debug:?}").style(REMARKABLE)
        );
    }

    // Test clone
    if has_clone {
        facts.insert(Fact::Clone);

        let src_ptr = PtrConst::new(core::ptr::from_ref(val1));

        let ptr = BoxPtrUninit::new_sized::<T>();
        unsafe { shape.call_clone_into(src_ptr, ptr.ptr.assume_init()) };
        let ptr = unsafe { ptr.assume_init() };
        let debug = unsafe { debug(shape, ptr.ptr.as_const()) };
        eprintln!(
            "Clone:      {}",
            format_args!("{debug:?}").style(REMARKABLE)
        );
    }

    facts
}

#[track_caller]
fn report_maybe_mismatch<'f, 'a, T>(
    val1: &'a T,
    val2: &'a T,
    expected_facts: BTreeSet<Fact>,
    facts: BTreeSet<Fact>,
) where
    T: Facet<'f> + ?Sized,
{
    let name = format!("{}", T::SHAPE);

    let expected_minus_actual: BTreeSet<_> = expected_facts.difference(&facts).collect();
    let actual_minus_expected: BTreeSet<_> = facts.difference(&expected_facts).collect();

    let l = VTableValueView(val1);
    let r = VTableValueView(val2);

    assert!(
        expected_facts == facts,
        "{} for {}: ({:?} vs {:?})\n{}\n{}",
        "Facts mismatch".red().bold(),
        name.style(REMARKABLE),
        l.red(),
        r.blue(),
        expected_minus_actual
            .iter()
            .map(|f| format!("- {f}"))
            .collect::<Vec<_>>()
            .join("\n")
            .yellow(),
        actual_minus_expected
            .iter()
            .map(|f| format!("+ {f}"))
            .collect::<Vec<_>>()
            .join("\n")
            .yellow(),
    );
}

#[track_caller]
fn check_facts<'f, 'a, T>(val1: &'a T, val2: &'a T, expected_facts: BTreeSet<Fact>)
where
    T: Facet<'f> + ?Sized,
{
    let name = format!("{}", T::SHAPE);
    eprint!("{}", format_args!("== {name}: ").yellow());

    let facts = collect_facts(val1, val2);

    report_maybe_mismatch(val1, val2, expected_facts, facts);
}

// slightly different version to overwrite the equality parts as miri juggles the addresses
#[cfg(feature = "fn-ptr")]
#[track_caller]
fn check_facts_no_cmp<'f, 'a, T>(val1: &'a T, val2: &'a T, expected_facts: BTreeSet<Fact>)
where
    T: Facet<'f> + ?Sized,
{
    #[cfg(not(miri))]
    {
        check_facts(val1, val2, expected_facts)
    }

    #[cfg(miri)]
    {
        let mut expected_facts = expected_facts;
        let name = format!("{}", T::SHAPE);
        eprint!("{}", format_args!("== {name}: ").yellow());

        let facts = collect_facts(val1, val1);
        for &fact in facts.iter() {
            if let Fact::PartialEqAnd { .. } | Fact::OrdAnd { .. } | Fact::PartialOrdAnd { .. } =
                fact
            {
                expected_facts.insert(fact);
            }
        }

        report_maybe_mismatch(val1, val2, expected_facts, facts);
    }
}

#[derive(Default)]
pub struct FactBuilder {
    has_debug: bool,
    has_display: bool,
    has_partial_eq_and: Option<bool>,
    has_ord_and: Option<Ordering>,
    has_partial_ord_and: Option<Option<Ordering>>,
    has_default: bool,
    has_clone: bool,
}

impl FactBuilder {
    fn new() -> Self {
        Default::default()
    }

    fn debug(mut self) -> Self {
        self.has_debug = true;
        self
    }

    fn display(mut self) -> Self {
        self.has_display = true;
        self
    }

    fn partial_eq_and(mut self, l_eq_r: bool) -> Self {
        self.has_partial_eq_and = Some(l_eq_r);
        self
    }

    fn correct_ord_and(self, l_ord_r: Ordering) -> Self {
        self.ord_and(l_ord_r).partial_ord_and(Some(l_ord_r))
    }

    fn ord_and(mut self, l_ord_r: Ordering) -> Self {
        self.has_ord_and = Some(l_ord_r);
        self
    }

    fn partial_ord_and(mut self, l_ord_r: Option<Ordering>) -> Self {
        self.has_partial_ord_and = Some(l_ord_r);
        self
    }

    fn default(mut self) -> Self {
        self.has_default = true;
        self
    }

    fn clone(mut self) -> Self {
        self.has_clone = true;
        self
    }

    fn build(self) -> BTreeSet<Fact> {
        let mut facts = BTreeSet::new();
        if self.has_debug {
            facts.insert(Fact::Debug);
        }
        if self.has_display {
            facts.insert(Fact::Display);
        }
        if let Some(l_eq_r) = self.has_partial_eq_and {
            facts.insert(Fact::PartialEqAnd { l_eq_r });
        }
        if let Some(l_ord_r) = self.has_ord_and {
            facts.insert(Fact::OrdAnd { l_ord_r });
        }
        if let Some(l_ord_r) = self.has_partial_ord_and {
            facts.insert(Fact::PartialOrdAnd { l_ord_r });
        }
        if self.has_default {
            facts.insert(Fact::Default);
        }
        if self.has_clone {
            facts.insert(Fact::Clone);
        }
        facts
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Fact {
    Debug,
    Display,
    PartialEqAnd { l_eq_r: bool },
    OrdAnd { l_ord_r: Ordering },
    PartialOrdAnd { l_ord_r: Option<Ordering> },
    Default,
    Clone,
}

use core::fmt::{Display, Formatter, Result};

impl Display for Fact {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Fact::Debug => write!(f, "impl Debug"),
            Fact::Display => write!(f, "impl Display"),
            Fact::PartialEqAnd { l_eq_r } => write!(
                f,
                "impl Equal and l {} r",
                if *l_eq_r { "==" } else { "!=" }
            ),
            Fact::OrdAnd { l_ord_r } => {
                let ord_str = match l_ord_r {
                    Ordering::Less => "<",
                    Ordering::Equal => "==",
                    Ordering::Greater => ">",
                };
                write!(f, "impl Ord and l {ord_str} r")
            }
            Fact::PartialOrdAnd { l_ord_r } => {
                let ord_str = match l_ord_r {
                    Some(Ordering::Less) => "<",
                    Some(Ordering::Equal) => "==",
                    Some(Ordering::Greater) => ">",
                    None => "??",
                };
                write!(f, "impl PartialOrd and l {ord_str} r")
            }
            Fact::Default => write!(f, "impl Default"),
            Fact::Clone => write!(f, "impl Clone"),
        }
    }
}

#[test]
fn test_integer_traits() {
    // i32 implements Debug, PartialEq, and Ord
    check_facts(
        &42,
        &24,
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Greater)
            .default()
            .clone()
            .build(),
    );

    // Test equal i32 values
    check_facts(
        &42,
        &42,
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .default()
            .clone()
            .build(),
    );

    // Test i32::MIN and i32::MAX
    check_facts(
        &i32::MIN,
        &i32::MAX,
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .default()
            .clone()
            .build(),
    );

    // Test i32 with 0
    check_facts(
        &0,
        &42,
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .default()
            .clone()
            .build(),
    );

    // Test negative i32 values
    check_facts(
        &-10,
        &10,
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .default()
            .clone()
            .build(),
    );
}

#[test]
fn test_boolean_traits() {
    // bool implements Debug, PartialEq, Ord, and Display
    check_facts(
        &true,
        &false,
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Greater)
            .default()
            .clone()
            .build(),
    );

    check_facts(
        &true,
        &true,
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .default()
            .clone()
            .build(),
    );

    check_facts(
        &false,
        &true,
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .default()
            .clone()
            .build(),
    );

    check_facts(
        &false,
        &false,
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .default()
            .clone()
            .build(),
    );
}

#[test]
fn test_floating_traits() {
    // f64 implements Debug, PartialEq
    check_facts(
        &3.18,
        &2.71,
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .partial_ord_and(Some(Ordering::Greater))
            .default()
            .clone()
            .build(),
    );

    check_facts(
        &f64::NAN,
        &1.0,
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .partial_ord_and(None)
            .default()
            .clone()
            .build(),
    );

    check_facts(
        &f64::NAN,
        &f64::NAN,
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .partial_ord_and(None)
            .default()
            .clone()
            .build(),
    );
}

#[test]
fn test_string_traits() {
    // String implements Debug, PartialEq, and Ord
    check_facts(
        &String::from("hello"),
        &String::from("world"),
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .default()
            .clone()
            .build(),
    );

    // &str implements Debug, PartialEq, and Ord
    check_facts(
        &"hello",
        &"world",
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .clone()
            .build(),
    );

    // Cow<'a, str> implements Debug, PartialEq, and Ord
    use std::borrow::Cow;
    check_facts(
        &Cow::Borrowed("hello"),
        &Cow::Borrowed("world"),
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .clone()
            .default()
            .build(),
    );
    check_facts(
        &Cow::<str>::Owned("hello".to_string()),
        &Cow::<str>::Owned("world".to_string()),
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .clone()
            .default()
            .build(),
    );
    check_facts(
        &Cow::Borrowed("same"),
        &Cow::Owned("same".to_string()),
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .clone()
            .default()
            .build(),
    );
}

#[test]
fn test_str_traits() {
    check_facts(
        "abc",
        "abc",
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .build(),
    );

    check_facts(
        "abc",
        "def",
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .build(),
    );

    let s = String::from("abc");
    let s = s.as_str();

    check_facts(
        s,
        s,
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .build(),
    );
}

#[test]
fn test_slice_traits() {
    check_facts(
        &[1, 2, 3][..],
        &[4, 5, 6][..],
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .build(),
    );

    check_facts(
        &["hello", "world"][..],
        &["foo", "bar"][..],
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Greater)
            .build(),
    );
}

#[test]
fn test_slice_ref_traits() {
    // &[i32] implements Debug, PartialEq, and Ord
    check_facts(
        &&[1, 2, 3][..],
        &&[4, 5, 6][..],
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .clone()
            .build(),
    );

    // &[&str] implements Debug, PartialEq, and Ord
    check_facts(
        &&["hello", "world"][..],
        &&["foo", "bar"][..],
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Greater)
            .clone()
            .build(),
    );
}

#[test]
fn test_array_traits() {
    // [i32; 1] implements Debug, PartialEq, Ord, Default, and Clone
    check_facts(
        &[42],
        &[24],
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Greater)
            .default()
            .clone()
            .build(),
    );
}

#[test]
fn test_vecs() {
    check_facts(
        &vec![1, 2, 3],
        &vec![4, 5, 6],
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .default()
            .build(),
    );
}

#[test]
fn test_custom_structs() {
    // Struct with no trait implementations
    #[derive(Facet)]
    #[facet(auto_traits)]
    struct StructNoTraits {
        value: i32,
    }
    check_facts(
        &StructNoTraits { value: 42 },
        &StructNoTraits { value: 24 },
        FactBuilder::new().build(),
    );

    // Struct with Debug only
    #[derive(Facet, Debug)]
    #[facet(auto_traits)]
    struct StructDebug {
        value: i32,
    }
    check_facts(
        &StructDebug { value: 42 },
        &StructDebug { value: 24 },
        FactBuilder::new().debug().build(),
    );

    // Struct with Debug and PartialEq
    #[derive(Facet, Debug, PartialEq)]
    #[facet(auto_traits)]
    struct StructDebugEq {
        value: i32,
    }
    check_facts(
        &StructDebugEq { value: 42 },
        &StructDebugEq { value: 24 },
        FactBuilder::new().debug().partial_eq_and(false).build(),
    );

    // Struct with all traits
    #[derive(Facet, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
    #[facet(auto_traits)]
    struct StructAll {
        value: i32,
    }
    check_facts(
        &StructAll { value: 42 },
        &StructAll { value: 24 },
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Greater)
            .clone()
            .build(),
    );
    check_facts(
        &StructAll { value: 10 },
        &StructAll { value: 90 },
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .clone()
            .build(),
    );
    check_facts(
        &StructAll { value: 69 },
        &StructAll { value: 69 },
        FactBuilder::new()
            .debug()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .clone()
            .build(),
    );
}

#[test]
fn test_tuple_structs() {
    // Tuple struct with no trait implementations
    #[derive(Facet)]
    #[facet(auto_traits)]
    #[allow(dead_code)]
    struct TupleNoTraits(i32, String);
    check_facts(
        &TupleNoTraits(42, "Hello".to_string()),
        &TupleNoTraits(24, "World".to_string()),
        FactBuilder::new().build(),
    );

    // Tuple struct with Debug only
    #[derive(Facet, Debug)]
    #[facet(auto_traits)]
    #[allow(dead_code)]
    struct TupleDebug(i32, String);
    check_facts(
        &TupleDebug(42, "Hello".to_string()),
        &TupleDebug(24, "World".to_string()),
        FactBuilder::new().debug().build(),
    );

    // Tuple struct with EQ only
    #[derive(Facet, PartialEq)]
    #[facet(auto_traits)]
    struct TupleEq(i32, String);
    check_facts(
        &TupleEq(42, "Hello".to_string()),
        &TupleEq(24, "World".to_string()),
        FactBuilder::new().partial_eq_and(false).build(),
    );

    // Tuple struct with all traits
    #[derive(Facet, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
    #[facet(auto_traits)]
    struct TupleAll(i32, String);
    check_facts(
        &TupleAll(42, "Hello".to_string()),
        &TupleAll(24, "World".to_string()),
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Greater)
            .clone()
            .build(),
    );
}

#[test]
fn test_enums() {
    #[derive(Facet, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
    #[facet(auto_traits)]
    #[repr(u8)]
    enum TestEnum {
        Variant1,
        Variant2(i32),
        Variant3 { field: String },
    }

    // Unit variant with equal values
    check_facts(
        &TestEnum::Variant1,
        &TestEnum::Variant1,
        FactBuilder::new()
            .debug()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .clone()
            .build(),
    );

    // Tuple variant with different values
    check_facts(
        &TestEnum::Variant2(42),
        &TestEnum::Variant2(24),
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Greater)
            .clone()
            .build(),
    );

    // Struct variant with different values
    check_facts(
        &TestEnum::Variant3 {
            field: "Hello".to_string(),
        },
        &TestEnum::Variant3 {
            field: "World".to_string(),
        },
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .clone()
            .build(),
    );
}

#[test]
fn test_weird_cmp() {
    #[derive(Facet)]
    #[facet(auto_traits)]
    struct WeirdCmp;

    impl PartialEq for WeirdCmp {
        fn eq(&self, _: &Self) -> bool {
            false
        }

        #[allow(clippy::partialeq_ne_impl)]
        fn ne(&self, _: &Self) -> bool {
            false
        }
    }

    impl Eq for WeirdCmp {}

    #[allow(clippy::non_canonical_partial_ord_impl)]
    impl PartialOrd for WeirdCmp {
        fn partial_cmp(&self, _: &Self) -> Option<Ordering> {
            Some(Ordering::Equal)
        }
    }

    impl Ord for WeirdCmp {
        fn cmp(&self, _: &Self) -> Ordering {
            Ordering::Greater
        }
    }

    check_facts(
        &WeirdCmp,
        &WeirdCmp,
        FactBuilder::new()
            .partial_eq_and(false)
            .partial_ord_and(Some(Ordering::Equal))
            .ord_and(Ordering::Greater)
            .build(),
    );
}

#[test]
#[cfg(feature = "fn-ptr")]
fn test_fn_ptr() {
    let c = |_: u32| -> u32 { 0 };
    let c = c as fn(_) -> _;

    check_facts_no_cmp::<fn(u32) -> u32>(
        &c,
        &c,
        FactBuilder::new()
            .debug()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .clone()
            .build(),
    );

    extern "C" fn foo(_: usize) -> u32 {
        0
    }

    let foo = foo as extern "C" fn(_) -> _;

    check_facts_no_cmp::<extern "C" fn(usize) -> u32>(
        &foo,
        &foo,
        FactBuilder::new()
            .debug()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .clone()
            .build(),
    );

    let l = (|_| 0) as fn(_) -> _;
    let r = (|_| 1) as fn(_) -> _;
    check_facts_no_cmp::<fn(u32) -> u32>(
        &l,
        &r,
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(
                #[allow(unpredictable_function_pointer_comparisons)]
                l.cmp(&r),
            )
            .clone()
            .build(),
    );
}

#[test]
fn test_ptr() {
    let unit = ();
    let ptr = &raw const unit;

    check_facts(
        &ptr,
        &ptr,
        FactBuilder::new()
            .clone()
            .debug()
            .partial_eq_and(true)
            .ord_and(Ordering::Equal)
            .partial_ord_and(Some(Ordering::Equal))
            .build(),
    );

    check_facts(
        &ptr.cast_mut(),
        &ptr.cast_mut(),
        FactBuilder::new()
            .clone()
            .debug()
            .partial_eq_and(true)
            .ord_and(Ordering::Equal)
            .partial_ord_and(Some(Ordering::Equal))
            .build(),
    );

    let s = "abc";
    let ptr = core::ptr::from_ref(s);

    check_facts(
        &ptr,
        &ptr,
        FactBuilder::new()
            .clone()
            .debug()
            .partial_eq_and(true)
            .ord_and(Ordering::Equal)
            .partial_ord_and(Some(Ordering::Equal))
            .build(),
    );

    check_facts(
        &ptr.cast_mut(),
        &ptr.cast_mut(),
        FactBuilder::new()
            .clone()
            .debug()
            .partial_eq_and(true)
            .ord_and(Ordering::Equal)
            .partial_ord_and(Some(Ordering::Equal))
            .build(),
    );

    check_facts(
        &ptr,
        &&raw const s[..1],
        FactBuilder::new()
            .clone()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(ptr.cmp(&&raw const s[..1]))
            .build(),
    );

    check_facts(
        &ptr.cast_mut(),
        &core::ptr::from_ref(&s[..1]).cast_mut(),
        FactBuilder::new()
            .clone()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(ptr.cmp(&&raw const s[..1]))
            .build(),
    );
}

#[test]
fn test_ref() {
    check_facts(
        &&(),
        &&(),
        FactBuilder::new()
            .debug()
            .partial_eq_and(true)
            .ord_and(Ordering::Equal)
            .partial_ord_and(Some(Ordering::Equal))
            .clone()
            .build(),
    );

    let unit = ();
    let ptr = &raw const unit;

    check_facts(
        &&ptr,
        &&ptr,
        FactBuilder::new()
            .debug()
            .partial_eq_and(true)
            .ord_and(Ordering::Equal)
            .partial_ord_and(Some(Ordering::Equal))
            .clone()
            .build(),
    );
}

#[test]
fn test_mut_ref() {
    check_facts(
        &&mut (),
        &&mut (),
        FactBuilder::new()
            .debug()
            .partial_eq_and(true)
            .ord_and(Ordering::Equal)
            .partial_ord_and(Some(Ordering::Equal))
            .build(),
    );

    let unit = ();
    let mut ptr1 = &raw const unit;
    let mut ptr2 = &raw const unit;
    let ref1 = &mut ptr1;
    let ref2 = &mut ptr2;

    check_facts(
        &ref1,
        &ref2,
        FactBuilder::new()
            .debug()
            .partial_eq_and(true)
            .ord_and(Ordering::Equal)
            .partial_ord_and(Some(Ordering::Equal))
            .build(),
    );
}

#[test]
fn test_rc_weak() {
    let v = Rc::new(());
    let mut w1 = Rc::downgrade(&v);
    let mut w2 = Rc::downgrade(&v);

    check_facts(
        &w1,
        &w2,
        FactBuilder::new().clone().debug().default().build(),
    );

    check_facts(&&w1, &&w2, FactBuilder::new().clone().debug().build());

    check_facts(&&mut w1, &&mut w2, FactBuilder::new().debug().build());

    let ptr = &raw const w1;

    check_facts(
        &ptr,
        &ptr,
        FactBuilder::new()
            .clone()
            .debug()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .build(),
    );

    check_facts(
        &ptr.cast_mut(),
        &ptr.cast_mut(),
        FactBuilder::new()
            .clone()
            .debug()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .build(),
    );
}

#[test]
#[cfg(feature = "net")]
fn test_ipv4_addr_parse_from_str() {
    use core::net::Ipv4Addr;
    use facet_reflect::Partial;

    // Test that Ipv4Addr can be parsed from a string using facet reflection
    // SAFETY: Ipv4Addr::SHAPE comes from the trusted Facet implementation for Ipv4Addr
    let wip = unsafe { Partial::alloc_shape(Ipv4Addr::SHAPE) }.unwrap();

    // This should work - parse a valid IP address
    let wip = wip
        .parse_from_str("127.0.0.1")
        .expect("Failed to parse valid IP address");

    let value: Ipv4Addr = wip.build().unwrap().materialize().unwrap();
    assert_eq!(value, "127.0.0.1".parse::<Ipv4Addr>().unwrap());

    // Test that invalid IP addresses fail to parse
    // SAFETY: Ipv4Addr::SHAPE comes from the trusted Facet implementation for Ipv4Addr
    let wip2 = unsafe { Partial::alloc_shape(Ipv4Addr::SHAPE) }.unwrap();
    let result2 = wip2.parse_from_str("not.an.ip.address");
    assert!(result2.is_err(), "Should fail to parse invalid IP address");

    // Test that Ipv4Addr shape indicates it supports parsing
    let shape = Ipv4Addr::SHAPE;
    assert!(
        shape.is_from_str(),
        "Ipv4Addr should support parsing from string"
    );

    // Check that the vtable has a parse function
    assert!(
        shape.vtable.has_parse(),
        "Ipv4Addr should have a parse function in vtable"
    );
}

#[test]
#[cfg(feature = "num-complex")]
fn test_complex_default() {
    use num_complex::Complex;

    // Test Complex<f64> - it has default_in_place because f64 does
    check_facts(
        &Complex::new(1.0f64, 2.0f64),
        &Complex::new(3.0f64, 4.0f64),
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .default()
            .build(),
    );

    // Test Complex<f32>
    check_facts(
        &Complex::new(1.0f32, 2.0f32),
        &Complex::new(1.0f32, 2.0f32),
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(true)
            .default()
            .build(),
    );
}

#[test]
fn test_ref_clone_debug() {
    let shape = <&()>::SHAPE;
    eprintln!("Shape for &(): {}", shape);
    let has_clone = shape
        .type_ops
        .map(|ops| ops.has_clone_into())
        .unwrap_or(false);
    eprintln!("type_ops has_clone_into: {}", has_clone);

    let unit = ();
    let ref_to_unit: &() = &unit;
    let ptr = PtrConst::new(&ref_to_unit as *const &());

    // Try to clone
    let mut dst: std::mem::MaybeUninit<&()> = std::mem::MaybeUninit::uninit();
    let dst_ptr = PtrMut::new(dst.as_mut_ptr());

    let result = unsafe { shape.call_clone_into(ptr, dst_ptr) };
    eprintln!("call_clone_into result: {:?}", result);

    assert!(
        shape
            .type_ops
            .map(|ops| ops.has_clone_into())
            .unwrap_or(false),
        "REF type_ops should have clone_into"
    );
    assert!(result.is_some(), "clone_into should succeed");
}

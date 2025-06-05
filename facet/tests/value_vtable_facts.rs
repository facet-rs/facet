use std::alloc::Layout;
use std::fmt::Debug;
use std::net::Ipv4Addr;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::rc::Rc;
use std::{cmp::Ordering, collections::BTreeSet, marker::PhantomData};

use facet_core::{
    DropInPlaceFn, Facet, MarkerTraits, PtrConst, PtrMut, PtrUninit, VTableView, ValueVTable,
};
use facet_macros::Facet;
use facet_testhelpers::test;
use owo_colors::{OwoColorize, Style};

const REMARKABLE: Style = Style::new().blue();

struct BoxPtrUninit<'mem> {
    ptr: PtrUninit<'mem>,
    layout: Layout,
    drop_in_place: Option<DropInPlaceFn>,
}

impl<'mem> BoxPtrUninit<'mem> {
    // This has a `?Sized` bound to be usable in generic contexts.
    // This will panic when `T` is not `Sized`.
    fn new_sized<'a, T: Facet<'a> + ?Sized>() -> Self {
        let layout = T::SHAPE.layout.sized_layout().expect("T must be Sized");
        let drop_in_place = T::VTABLE.sized().and_then(|v| (v.drop_in_place)());

        let ptr = if layout.size() == 0 {
            core::ptr::without_provenance_mut(layout.align())
        } else {
            // SAFETY: size is non-zero
            unsafe { std::alloc::alloc(layout) }
        };

        let ptr = PtrUninit::new(ptr);
        Self {
            ptr,
            layout,
            drop_in_place,
        }
    }

    unsafe fn assume_init(self) -> BoxPtrMut<'mem> {
        let r = BoxPtrMut {
            ptr: unsafe { self.ptr.assume_init() },
            layout: self.layout,
            drop_in_place: self.drop_in_place,
        };
        core::mem::forget(self);
        r
    }
}

impl<'mem> Drop for BoxPtrUninit<'mem> {
    fn drop(&mut self) {
        if self.layout.size() > 0 {
            unsafe { std::alloc::dealloc(self.ptr.as_mut_byte_ptr(), self.layout) };
        }
    }
}

struct BoxPtrMut<'mem> {
    ptr: PtrMut<'mem>,
    layout: Layout,
    drop_in_place: Option<DropInPlaceFn>,
}

impl<'mem> Drop for BoxPtrMut<'mem> {
    fn drop(&mut self) {
        if let Some(drop_in_place) = self.drop_in_place {
            unsafe { drop_in_place(self.ptr) };
        }
        if self.layout.size() > 0 {
            unsafe { std::alloc::dealloc(self.ptr.as_mut_byte_ptr(), self.layout) };
        }
    }
}

struct VTableValueView<'a, T: ?Sized>(&'a T);

impl<'a, 'facet, T: Facet<'facet> + ?Sized> VTableValueView<'a, T> {
    fn view() -> VTableView<T> {
        VTableView::of()
    }
}

impl<'a, 'facet, T: Facet<'facet> + ?Sized> Display for VTableValueView<'a, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match Self::view().display() {
            Some(fun) => fun(self.0, f),
            None => write!(f, "???"),
        }
    }
}

impl<'a, 'facet, T: Facet<'facet> + ?Sized> Debug for VTableValueView<'a, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match Self::view().debug() {
            Some(fun) => fun(self.0, f),
            None => write!(f, "???"),
        }
    }
}

unsafe fn debug(vtable: &'static ValueVTable, ptr: PtrConst) -> impl Debug {
    struct Debugger<'a>(&'static ValueVTable, PtrConst<'a>);

    impl<'a> Debug for Debugger<'a> {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match self.0.sized().and_then(|v| (v.display)()) {
                Some(fun) => unsafe { fun(self.1, f) },
                None => write!(f, "???"),
            }
        }
    }

    Debugger(vtable, ptr)
}

fn ord_str(ordering: Option<Ordering>) -> &'static str {
    match ordering {
        Some(Ordering::Less) => "<",
        Some(Ordering::Equal) => "==",
        Some(Ordering::Greater) => ">",
        None => "??",
    }
}

fn collect_facts<'a, T>(val1: &T, val2: &T) -> BTreeSet<Fact>
where
    T: Facet<'a> + ?Sized,
{
    let mut facts: BTreeSet<Fact> = BTreeSet::new();
    let value_vtable = T::SHAPE.vtable;
    let traits = [
        (
            "Debug",
            value_vtable.sized().and_then(|v| (v.debug)()).is_some(),
        ),
        (
            "Display",
            value_vtable.sized().and_then(|v| (v.display)()).is_some(),
        ),
        (
            "Default",
            value_vtable
                .sized()
                .and_then(|v| (v.default_in_place)())
                .is_some(),
        ),
        (
            "PartialEq",
            value_vtable
                .sized()
                .and_then(|v| (v.partial_eq)())
                .is_some(),
        ),
        (
            "Ord",
            value_vtable.sized().and_then(|v| (v.ord)()).is_some(),
        ),
        (
            "PartialOrd",
            value_vtable
                .sized()
                .and_then(|v| (v.partial_ord)())
                .is_some(),
        ),
        (
            "Clone",
            value_vtable
                .sized()
                .and_then(|v| (v.clone_into)())
                .is_some(),
        ),
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

    let vtable = VTableView::<T>::of();
    let l = VTableValueView(val1);
    let r = VTableValueView(val2);

    // Format display representation
    if vtable.display().is_some() {
        facts.insert(Fact::Display);
        eprintln!(
            "Display:    {}",
            format_args!("{} vs {}", l.style(REMARKABLE), r.style(REMARKABLE))
        );
    }

    // Format debug representation
    if vtable.debug().is_some() {
        facts.insert(Fact::Debug);
        eprintln!(
            "Debug:      {}",
            format_args!("{:?} vs {:?}", l.style(REMARKABLE), r.style(REMARKABLE))
        );
    }

    // Test equality
    if let Some(eq_fn) = vtable.partial_eq() {
        let eq_result = eq_fn(val1, val2);
        facts.insert(Fact::PartialEqAnd { l_eq_r: eq_result });
        let eq_str = format!(
            "{:?} {} {:?}",
            l.style(REMARKABLE),
            if eq_result { "==" } else { "!=" }.yellow(),
            r.style(REMARKABLE),
        );
        eprintln!("Equality:   {}", eq_str);
    }

    // Test ordering
    if let Some(cmp_fn) = vtable.ord() {
        let cmp_result = cmp_fn(val1, val2);
        facts.insert(Fact::OrdAnd {
            l_ord_r: cmp_result,
        });
        let cmp_str = format!(
            "{:?} {} {:?}",
            l.style(REMARKABLE),
            ord_str(Some(cmp_result)).yellow(),
            r.style(REMARKABLE),
        );
        eprintln!("PartialOrd: {}", cmp_str);
    }

    if let Some(cmp_fn) = vtable.partial_ord() {
        let cmp_result = cmp_fn(val1, val2);
        facts.insert(Fact::PartialOrdAnd {
            l_ord_r: cmp_result,
        });
        let cmp_str = format!(
            "{:?} {} {:?}",
            l.style(REMARKABLE),
            ord_str(cmp_result).yellow(),
            r.style(REMARKABLE),
        );
        eprintln!("Ord:        {}", cmp_str);
    }

    // Test default_in_place
    if let Some(default_in_place) = T::VTABLE.sized().and_then(|v| (v.default_in_place)()) {
        facts.insert(Fact::Default);

        let ptr = BoxPtrUninit::new_sized::<T>();

        unsafe { default_in_place(ptr.ptr) };
        let ptr = unsafe { ptr.assume_init() };
        let debug = unsafe { debug(T::VTABLE, ptr.ptr.as_const()) };
        eprintln!(
            "Default:    {}",
            format_args!("{:?}", debug).style(REMARKABLE)
        );
    }

    // Test clone
    if let Some(clone_into) = T::VTABLE.sized().and_then(|v| (v.clone_into)()) {
        facts.insert(Fact::Clone);

        let src_ptr = PtrConst::new(core::ptr::from_ref(val1).cast::<u8>());

        let ptr = BoxPtrUninit::new_sized::<T>();
        unsafe { clone_into(src_ptr, ptr.ptr) };
        let ptr = unsafe { ptr.assume_init() };
        let debug = unsafe { debug(T::VTABLE, ptr.ptr.as_const()) };
        eprintln!(
            "Clone:      {}",
            format_args!("{:?}", debug).style(REMARKABLE)
        );
    }

    // Marker traits
    facts.extend(T::VTABLE.marker_traits().iter().map(Fact::MarkerTrait));

    facts
}

fn report_maybe_mismatch<'a, T>(
    val1: &T,
    val2: &T,
    expected_facts: BTreeSet<Fact>,
    facts: BTreeSet<Fact>,
) where
    T: Facet<'a> + ?Sized,
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
            .map(|f| format!("- {}", f))
            .collect::<Vec<_>>()
            .join("\n")
            .yellow(),
        actual_minus_expected
            .iter()
            .map(|f| format!("+ {}", f))
            .collect::<Vec<_>>()
            .join("\n")
            .yellow(),
    );
}

fn check_facts<'a, 'b: 'a, T>(
    val1: &'b T,
    val2: &'b T,
    mut expected_facts: BTreeSet<Fact>,
    marker_traits: TypedMarkerTraits<T>,
) where
    T: Facet<'a> + ?Sized,
{
    let name = format!("{}", T::SHAPE);
    eprint!("{}", format_args!("== {name}: ").yellow());

    let facts = collect_facts(val1, val2);

    expected_facts.extend(marker_traits.marker_traits.iter().map(Fact::MarkerTrait));

    dbg!(T::VTABLE.marker_traits());

    report_maybe_mismatch(val1, val2, expected_facts, facts);
}

// slightly different version to overwrite the equality parts as miri juggles the addresses
#[cfg(feature = "fn-ptr")]
fn check_facts_no_cmp<'a, 'b: 'a, T>(
    val1: &'b T,
    val2: &'b T,
    mut expected_facts: BTreeSet<Fact>,
    marker_traits: TypedMarkerTraits<T>,
) where
    T: Facet<'a> + ?Sized,
{
    let name = format!("{}", T::SHAPE);
    eprint!("{}", format_args!("== {name}: ").yellow());

    let facts = collect_facts(val1, val1);
    for &fact in facts.iter() {
        if let Fact::PartialEqAnd { .. } | Fact::OrdAnd { .. } | Fact::PartialOrdAnd { .. } = fact {
            expected_facts.insert(fact);
        }
    }

    expected_facts.extend(marker_traits.marker_traits.iter().map(Fact::MarkerTrait));

    report_maybe_mismatch(val1, val2, expected_facts, facts);
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

#[derive(Debug)]
struct TypedMarkerTraits<T: ?Sized> {
    marker_traits: MarkerTraits,
    phantom: PhantomData<T>,
}

impl<T: ?Sized> Clone for TypedMarkerTraits<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: ?Sized> Copy for TypedMarkerTraits<T> {}

impl<T: ?Sized> TypedMarkerTraits<T> {
    fn new() -> Self {
        Self {
            marker_traits: MarkerTraits::empty(),
            phantom: PhantomData,
        }
    }

    fn eq(mut self) -> Self
    where
        T: Eq,
    {
        self.marker_traits |= MarkerTraits::EQ;
        self
    }

    fn send(mut self) -> Self
    where
        T: Send,
    {
        self.marker_traits |= MarkerTraits::SEND;
        self
    }

    fn sync(mut self) -> Self
    where
        T: Sync,
    {
        self.marker_traits |= MarkerTraits::SYNC;
        self
    }

    fn copy(mut self) -> Self
    where
        T: Copy,
    {
        self.marker_traits |= MarkerTraits::COPY;
        self
    }

    fn unpin(mut self) -> Self
    where
        T: Unpin,
    {
        self.marker_traits |= MarkerTraits::UNPIN;
        self
    }

    fn unwind_safe(mut self) -> Self
    where
        T: UnwindSafe,
    {
        self.marker_traits |= MarkerTraits::UNWIND_SAFE;
        self
    }

    fn ref_unwind_safe(mut self) -> Self
    where
        T: RefUnwindSafe,
    {
        self.marker_traits |= MarkerTraits::REF_UNWIND_SAFE;
        self
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
    MarkerTrait(MarkerTraits),
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
                write!(f, "impl Ord and l {} r", ord_str)
            }
            Fact::PartialOrdAnd { l_ord_r } => {
                let ord_str = match l_ord_r {
                    Some(Ordering::Less) => "<",
                    Some(Ordering::Equal) => "==",
                    Some(Ordering::Greater) => ">",
                    None => "??",
                };
                write!(f, "impl PartialOrd and l {} r", ord_str)
            }
            Fact::Default => write!(f, "impl Default"),
            Fact::Clone => write!(f, "impl Clone"),
            Fact::MarkerTrait(marker_trait) => write!(f, "impl {marker_trait:?}"),
        }
    }
}

#[test]
fn test_integer_traits() {
    let marker_traits = TypedMarkerTraits::new()
        .eq()
        .send()
        .sync()
        .copy()
        .unpin()
        .unwind_safe()
        .ref_unwind_safe();

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
        marker_traits,
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
        marker_traits,
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
        marker_traits,
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
        marker_traits,
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
        marker_traits,
    );
}

#[test]
fn test_boolean_traits() {
    let marker_traits = TypedMarkerTraits::new()
        .eq()
        .send()
        .sync()
        .copy()
        .unpin()
        .unwind_safe()
        .ref_unwind_safe();

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
        marker_traits,
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
        marker_traits,
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
        marker_traits,
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
        marker_traits,
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
        TypedMarkerTraits::new()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );
    check_facts(
        &Cow::Owned("hello".to_string()),
        &Cow::Owned("world".to_string()),
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .clone()
            .default()
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    check_facts(
        &["hello", "world"][..],
        &["foo", "bar"][..],
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Greater)
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );
}

#[test]
fn test_array_traits() {
    // [i32; 0] implements Debug, PartialEq, Ord, Default, and Clone
    check_facts::<[i32; 0]>(
        &[],
        &[],
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .default()
            .clone()
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );
    // [i32; 1] implements Debug, PartialEq, Ord, Default, and Clone
    check_facts(
        &[42],
        &[24],
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Greater)
            .default()
            .clone()
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );
    // [i32; 2] implements Debug, PartialEq, Ord, Default, and Clone
    check_facts(
        &[1, 2],
        &[1, 3],
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .default()
            .clone()
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );
    // [i32; 33] implements Debug, PartialEq, Ord and Clone but not yet `Default`
    check_facts(
        &[0; 33],
        &[0; 33],
        FactBuilder::new()
            .debug()
            .display()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .clone()
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    // [&str; 1] implements Debug, PartialEq, Ord, Default, and Clone
    check_facts(
        &["hello"],
        &["world"],
        FactBuilder::new()
            .display()
            .debug()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Less)
            .clone()
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );
}

#[test]
fn test_vecs() {
    // Vec<i32> implements Debug, PartialEq, but not Ord
    check_facts(
        &vec![1, 2, 3],
        &vec![4, 5, 6],
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .default()
            .clone()
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    // Vec<String> implements Debug, PartialEq, but not Ord
    check_facts(
        &vec!["hello".to_string(), "world".to_string()],
        &vec!["foo".to_string(), "bar".to_string()],
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .default()
            .clone()
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    // Two pairs of equal Vecs
    let vec1 = vec![1, 2, 3];
    let vec2 = vec![1, 2, 3];
    check_facts(
        &vec1.clone(),
        &vec2.clone(),
        FactBuilder::new()
            .debug()
            .partial_eq_and(true)
            .default()
            .clone()
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    let vec3 = vec!["hello".to_string(), "world".to_string()];
    let vec4 = vec!["hello".to_string(), "world".to_string()];
    check_facts(
        &vec3.clone(),
        &vec4.clone(),
        FactBuilder::new()
            .debug()
            .partial_eq_and(true)
            .default()
            .clone()
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );
}

#[test]
fn test_hashmaps() {
    use std::collections::HashMap;

    // HashMap<String, i32> implements Debug, PartialEq, but not Ord
    let mut map1 = HashMap::new();
    map1.insert("key1".to_string(), 42);
    map1.insert("key2".to_string(), 24);

    let mut map2 = HashMap::new();
    map2.insert("key3".to_string(), 100);
    map2.insert("key4".to_string(), 200);

    check_facts(
        &map1.clone(),
        &map2.clone(),
        FactBuilder::new()
            .debug()
            .partial_eq_and(false)
            .default()
            .clone()
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    // Two pairs of equal HashMaps
    let mut map3 = HashMap::new();
    map3.insert("key1".to_string(), 10);
    map3.insert("key2".to_string(), 20);

    let mut map4 = HashMap::new();
    map4.insert("key1".to_string(), 10);
    map4.insert("key2".to_string(), 20);

    check_facts(
        &map3.clone(),
        &map4.clone(),
        FactBuilder::new()
            .debug()
            .partial_eq_and(true)
            .default()
            .clone()
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );
}

#[test]
fn test_custom_structs() {
    // Struct with no trait implementations
    #[derive(Facet)]
    struct StructNoTraits {
        value: i32,
    }
    check_facts(
        &StructNoTraits { value: 42 },
        &StructNoTraits { value: 24 },
        FactBuilder::new().build(),
        TypedMarkerTraits::new()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    // Struct with Debug only
    #[derive(Facet, Debug)]
    struct StructDebug {
        value: i32,
    }
    check_facts(
        &StructDebug { value: 42 },
        &StructDebug { value: 24 },
        FactBuilder::new().debug().build(),
        TypedMarkerTraits::new()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    // Struct with Debug and PartialEq
    #[derive(Facet, Debug, PartialEq)]
    struct StructDebugEq {
        value: i32,
    }
    check_facts(
        &StructDebugEq { value: 42 },
        &StructDebugEq { value: 24 },
        FactBuilder::new().debug().partial_eq_and(false).build(),
        TypedMarkerTraits::new()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    // Struct with all traits
    #[derive(Facet, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );
}

#[test]
fn test_tuple_structs() {
    // Tuple struct with no trait implementations
    #[derive(Facet)]
    #[allow(dead_code)]
    struct TupleNoTraits(i32, String);
    check_facts(
        &TupleNoTraits(42, "Hello".to_string()),
        &TupleNoTraits(24, "World".to_string()),
        FactBuilder::new().build(),
        TypedMarkerTraits::new()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    // Tuple struct with Debug only
    #[derive(Facet, Debug)]
    #[allow(dead_code)]
    struct TupleDebug(i32, String);
    check_facts(
        &TupleDebug(42, "Hello".to_string()),
        &TupleDebug(24, "World".to_string()),
        FactBuilder::new().debug().build(),
        TypedMarkerTraits::new()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    // Tuple struct with EQ only
    #[derive(Facet, PartialEq)]
    struct TupleEq(i32, String);
    check_facts(
        &TupleEq(42, "Hello".to_string()),
        &TupleEq(24, "World".to_string()),
        FactBuilder::new().partial_eq_and(false).build(),
        TypedMarkerTraits::new()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    // Tuple struct with all traits
    #[derive(Facet, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );
}

#[test]
fn test_enums() {
    #[derive(Facet, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );
}

#[test]
fn test_weird_cmp() {
    #[derive(Facet)]
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
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
        FactBuilder::new().debug().clone().build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    extern "C" fn foo(_: usize) -> u32 {
        0
    }

    let foo = foo as extern "C" fn(_) -> _;

    check_facts_no_cmp::<extern "C" fn(usize) -> u32>(
        &foo,
        &foo,
        FactBuilder::new().debug().clone().build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    check_facts_no_cmp::<fn(u32) -> u32>(
        &((|_| 0) as fn(_) -> _),
        &((|_| 1) as fn(_) -> _),
        FactBuilder::new().debug().clone().build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
            .debug()
            .clone()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    check_facts(
        &ptr.cast_mut(),
        &ptr.cast_mut(),
        FactBuilder::new()
            .debug()
            .clone()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    let s = "abc";
    let ptr = core::ptr::from_ref(s);

    check_facts(
        &ptr,
        &ptr,
        FactBuilder::new()
            .debug()
            .clone()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    check_facts(
        &ptr.cast_mut(),
        &ptr.cast_mut(),
        FactBuilder::new()
            .debug()
            .clone()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    check_facts(
        &ptr,
        &&raw const s[..1],
        FactBuilder::new()
            .debug()
            .clone()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Greater)
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    check_facts(
        &ptr.cast_mut(),
        &core::ptr::from_ref(&s[..1]).cast_mut(),
        FactBuilder::new()
            .debug()
            .clone()
            .partial_eq_and(false)
            .correct_ord_and(Ordering::Greater)
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );
}

#[test]
fn test_ref() {
    check_facts(
        &&(),
        &&(),
        FactBuilder::new()
            .debug()
            .clone()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
    );

    let unit = ();
    let ptr = &raw const unit;

    check_facts(
        &&ptr,
        &&ptr,
        FactBuilder::new()
            .debug()
            .clone()
            .partial_eq_and(true)
            .correct_ord_and(Ordering::Equal)
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .copy()
            .unpin()
            .unwind_safe()
            .ref_unwind_safe(),
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
            .correct_ord_and(Ordering::Equal)
            .build(),
        TypedMarkerTraits::new()
            .eq()
            .send()
            .sync()
            .unpin()
            .ref_unwind_safe(),
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
            .correct_ord_and(Ordering::Equal)
            .build(),
        TypedMarkerTraits::new().eq().unpin().ref_unwind_safe(),
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
        TypedMarkerTraits::new().unpin(),
    );

    check_facts(
        &&w1,
        &&w2,
        FactBuilder::new().clone().debug().build(),
        TypedMarkerTraits::new().copy().unpin(),
    );

    check_facts(
        &&mut w1,
        &&mut w2,
        FactBuilder::new().debug().build(),
        TypedMarkerTraits::new().unpin(),
    );

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
        TypedMarkerTraits::new().eq().copy().unpin(),
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
        TypedMarkerTraits::new().eq().copy().unpin(),
    );
}

#[test]
fn test_ipv4_addr_parse_from_str() {
    use facet_reflect::Partial;

    // Test that Ipv4Addr can be parsed from a string using facet reflection
    let mut wip = Partial::alloc_shape(Ipv4Addr::SHAPE).unwrap();

    // This should work - parse a valid IP address
    let result = wip.parse_from_str("127.0.0.1");
    assert!(result.is_ok(), "Failed to parse valid IP address");

    let value: Ipv4Addr = wip.build().unwrap().materialize().unwrap();
    assert_eq!(value, "127.0.0.1".parse::<Ipv4Addr>().unwrap());

    // Test that invalid IP addresses fail to parse
    let mut wip2 = Partial::alloc_shape(Ipv4Addr::SHAPE).unwrap();
    let result2 = wip2.parse_from_str("not.an.ip.address");
    assert!(result2.is_err(), "Should fail to parse invalid IP address");

    // Test that Ipv4Addr shape indicates it supports parsing
    let shape = Ipv4Addr::SHAPE;
    assert!(
        shape.is_from_str(),
        "Ipv4Addr should support parsing from string"
    );

    // Check that the vtable has a parse function
    let parse_fn = shape.vtable.sized().and_then(|v| (v.parse)());
    assert!(
        parse_fn.is_some(),
        "Ipv4Addr should have a parse function in vtable"
    );
}

use ctor::ctor;
use facet_core::{Facet, OpaqueConst, OpaqueUninit};
use facet_derive::Facet;
use facet_poke::{Peek, Poke};

use owo_colors::{OwoColorize, Style};
use std::{cmp::Ordering, collections::HashSet, fmt::Debug};

use facet_core as facet;

// Allow dead code in test modules since we're not constructing all enum variants
#[derive(Debug, PartialEq, Eq, Facet)]
struct FooBar {
    foo: u64,
    bar: String,
}

impl Default for FooBar {
    fn default() -> Self {
        FooBar {
            foo: 69,
            bar: String::new(),
        }
    }
}

#[test]
fn build_foobar_through_reflection() {
    let (poke, guard) = Poke::alloc::<FooBar>();
    let mut poke = poke.into_struct();
    unsafe {
        poke.unchecked_set_by_name("foo", OpaqueConst::new(&42u64))
            .unwrap();
    }

    {
        let bar = String::from("Hello, World!");
        unsafe {
            poke.unchecked_set_by_name("bar", OpaqueConst::new(&bar))
                .unwrap();
        }
        // bar has been moved out of
        core::mem::forget(bar);
    }

    let foo_bar = poke.build::<FooBar>(Some(guard));

    // Verify the fields were set correctly
    assert_eq!(foo_bar.foo, 42);
    assert_eq!(foo_bar.bar, "Hello, World!");

    assert_eq!(
        FooBar {
            foo: 42,
            bar: "Hello, World!".to_string()
        },
        foo_bar
    )
}

#[test]
#[should_panic(expected = "Field 'bar' was not initialized")]
fn build_foobar_incomplete() {
    let (poke, guard) = Poke::alloc::<FooBar>();
    let mut poke = poke.into_struct();
    unsafe {
        poke.unchecked_set_by_name("foo", OpaqueConst::new(&42u64))
            .unwrap();
    }

    let foo_bar = poke.build::<FooBar>(Some(guard));

    // Verify the fields were set correctly
    assert_eq!(foo_bar.foo, 42);
    assert_eq!(foo_bar.bar, "Hello, World!");

    assert_eq!(
        FooBar {
            foo: 42,
            bar: "Hello, World!".to_string()
        },
        foo_bar
    )
}

#[test]
fn build_foobar_after_default() {
    let mut foo_bar: FooBar = Default::default();

    let mut poke =
        unsafe { Poke::unchecked_new(OpaqueUninit::new(&mut foo_bar as *mut _), FooBar::SHAPE) }
            .into_struct();
    unsafe {
        poke.mark_initialized(0);
        poke.mark_initialized(1);
    }

    {
        let bar = String::from("Hello, World!");
        unsafe {
            poke.unchecked_set_by_name("bar", OpaqueConst::new(&bar))
                .unwrap();
        }
        // bar has been moved out of
        core::mem::forget(bar);
    }
    poke.build_in_place();

    // Verify the fields were set correctly
    assert_eq!(foo_bar.foo, 69);
    assert_eq!(foo_bar.bar, "Hello, World!");
}

#[ctor]
fn init_backtrace() {
    color_backtrace::install();
}

fn test_peek_pair<T>(val1: T, val2: T, expected_facts: HashSet<Fact>)
where
    T: Facet + 'static,
{
    let mut facts: HashSet<Fact> = HashSet::new();
    let name = format!("{}", T::SHAPE);

    eprint!("{}", format!("== {name}: ").yellow());
    let value_vtable = T::SHAPE.vtable;
    let traits = [
        ("Debug", value_vtable.debug.is_some()),
        ("Display", value_vtable.display.is_some()),
        ("Default", value_vtable.default_in_place.is_some()),
        ("Eq", value_vtable.eq.is_some()),
        ("Ord", value_vtable.ord.is_some()),
        ("Clone", value_vtable.clone_into.is_some()),
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

    let l = Peek::new(&val1);
    let r = Peek::new(&val2);

    let remarkable = Style::new().blue();

    // Format display representation
    if l.as_value().shape().vtable.display.is_some() {
        facts.insert(Fact::Display);
        let display_str = format!("{} vs {}", l.style(remarkable), r.style(remarkable));
        eprintln!("Display:   {}", display_str);
    }

    // Format debug representation
    if l.as_value().shape().vtable.debug.is_some() {
        facts.insert(Fact::Debug);
        let debug_str = format!("{:?} vs {:?}", l.style(remarkable), r.style(remarkable));
        eprintln!("Debug:     {}", debug_str);
    }

    // Test equality
    if let Some(eq_result) = l.as_value().eq(&r.as_value()) {
        facts.insert(Fact::EqualAnd { l_eq_r: eq_result });
        let eq_str = format!(
            "{:?} {} {:?}",
            l.style(remarkable),
            if eq_result { "==" } else { "!=" }.yellow(),
            r.style(remarkable),
        );
        eprintln!("Equality:  {}", eq_str);
    }

    // Test ordering
    if let Some(cmp_result) = l.as_value().cmp(&r.as_value()) {
        facts.insert(Fact::OrdAnd {
            l_ord_r: cmp_result,
        });
        let cmp_symbol = match cmp_result {
            Ordering::Less => "<",
            Ordering::Equal => "==",
            Ordering::Greater => ">",
        };
        let cmp_str = format!(
            "{:?} {} {:?}",
            l.style(remarkable),
            cmp_symbol.yellow(),
            r.style(remarkable),
        );
        eprintln!("Ordering:  {}", cmp_str);
    }

    // Test default_in_place
    let (poke, _guard) = Poke::alloc::<T>();
    let poke_value = poke.into_value();
    if let Ok(value) = poke_value.default_in_place() {
        facts.insert(Fact::Default);
        let peek = unsafe { Peek::unchecked_new(value.as_const(), T::SHAPE) };
        eprintln!("Default:   {}", format!("{:?}", peek).style(remarkable));
    }

    // Test clone
    if l.as_value().shape().vtable.clone_into.is_some() {
        facts.insert(Fact::Clone);
        eprintln!("Clone:     Implemented");
    }

    let expected_minus_actual: HashSet<_> = expected_facts.difference(&facts).collect();
    let actual_minus_expected: HashSet<_> = facts.difference(&expected_facts).collect();

    assert!(
        expected_facts == facts,
        "{} for {}: ({:?} vs {:?})\n{}\n{}",
        "Facts mismatch".red().bold(),
        name.style(remarkable),
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

#[test]
fn test_integer_traits() {
    // i32 implements Debug, PartialEq, and Ord
    test_peek_pair(
        42,
        24,
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(false)
            .ord_and(Ordering::Greater)
            .default()
            .clone()
            .build(),
    );

    // Test equal i32 values
    test_peek_pair(
        42,
        42,
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(true)
            .ord_and(Ordering::Equal)
            .default()
            .clone()
            .build(),
    );

    // Test i32::MIN and i32::MAX
    test_peek_pair(
        i32::MIN,
        i32::MAX,
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(false)
            .ord_and(Ordering::Less)
            .default()
            .clone()
            .build(),
    );

    // Test i32 with 0
    test_peek_pair(
        0,
        42,
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(false)
            .ord_and(Ordering::Less)
            .default()
            .clone()
            .build(),
    );

    // Test negative i32 values
    test_peek_pair(
        -10,
        10,
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(false)
            .ord_and(Ordering::Less)
            .default()
            .clone()
            .build(),
    );
}

#[test]
fn test_boolean_traits() {
    // bool implements Debug, PartialEq, Ord, and Display
    test_peek_pair(
        true,
        false,
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(false)
            .ord_and(Ordering::Greater)
            .default()
            .clone()
            .build(),
    );

    test_peek_pair(
        true,
        true,
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(true)
            .ord_and(Ordering::Equal)
            .default()
            .clone()
            .build(),
    );

    test_peek_pair(
        false,
        true,
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(false)
            .ord_and(Ordering::Less)
            .default()
            .clone()
            .build(),
    );

    test_peek_pair(
        false,
        false,
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(true)
            .ord_and(Ordering::Equal)
            .default()
            .clone()
            .build(),
    );
}

#[test]
fn test_floating_traits() {
    // f64 implements Debug, PartialEq
    test_peek_pair(
        3.18,
        2.71,
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(false)
            .default()
            .clone()
            .build(),
    );
}

#[test]
fn test_string_traits() {
    // String implements Debug, PartialEq, and Ord
    test_peek_pair(
        String::from("hello"),
        String::from("world"),
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(false)
            .ord_and(Ordering::Less)
            .default()
            .clone()
            .build(),
    );

    // &str implements Debug, PartialEq, and Ord
    test_peek_pair(
        "hello",
        "world",
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(false)
            .ord_and(Ordering::Less)
            .clone()
            .default()
            .build(),
    );

    // Cow<'a, str> implements Debug, PartialEq, and Ord
    use std::borrow::Cow;
    test_peek_pair(
        Cow::Borrowed("hello"),
        Cow::Borrowed("world"),
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(false)
            .ord_and(Ordering::Less)
            .clone()
            .default()
            .build(),
    );
    test_peek_pair(
        Cow::Owned("hello".to_string()),
        Cow::Owned("world".to_string()),
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(false)
            .ord_and(Ordering::Less)
            .clone()
            .default()
            .build(),
    );
    test_peek_pair(
        Cow::Borrowed("same"),
        Cow::Owned("same".to_string()),
        FactBuilder::new()
            .debug()
            .display()
            .equal_and(true)
            .ord_and(Ordering::Equal)
            .clone()
            .default()
            .build(),
    );
}

#[test]
fn test_slice_traits() {
    // &[i32] implements Debug, PartialEq, and Ord
    test_peek_pair(
        &[1, 2, 3][..],
        &[4, 5, 6][..],
        FactBuilder::new()
            .debug()
            .equal_and(false)
            .ord_and(Ordering::Less)
            .clone()
            .default()
            .build(),
    );

    // &[&str] implements Debug, PartialEq, and Ord
    test_peek_pair(
        &["hello", "world"][..],
        &["foo", "bar"][..],
        FactBuilder::new()
            .debug()
            .equal_and(false)
            .ord_and(Ordering::Greater)
            .clone()
            .default()
            .build(),
    );
}

#[test]
fn test_array_traits() {
    // [i32; 1] implements Debug, PartialEq, Ord, Default, and Clone
    test_peek_pair(
        [42],
        [24],
        FactBuilder::new()
            .debug()
            .equal_and(false)
            .ord_and(Ordering::Greater)
            .default()
            .clone()
            .build(),
    );

    // [&str; 1] implements Debug, PartialEq, Ord, Default, and Clone
    test_peek_pair(
        ["hello"],
        ["world"],
        FactBuilder::new()
            .debug()
            .equal_and(false)
            .ord_and(Ordering::Less)
            .default()
            .clone()
            .build(),
    );
}

#[test]
fn test_vecs() {
    // Vec<i32> implements Debug, PartialEq, but not Ord
    test_peek_pair(
        vec![1, 2, 3],
        vec![4, 5, 6],
        FactBuilder::new()
            .debug()
            .equal_and(false)
            .default()
            .clone()
            .build(),
    );

    // Vec<String> implements Debug, PartialEq, but not Ord
    test_peek_pair(
        vec!["hello".to_string(), "world".to_string()],
        vec!["foo".to_string(), "bar".to_string()],
        FactBuilder::new()
            .debug()
            .equal_and(false)
            .default()
            .clone()
            .build(),
    );

    // Two pairs of equal Vecs
    let vec1 = vec![1, 2, 3];
    let vec2 = vec![1, 2, 3];
    test_peek_pair(
        vec1.clone(),
        vec2.clone(),
        FactBuilder::new()
            .debug()
            .equal_and(true)
            .default()
            .clone()
            .build(),
    );

    let vec3 = vec!["hello".to_string(), "world".to_string()];
    let vec4 = vec!["hello".to_string(), "world".to_string()];
    test_peek_pair(
        vec3.clone(),
        vec4.clone(),
        FactBuilder::new()
            .debug()
            .equal_and(true)
            .default()
            .clone()
            .build(),
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

    test_peek_pair(
        map1.clone(),
        map2.clone(),
        FactBuilder::new()
            .debug()
            .equal_and(false)
            .default()
            .clone()
            .build(),
    );

    // Two pairs of equal HashMaps
    let mut map3 = HashMap::new();
    map3.insert("key1".to_string(), 10);
    map3.insert("key2".to_string(), 20);

    let mut map4 = HashMap::new();
    map4.insert("key1".to_string(), 10);
    map4.insert("key2".to_string(), 20);

    test_peek_pair(
        map3.clone(),
        map4.clone(),
        FactBuilder::new()
            .debug()
            .equal_and(true)
            .default()
            .clone()
            .build(),
    );
}

#[test]
fn test_custom_structs() {
    // Struct with no trait implementations
    #[derive(Facet)]
    struct StructNoTraits {
        value: i32,
    }
    test_peek_pair(
        StructNoTraits { value: 42 },
        StructNoTraits { value: 24 },
        FactBuilder::new().build(),
    );

    // Struct with Debug only
    #[derive(Facet, Debug)]
    struct StructDebug {
        value: i32,
    }
    test_peek_pair(
        StructDebug { value: 42 },
        StructDebug { value: 24 },
        FactBuilder::new().debug().build(),
    );

    // Struct with Debug and PartialEq
    #[derive(Facet, Debug, PartialEq)]
    struct StructDebugEq {
        value: i32,
    }
    test_peek_pair(
        StructDebugEq { value: 42 },
        StructDebugEq { value: 24 },
        FactBuilder::new().debug().equal_and(false).build(),
    );

    // Struct with all traits
    #[derive(Facet, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
    struct StructAll {
        value: i32,
    }
    test_peek_pair(
        StructAll { value: 42 },
        StructAll { value: 24 },
        FactBuilder::new()
            .debug()
            .equal_and(false)
            .ord_and(Ordering::Greater)
            .clone()
            .build(),
    );
    test_peek_pair(
        StructAll { value: 10 },
        StructAll { value: 90 },
        FactBuilder::new()
            .debug()
            .equal_and(false)
            .ord_and(Ordering::Less)
            .clone()
            .build(),
    );
    test_peek_pair(
        StructAll { value: 69 },
        StructAll { value: 69 },
        FactBuilder::new()
            .debug()
            .equal_and(true)
            .ord_and(Ordering::Equal)
            .clone()
            .build(),
    );
}

#[test]
fn test_tuple_structs() {
    // Tuple struct with no trait implementations
    #[derive(Facet)]
    struct TupleNoTraits(i32, String);
    test_peek_pair(
        TupleNoTraits(42, "Hello".to_string()),
        TupleNoTraits(24, "World".to_string()),
        FactBuilder::new().build(),
    );

    // Tuple struct with Debug only
    #[derive(Facet, Debug)]
    struct TupleDebug(i32, String);
    test_peek_pair(
        TupleDebug(42, "Hello".to_string()),
        TupleDebug(24, "World".to_string()),
        FactBuilder::new().debug().build(),
    );

    // Tuple struct with EQ only
    #[derive(Facet, PartialEq)]
    struct TupleEq(i32, String);
    test_peek_pair(
        TupleEq(42, "Hello".to_string()),
        TupleEq(24, "World".to_string()),
        FactBuilder::new().equal_and(false).build(),
    );

    // Tuple struct with all traits
    #[derive(Facet, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
    struct TupleAll(i32, String);
    test_peek_pair(
        TupleAll(42, "Hello".to_string()),
        TupleAll(24, "World".to_string()),
        FactBuilder::new()
            .debug()
            .equal_and(false)
            .ord_and(Ordering::Greater)
            .clone()
            .build(),
    );
}

// Commented out enum tests for now as they may need special handling
/*
#[test]
fn test_enums() {
    #[derive(Facet, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum TestEnum {
        Variant1,
        Variant2(i32),
        Variant3 { field: String },
    }

    test_peek_pair(
        "Enum-Unit",
        TestEnum::Variant1,
        TestEnum::Variant1,
        true,
        true,
        true,
    );

    test_peek_pair(
        "Enum-Tuple",
        TestEnum::Variant2(42),
        TestEnum::Variant2(24),
        true,
        true,
        true,
    );

    test_peek_pair(
        "Enum-Struct",
        TestEnum::Variant3 {
            field: "Hello".to_string(),
        },
        TestEnum::Variant3 {
            field: "World".to_string(),
        },
        true,
        true,
        true,
    );
}
*/

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Fact {
    Debug,
    Display,
    EqualAnd { l_eq_r: bool },
    OrdAnd { l_ord_r: Ordering },
    Default,
    Clone,
}

use core::fmt::{Display, Formatter, Result};

impl Display for Fact {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Fact::Debug => write!(f, "impl Debug"),
            Fact::Display => write!(f, "impl Display"),
            Fact::EqualAnd { l_eq_r } => write!(
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
            Fact::Default => write!(f, "impl Default"),
            Fact::Clone => write!(f, "impl Clone"),
        }
    }
}

#[derive(Default)]
pub struct FactBuilder {
    has_debug: bool,
    has_display: bool,
    has_equal_and: Option<bool>,
    has_ord_and: Option<Ordering>,
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

    fn equal_and(mut self, l_eq_r: bool) -> Self {
        self.has_equal_and = Some(l_eq_r);
        self
    }

    fn ord_and(mut self, l_ord_r: Ordering) -> Self {
        self.has_ord_and = Some(l_ord_r);
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

    fn build(self) -> HashSet<Fact> {
        let mut facts = HashSet::new();
        if self.has_debug {
            facts.insert(Fact::Debug);
        }
        if self.has_display {
            facts.insert(Fact::Display);
        }
        if let Some(l_eq_r) = self.has_equal_and {
            facts.insert(Fact::EqualAnd { l_eq_r });
        }
        if let Some(l_ord_r) = self.has_ord_and {
            facts.insert(Fact::OrdAnd { l_ord_r });
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

#[test]
fn build_u64_properly() {
    let shape = u64::SHAPE;
    eprintln!("{:#?}", shape);

    let (poke, _guard) = Poke::alloc::<u64>();
    let poke = poke.into_scalar();
    let data = unsafe { poke.write(OpaqueConst::new(&42u64)) };
    let value = unsafe { data.read::<u64>() };

    // Verify the value was set correctly
    assert_eq!(value, 42);
}

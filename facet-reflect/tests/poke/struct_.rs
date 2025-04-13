use facet::Facet;
use facet_reflect::{PokeValueUninit, ReflectError};

use std::{fmt::Debug, mem::MaybeUninit};

#[derive(Debug, PartialEq, Eq, Facet)]
struct Person {
    age: u64,
    name: String,
}

impl Default for Person {
    fn default() -> Self {
        Person {
            age: 69,
            name: String::new(),
        }
    }
}

#[test]
fn build_person_through_reflection() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let (poke, guard) = PokeValueUninit::alloc::<Person>();
    let poke = poke.into_struct().unwrap();
    let poke = poke.field_by_name("age")?.set(42u64)?.into_struct_uninit();
    let poke = poke
        .field_by_name("name")?
        .set(String::from("Joan Watson"))?
        .into_struct_uninit();
    let person: Person = poke.build(Some(guard))?;

    assert_eq!(
        Person {
            age: 42,
            name: "Joan Watson".to_string()
        },
        person
    );
    Ok(())
}

#[test]
fn set_by_name_no_such_field() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let (poke, _guard) = PokeValueUninit::alloc::<Person>();
    let poke = poke.into_struct().unwrap();
    match poke.field_by_name("philosophy") {
        Err(facet::FieldError::NoSuchField) => Ok(()),
        other => panic!("Expected NoSuchField error, got {:?}", other),
    }
}

#[test]
fn set_by_name_type_mismatch() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let (poke, _guard) = PokeValueUninit::alloc::<Person>();
    let poke = poke.into_struct().unwrap();
    match poke.field_by_name("age")?.set(42u16) {
        Err(ReflectError::WrongShape { expected, actual }) => {
            assert_eq!(expected, u64::SHAPE);
            assert_eq!(actual, u16::SHAPE);
            Ok(())
        }
        other => panic!("Expected TypeMismatch error, got {:?}", other),
    }
}

#[test]
fn build_person_incomplete() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let (poke, guard) = PokeValueUninit::alloc::<Person>();
    let poke = poke.into_struct().unwrap();
    let poke = poke.field_by_name("age")?.set(42u64)?.into_struct_uninit();

    // we haven't set name, this'll panic
    assert!(poke.build::<Person>(Some(guard)).is_err());
    Ok(())
}

#[derive(Facet, Debug, PartialEq, Eq)]
struct ScoredPerson {
    person: Person,
    score: u64,
}

#[test]
fn nested_struct() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let (poke, guard) = PokeValueUninit::alloc::<ScoredPerson>();
    let poke = poke.into_struct().unwrap();
    let poke = poke.field_by_name("person")?;
    let poke = poke.into_struct()?;
    let poke = poke.field_by_name("age")?.set(42u64)?.into_struct_slot();
    let poke = poke
        .field_by_name("name")?
        .set(String::from("Joan Watson"))?
        .into_struct_slot();
    let poke = poke.finish()?.into_struct_uninit();
    let poke = poke
        .field_by_name("score")?
        .set(42u64)?
        .into_struct_uninit();
    let person: ScoredPerson = poke.build(Some(guard))?;

    assert_eq!(
        ScoredPerson {
            person: Person {
                age: 42,
                name: "Joan Watson".to_string()
            },
            score: 42,
        },
        person
    );
    Ok(())
}

#[test]
fn struct_with_invariant() {
    #[derive(Facet)]
    #[facet(invariants = "invariants")]
    struct MyNonZeroU8 {
        value: u8,
    }

    impl MyNonZeroU8 {
        // TODO: does this need to be MaybUninit?
        fn invariants(&self) -> bool {
            self.value != 0
        }
    }

    facet_testhelpers::setup();

    let (poke, guard) = PokeValueUninit::alloc::<MyNonZeroU8>();
    let poke = poke.into_struct().unwrap();
    let poke = poke
        .field_by_name("value")
        .unwrap()
        .set(0u8)
        .unwrap()
        .into_struct_uninit();

    // This should fail because the invariant is violated (value == 0)
    match poke.build::<MyNonZeroU8>(Some(guard)) {
        Err(ReflectError::InvariantViolation) => {
            // Expected failure due to invariant violation
        }
        Ok(_) => panic!("Expected invariant violation, but build succeeded"),
        Err(e) => panic!("Expected invariant violation, got {:?}", e),
    }
}

// #[test]
// fn mutate_person() {
//     facet_testhelpers::setup();

//     let mut person: Person = Default::default();

//     {
//         let mut poke = Poke::new(&mut person).into_struct();
//         // Use the safe set_by_name method
//         poke.set_by_name("name", String::from("Hello, World!"))
//             .unwrap();
//         poke.build_in_place();
//     }

//     // Verify the fields were set correctly
//     assert_eq!(person.age, 69);
//     assert_eq!(person.name, "Hello, World!");
// }

use std::{
    collections::{BTreeSet, HashSet},
    convert::TryInto,
};

use quickcheck_macros::quickcheck;
use static_assertions::{assert_eq_align, assert_eq_size, assert_eq_size_ptr, assert_eq_size_val};

use crate::{Validated, ValidatedBuf};

#[test]
pub fn equality_tests() -> Result<(), Box<dyn std::error::Error>> {
    let x = ValidatedBuf::from_static("One");
    let y = Validated::from_static("One");
    assert_eq!(x, y);
    assert_eq!(x, *y);
    assert_eq!(&x, y);
    assert_eq!(y, x);
    assert_eq!(y, &x);
    assert_eq!(*y, x);

    assert_eq!("One", x.clone().take());
    let z = x.clone().into_boxed_ref();
    assert_eq!(y, &*z);
    assert_eq!(&*z, y);
    assert_eq!(x, &*z);
    assert_eq!(&*z, x);

    assert_eq!(x, z.into_owned());

    Ok(())
}

#[test]
pub fn parsing_owned_pass() -> Result<(), Box<dyn std::error::Error>> {
    let x: ValidatedBuf = "One".parse()?;
    assert_eq!("One", x.as_str());
    Ok(())
}

#[test]
#[should_panic]
pub fn parsing_owned_fails() {
    let _: ValidatedBuf = "Test 🏗".parse().unwrap();
}

#[test]
pub fn try_from_owned_pass() -> Result<(), Box<dyn std::error::Error>> {
    let x: ValidatedBuf = "One".try_into()?;
    assert_eq!("One", x.as_str());
    Ok(())
}

#[test]
#[should_panic]
pub fn try_from_owned_fails() {
    let _: ValidatedBuf = "Test 🏗".try_into().unwrap();
}

#[test]
pub fn try_from_borrowed_pass() -> Result<(), Box<dyn std::error::Error>> {
    let x: &Validated = "One".try_into()?;
    assert_eq!("One", x.as_str());
    Ok(())
}

#[test]
#[should_panic]
pub fn try_from_borrowed_fails() {
    let _: &Validated = "Test 🏗".try_into().unwrap();
}

#[test]
fn debug_and_display_tests() {
    let x = ValidatedBuf::from_static("One");
    let y = Validated::from_static("One");

    assert_eq!("One", x.to_string());
    assert_eq!("One", y.to_string());
    assert_eq!("\"One\"", format!("{:?}", x));
    assert_eq!("\"One\"", format!("{:?}", y));
}

#[cfg_attr(miri, ignore = "takes too long on miri")]
#[quickcheck]
fn owned_and_borrowed_hashes_are_equivalent(s: String) -> quickcheck::TestResult {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    let owned = if let Ok(x) = ValidatedBuf::new(s.clone()) {
        x
    } else {
        return quickcheck::TestResult::discard();
    };

    let owned_hash = {
        let mut hasher = DefaultHasher::new();
        owned.hash(&mut hasher);
        hasher.finish()
    };

    let borrowed = Validated::from_str(&s).unwrap();

    let borrowed_hash = {
        let mut hasher = DefaultHasher::new();
        borrowed.hash(&mut hasher);
        hasher.finish()
    };

    if owned_hash == borrowed_hash {
        quickcheck::TestResult::passed()
    } else {
        quickcheck::TestResult::failed()
    }
}

#[test]
fn can_use_as_hash_keys() {
    let mut map = HashSet::new();

    assert!(map.insert(ValidatedBuf::from_static("One")));
    assert!(map.insert(ValidatedBuf::from_static("Seven")));

    assert!(map.contains(Validated::from_static("One")));
    assert!(map.contains(&ValidatedBuf::from_static("One")));
    assert!(!map.contains(Validated::from_static("Two")));

    assert!(!map.remove(Validated::from_static("Two")));
    assert!(map.remove(Validated::from_static("One")));
    assert!(!map.remove(Validated::from_static("One")));

    assert!(map.remove(&ValidatedBuf::from_static("Seven")));
    assert!(!map.remove(Validated::from_static("Seven")));

    assert!(map.is_empty());
}

#[test]
fn can_use_refs_as_hash_keys() {
    let mut map = HashSet::new();

    assert!(map.insert(Validated::from_static("One")));
    assert!(map.insert(Validated::from_static("Seven")));

    assert!(map.contains(Validated::from_static("One")));
    assert!(map.contains(&*ValidatedBuf::from_static("One")));
    assert!(!map.contains(Validated::from_static("Two")));

    assert!(!map.remove(Validated::from_static("Two")));
    assert!(map.remove(Validated::from_static("One")));
    assert!(!map.remove(Validated::from_static("One")));

    assert!(map.remove(&*ValidatedBuf::from_static("Seven")));
    assert!(!map.remove(Validated::from_static("Seven")));

    assert!(map.is_empty());
}

#[test]
fn can_use_as_btree_keys() {
    let mut map = BTreeSet::new();

    assert!(map.insert(ValidatedBuf::from_static("One")));
    assert!(map.insert(ValidatedBuf::from_static("Seven")));

    assert!(map.contains(Validated::from_static("One")));
    assert!(map.contains(&ValidatedBuf::from_static("One")));
    assert!(!map.contains(Validated::from_static("Two")));

    assert!(!map.remove(Validated::from_static("Two")));
    assert!(map.remove(Validated::from_static("One")));
    assert!(!map.remove(Validated::from_static("One")));

    assert!(map.remove(&ValidatedBuf::from_static("Seven")));
    assert!(!map.remove(Validated::from_static("Seven")));

    assert!(map.is_empty());
}

#[test]
fn can_use_refs_as_btree_keys() {
    let mut map = BTreeSet::new();

    assert!(map.insert(Validated::from_static("One")));
    assert!(map.insert(Validated::from_static("Seven")));

    assert!(map.contains(Validated::from_static("One")));
    assert!(map.contains(&*ValidatedBuf::from_static("One")));
    assert!(!map.contains(Validated::from_static("Two")));

    assert!(!map.remove(Validated::from_static("Two")));
    assert!(map.remove(Validated::from_static("One")));
    assert!(!map.remove(Validated::from_static("One")));

    assert!(map.remove(&*ValidatedBuf::from_static("Seven")));
    assert!(!map.remove(Validated::from_static("Seven")));

    assert!(map.is_empty());
}

#[test]
#[should_panic]
fn verify_serialization_fail_borrow() {
    const SERIALIZATION: &str = "\"Test 🏗\"";
    dbg!(SERIALIZATION.as_bytes());
    let _: &Validated = serde_json::from_str(SERIALIZATION).unwrap();
}

#[test]
#[should_panic]
fn verify_serialization_fail_boxed() {
    const SERIALIZATION: &str = "\"Test 🏗\"";
    let _: Box<Validated> = serde_json::from_str(SERIALIZATION).unwrap();
}

#[test]
#[should_panic]
fn verify_serialization_fail_owned() {
    const SERIALIZATION: &str = "\"Test 🏗\"";
    let _: ValidatedBuf = serde_json::from_str(SERIALIZATION).unwrap();
}

#[test]
fn verify_serialization_pass_borrow() -> Result<(), Box<dyn std::error::Error>> {
    const SERIALIZATION: &str = "\"Test \u{037E}\"";
    let expected = Validated::from_str("Test \u{037E}")?;
    let actual: &Validated = serde_json::from_str(SERIALIZATION)?;
    assert_eq!(expected, actual);
    Ok(())
}

#[test]
fn verify_serialization_pass_boxed() -> Result<(), Box<dyn std::error::Error>> {
    const SERIALIZATION: &str = "\"Test \u{037E}\"";
    let expected = Validated::from_str("Test \u{037E}")?;
    let actual: Box<Validated> = serde_json::from_str(SERIALIZATION)?;
    assert_eq!(expected, &*actual);
    Ok(())
}

#[test]
fn verify_serialization_pass_owned() -> Result<(), Box<dyn std::error::Error>> {
    const SERIALIZATION: &str = "\"Test \u{037E}\"";
    let expected = Validated::from_str("Test \u{037E}")?;
    let actual: ValidatedBuf = serde_json::from_str(SERIALIZATION)?;
    assert_eq!(expected, actual);
    Ok(())
}

#[test]
fn check_reference_alignment() {
    dbg!(std::mem::align_of::<&str>());
    dbg!(std::mem::align_of::<&Validated>());
    assert_eq_align!(&Validated, &str);
}

#[test]
fn check_reference_size() {
    dbg!(std::mem::size_of::<&str>());
    dbg!(std::mem::size_of::<&Validated>());
    assert_eq_size!(&Validated, &str);
}

#[test]
#[allow(clippy::forget_ref, clippy::transmute_ptr_to_ptr)]
fn check_reference_size_ptr() {
    let s = "source";
    let y: &Validated = Validated::from_str(s).unwrap();
    assert_eq_size_ptr!(&s, &y);
}

#[test]
#[allow(clippy::forget_ref, clippy::transmute_ptr_to_ptr)]
fn check_reference_size_val() {
    let s = "source";
    let y: &Validated = Validated::from_str(s).unwrap();
    dbg!(std::mem::size_of_val(s));
    dbg!(std::mem::size_of_val(y));
    assert_eq_size_val!(s, y);
}

#[test]
fn check_boxed_ref_alignment() {
    dbg!(std::mem::align_of::<Box<str>>());
    dbg!(std::mem::align_of::<Box<Validated>>());
    assert_eq_align!(Box<Validated>, Box<str>);
}

#[test]
fn check_boxed_ref_size() {
    dbg!(std::mem::size_of::<Box<str>>());
    dbg!(std::mem::size_of::<Box<Validated>>());
    assert_eq_size!(Box<Validated>, Box<str>);
}

#[test]
fn check_boxed_ref_size_ptr() {
    let source = String::from("source");
    let s = source.clone().into_boxed_str();
    let y: Box<Validated> = ValidatedBuf::new(source).unwrap().into_boxed_ref();
    assert_eq_size_ptr!(&s, &y);
}

#[test]
fn check_boxed_ref_size_val() {
    let source = String::from("source");
    let s = source.clone().into_boxed_str();
    let y: Box<Validated> = ValidatedBuf::new(source).unwrap().into_boxed_ref();
    dbg!(std::mem::size_of_val(&s));
    dbg!(std::mem::size_of_val(&y));
    assert_eq_size_val!(s, y);
}

#[test]
fn check_owned_alignment() {
    dbg!(std::mem::align_of::<String>());
    dbg!(std::mem::align_of::<ValidatedBuf>());
    assert_eq_align!(ValidatedBuf, String);
}

#[test]
fn check_owned_size() {
    dbg!(std::mem::size_of::<String>());
    dbg!(std::mem::size_of::<ValidatedBuf>());
    assert_eq_size!(ValidatedBuf, String);
}

#[test]
fn check_owned_size_ptr() {
    let s = String::from("source");
    let y: ValidatedBuf = ValidatedBuf::new(s.clone()).unwrap();
    assert_eq_size_ptr!(&s, &y);
}

#[test]
fn check_owned_size_val() {
    let s = String::from("source");
    let y: ValidatedBuf = ValidatedBuf::new(s.clone()).unwrap();
    dbg!(std::mem::size_of_val(&s));
    dbg!(std::mem::size_of_val(&y));
    assert_eq_size_val!(s, y);
}

assert_core_impls!(ValidatedBuf => Validated where ValidationError = crate::InvalidData);

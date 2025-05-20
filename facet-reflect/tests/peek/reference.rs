use facet_reflect::Peek;

#[test]
fn string_ref() {
    let s = String::from("abc");
    let r = &s;
    let peek = Peek::new::<&String>(&r);

    assert_eq!(format!("{peek}"), "abc");
}

#[test]
fn string_mut_ref() {
    let mut s = String::from("abc");
    let r = &mut s;
    let peek = Peek::new::<&mut String>(&r);

    assert_eq!(format!("{peek}"), "abc");
}

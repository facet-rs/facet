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

#[test]
fn str_ref() {
    let s = "abc";
    let peek = Peek::new::<&str>(&s);

    assert_eq!(format!("{peek}"), "abc");
}

#[test]
fn str_ref_ref() {
    let s = "abc";
    let r = &s;
    let peek = Peek::new::<&&str>(&r);

    assert_eq!(format!("{peek}"), "abc");
}

#[test]
fn str_mut_ref() {
    let mut s = String::from("abc");
    let r = s.as_mut_str();
    let peek = Peek::new::<&mut str>(&r);

    assert_eq!(format!("{peek}"), "abc");
}

#[test]
fn str_mut_ref_mut_ref() {
    let mut s = String::from("abc");
    let mut r = s.as_mut_str();
    let r = &mut r;
    let peek = Peek::new::<&mut &mut str>(&r);

    assert_eq!(format!("{peek}"), "abc");
}

#[test]
fn str_ref_mut_ref() {
    let mut s = "abc";
    let r = &mut s;
    let peek = Peek::new::<&mut &str>(&r);

    assert_eq!(format!("{peek}"), "abc");
}

#[test]
fn str_mut_ref_ref() {
    let mut s = String::from("abc");
    let r = s.as_mut_str();
    let r = &r;
    let peek = Peek::new::<&&mut str>(&r);

    assert_eq!(format!("{peek}"), "abc");
}

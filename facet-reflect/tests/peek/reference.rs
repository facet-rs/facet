use facet_reflect::Peek;

#[test]
fn string_owned() {
    let s = String::from("abc");
    let peek = Peek::new::<String>(&s);

    assert_eq!(format!("{peek}"), "abc");
}

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
    const S: &&str = &"abc";
    let peek = Peek::new::<&&str>(&S);
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
#[cfg_attr(miri, ignore)] // Intentional Box::leak
fn str_mut_ref_mut_ref() {
    let s = Box::leak(Box::new(String::from("abc")));
    let r: &'static mut &mut str = Box::leak(Box::new(s.as_mut_str()));
    let peek = Peek::new::<&mut &mut str>(&r);

    assert_eq!(format!("{peek}"), "abc");
}

#[test]
#[cfg_attr(miri, ignore)] // Intentional Box::leak
fn str_ref_mut_ref() {
    let s: &'static str = "abc";
    let r: &'static mut &str = Box::leak(Box::new(s));
    let peek = Peek::new::<&mut &str>(&r);

    assert_eq!(format!("{peek}"), "abc");
}

#[test]
#[cfg_attr(miri, ignore)] // Intentional Box::leak
fn str_mut_ref_ref() {
    let s = Box::leak(Box::new(String::from("abc")));
    let r: &'static &mut str = Box::leak(Box::new(s.as_mut_str()));
    let peek = Peek::new::<&&mut str>(&r);

    assert_eq!(format!("{peek}"), "abc");
}

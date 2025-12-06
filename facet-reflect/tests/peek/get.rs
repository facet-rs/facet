use facet_reflect::Peek;

#[test]
fn test_peek_get_via_mem() {
    fn retrieve_via_as_str<'a, 'mem, 'facet>(peek: &'a Peek<'mem, 'facet>) -> &'mem str {
        peek.as_str().unwrap()
    }

    fn retrieve_via_get<'a, 'mem, 'facet>(peek: &'a Peek<'mem, 'facet>) -> &'mem str {
        peek.get::<str>().unwrap()
    }

    let s = String::from("abc");
    let peek = Peek::new(s.as_str());
    let s1 = retrieve_via_as_str(&peek);
    assert_eq!(s1, "abc");
    let s2 = retrieve_via_get(&peek);
    assert_eq!(s2, "abc");
}

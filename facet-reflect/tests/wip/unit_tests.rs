use facet::Facet;
use facet_reflect::Wip;

#[derive(Facet, PartialEq, Eq, Debug)]
struct Outer {
    name: String,
    inner: Inner,
}

#[derive(Facet, PartialEq, Eq, Debug)]
struct Inner {
    x: i32,
    b: i32,
}

#[test]
fn wip_nested() -> eyre::Result<()> {
    facet_testhelpers::setup();

    let mut wip = Wip::alloc::<Outer>();
    wip.field_named("name")?
        .put(String::from("Hello, world!"))?
        .pop()?
        .field_named("inner")?
        .field_named("x")?
        .put(42)?
        .pop()?
        .field_named("b")?
        .put(43)?
        .pop()?
        .pop()?
        .finish()?;
    let v = wip.build()?.materialize::<Outer>()?;

    assert_eq!(
        v,
        Outer {
            name: String::from("Hello, world!"),
            inner: Inner { x: 42, b: 43 }
        }
    );

    Ok(())
}

#[test]
fn readme_sample() -> eyre::Result<()> {
    facet_testhelpers::setup();

    use facet::Facet;

    #[derive(Debug, PartialEq, Eq, Facet)]
    struct FooBar {
        foo: u64,
        bar: String,
    }

    let mut foo_bar = Wip::alloc::<FooBar>();
    foo_bar
        .field_named("foo")?
        .put(42u64)?
        .pop()?
        .field_named("bar")?
        .put(String::from("Hello, World!"))?
        .pop()?
        .finish()?;
    let foo_bar = foo_bar.build()?.materialize::<FooBar>()?;

    // Now we can use the constructed value
    println!("{}", foo_bar.bar);

    Ok(())
}

#[test]
fn lifetimes() -> eyre::Result<()> {
    #[derive(Debug, Facet)]
    struct Foo<'a> {
        s: &'a str,
    }

    let mut wip = Wip::alloc::<Foo>();
    {
        let s = "abc".to_string();
        let foo = Foo { s: &s };
        wip.put(foo)?.finish()?;
    };

    let v = wip.build()?.materialize::<Foo>()?;
    dbg!(v);

    Ok(())
}

#[test]
fn lifetimes_simple() {
    struct Owned;
    struct Borrowed<'a>(&'a mut Owned);

    impl<'a> Borrowed<'a> {
        fn put<T: 'a>(self, t: T) {}
    }

    struct Ref<'a>(&'a str);

    let mut owned = Owned;
    let borrowed = Borrowed(&mut owned);

    {
        let s = String::from("abc");
        let r = Ref(s.as_str());
        borrowed.put(r);
    }

    // dangling!
}

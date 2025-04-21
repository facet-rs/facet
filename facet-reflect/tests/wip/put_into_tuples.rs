use facet::Facet;
use facet_reflect::Wip;

#[test]
fn test_put_into_tuples() -> eyre::Result<()> {
    facet_testhelpers::setup();

    type T = (u32, String, bool);

    let mut wip = Wip::alloc::<T>();
    wip = wip.put::<u32>(42)?;
    wip = wip.put::<String>("hello".to_string())?;
    wip = wip.put::<bool>(true)?;
    let t = wip.build()?.materialize::<T>()?;
    assert_eq!(t, (42, "hello".to_string(), true));

    Ok(())
}

#[test]
fn test_put_into_struct_like_tuple() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct Point(u32, u32, String);

    let mut wip = Wip::alloc::<Point>();
    wip = wip.put::<u32>(10)?;
    wip = wip.put::<u32>(20)?;
    wip = wip.put::<String>("point".to_string())?;
    let point = wip.build()?.materialize::<Point>()?;
    assert_eq!(point.0, 10);
    assert_eq!(point.1, 20);
    assert_eq!(point.2, "point");

    Ok(())
}

#[test]
fn test_put_into_enum_variant_with_tuple_fields() -> eyre::Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Message {
        Quit,
        Move { x: i32, y: i32 },
        Write(String),
        ChangeColor(i32, i32, i32),
    }

    // Test with the ChangeColor variant that has tuple-like fields
    let mut wip = Wip::alloc::<Message>();
    wip = wip.variant_named("ChangeColor")?;
    wip = wip.put::<i32>(255)?;
    wip = wip.put::<i32>(0)?;
    wip = wip.put::<i32>(255)?;
    let message = wip.build()?.materialize::<Message>()?;

    match message {
        Message::ChangeColor(r, g, b) => {
            assert_eq!(r, 255);
            assert_eq!(g, 0);
            assert_eq!(b, 255);
        }
        _ => panic!("Expected ChangeColor variant"),
    }

    // Test with the Write variant that has a single tuple field
    let mut wip = Wip::alloc::<Message>();
    wip = wip.variant_named("Write")?;
    wip = wip.put::<String>("hello".to_string())?;
    let message = wip.build()?.materialize::<Message>()?;

    match message {
        Message::Write(s) => {
            assert_eq!(s, "hello");
        }
        _ => panic!("Expected Write variant"),
    }

    Ok(())
}

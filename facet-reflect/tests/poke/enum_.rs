use facet_core as facet;
use facet_derive::Facet;
use facet_poke::PokeUninit;
use facet_pretty::FacetPretty as _;

#[ctor::ctor]
fn init_logger() {
    color_backtrace::install();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
}

#[test]
fn build_enum() {
    #[derive(Facet, PartialEq, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum FooBar {
        Unit,
        Foo(u32),
        Bar(String),
        StructLike { a: u32, b: String },
    }

    let v = FooBar::Unit;
    eprintln!("{}", v.pretty());

    let v = FooBar::Foo(42);
    eprintln!("{}", v.pretty());

    let v = FooBar::Bar("junjito".into());
    eprintln!("{}", v.pretty());

    let v = FooBar::StructLike {
        a: 1,
        b: "Hello".into(),
    };
    eprintln!("{}", v.pretty());

    {
        // now let's try to build an enum variant with a poke
        let (poke, guard) = PokeUninit::alloc::<FooBar>();
        let pe = poke.into_enum();
        let pe = pe.set_variant_by_name("Unit").unwrap();
        let v = pe.build::<FooBar>(Some(guard));
        assert_eq!(v, FooBar::Unit);
    }

    {
        // Build the Foo variant with a u32 value
        let (poke, guard) = PokeUninit::alloc::<FooBar>();
        let pe = poke.into_enum();
        let mut pe = pe.set_variant_by_name("Foo").unwrap();
        unsafe {
            let poke_field = pe.tuple_field(0).unwrap();
            poke_field.into_value().put(43_u32);
            pe.mark_initialized(0);
        }
        let v = pe.build::<FooBar>(Some(guard));
        assert_eq!(v, FooBar::Foo(43));
    }

    {
        // Build the Bar variant with a String value
        let (poke, guard) = PokeUninit::alloc::<FooBar>();
        let pe = poke.into_enum();
        let mut pe = pe.set_variant_by_name("Bar").unwrap();
        unsafe {
            let poke_field = pe.tuple_field(0).unwrap();
            poke_field.into_value().put(String::from("junjito"));
            pe.mark_initialized(0);
        }
        let v = pe.build::<FooBar>(Some(guard));
        assert_eq!(v, FooBar::Bar("junjito".into()));
    }

    {
        // Build the StructLike variant with fields
        let (poke, guard) = PokeUninit::alloc::<FooBar>();
        let pe = poke.into_enum();
        let mut pe = pe.set_variant_by_name("StructLike").unwrap();
        unsafe {
            let (index_a, poke_a) = pe.field_by_name("a").unwrap();
            poke_a.into_value().put(1_u32);
            pe.mark_initialized(index_a);

            let (index_b, poke_b) = pe.field_by_name("b").unwrap();
            poke_b.into_value().put(String::from("Hello"));
            pe.mark_initialized(index_b);
        }
        let v = pe.build::<FooBar>(Some(guard));
        assert_eq!(
            v,
            FooBar::StructLike {
                a: 1,
                b: "Hello".into()
            }
        );
    }
}

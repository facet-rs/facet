use facet_poke::{Poke, PokeUninit};
use facet_pretty::FacetPretty as _;

#[test]
fn poke_option() {
    // Test creating a None value
    let (poke, guard) = PokeUninit::alloc::<Option<i32>>();
    let po = poke.into_option();
    let po = unsafe { po.init_none() };
    let option: Option<i32> = po.build(Some(guard));
    assert_eq!(option, None);

    // Now test creating a Some value
    let (poke, guard) = PokeUninit::alloc::<Option<i32>>();
    let po = poke.into_option();
    let po = unsafe { po.put(42) };
    let mut option: Option<i32> = po.build(Some(guard));
    assert_eq!(option, Some(42));

    // Now try replacing the option
    let poke = Poke::borrow(&mut option);
    poke.into_option().replace_with_none();
    assert_eq!(option, None);

    // Test replacing with a Some value
    let poke = Poke::borrow(&mut option);
    poke.into_option().replace_with_some(24);
    assert_eq!(option, Some(24));

    // Test pretty printing the option
    eprintln!("{}", option.pretty());
}

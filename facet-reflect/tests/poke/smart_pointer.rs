use std::sync::Arc;

use facet_reflect::PokeValueUninit;

#[test]
fn build_arc() {
    facet_testhelpers::setup();

    let (poke, _guard) = PokeValueUninit::alloc::<Arc<String>>();
    let po = poke.into_smart_pointer().unwrap();
    let po = po.from_t(String::from("Hello, World!")).unwrap();
    {
        let borrowed = po.try_borrow().unwrap();
        println!("borrowed: {}", borrowed);
    }

    let a: Arc<String> = po.build_in_place();
    println!("string: {}", a);
}

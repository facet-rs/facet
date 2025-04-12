use std::sync::Arc;

use facet_reflect::PokeUninit;

#[test]
fn build_arc() {
    facet_testhelpers::setup();

    let (poke, _guard) = PokeUninit::alloc::<Arc<String>>();
    let po = poke.into_smart_pointer();
    let po = po.from_t(String::from("Hello, World!")).unwrap();
    {
        let borrowed = po.try_borrow().unwrap();
        println!("borrowed: {}", borrowed);
    }

    let a: Arc<String> = po.build_in_place();
    println!("string: {}", a);
}

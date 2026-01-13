use facet::Facet;

#[derive(Facet, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum MyEnum {
    Foo,
    Bar { x: i32 },
}

#[derive(Facet, Debug)]
struct Test {
    value: MyEnum,
}

fn main() {
    // Test simple tag (unit variant)
    let input1 = "value @Foo";
    match facet_styx::from_str::<Test>(input1) {
        Ok(v) => println!("Test 1 (unit tag) OK: {:?}", v),
        Err(e) => println!("Test 1 (unit tag) ERR: {}", e),
    }

    // Test tag with struct payload
    let input2 = "value @Bar{x 42}";
    match facet_styx::from_str::<Test>(input2) {
        Ok(v) => println!("Test 2 (struct tag) OK: {:?}", v),
        Err(e) => println!("Test 2 (struct tag) ERR: {}", e),
    }
}

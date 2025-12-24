use facet::Facet;
use facet_format_postcard::to_vec;

#[derive(Facet)]
struct Test {
    value: Result<i32, String>,
}

fn main() {
    let ok_case = Test { value: Ok(42) };
    let ok_bytes = to_vec(&ok_case).unwrap();
    println!("Ok(42) bytes: {:?}", ok_bytes);

    let err_case = Test {
        value: Err("error".to_string()),
    };
    let err_bytes = to_vec(&err_case).unwrap();
    println!("Err('error') bytes: {:?}", err_bytes);
}

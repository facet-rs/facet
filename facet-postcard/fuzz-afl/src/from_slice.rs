#[macro_use]
extern crate afl;

use facet_postcard::from_slice;

fn main() {
    fuzz!(|data: &[u8]| {
        let _ = from_slice::<Vec<u8>>(data);
        let _ = from_slice::<String>(data);
        let _ = from_slice::<u64>(data);
    });
}

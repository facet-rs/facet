use facet_pretty::FacetPretty;

fn main() {
    let mut file = std::fs::File::open("/dev/urandom").expect("Failed to open /dev/urandom");
    let mut bytes = vec![0u8; 128];
    std::io::Read::read_exact(&mut file, &mut bytes).expect("Failed to read from /dev/urandom");
    println!("{}", bytes.pretty());
}

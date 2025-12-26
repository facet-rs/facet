use postcard::to_allocvec;

fn print_hex(name: &str, bytes: &[u8]) {
    let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    println!("{}: {}", name, hex);
}

fn main() {
    println!("=== Unsigned Varint Test Vectors ===");

    // Test unsigned values
    let unsigned_values: &[(&str, u64)] = &[
        ("0", 0),
        ("1", 1),
        ("127", 127),
        ("128", 128),
        ("255", 255),
        ("256", 256),
        ("16383", 16383),
        ("16384", 16384),
        ("u32::MAX", u32::MAX as u64),
        ("u64::MAX", u64::MAX),
    ];

    for (name, value) in unsigned_values {
        let bytes = to_allocvec(value).expect("serialization failed");
        print_hex(name, &bytes);
    }

    println!();
    println!("=== Signed Varint Test Vectors (zigzag encoded) ===");

    // Test signed values - postcard uses zigzag encoding for signed integers
    let signed_values: &[(&str, i64)] = &[
        ("0", 0),
        ("-1", -1),
        ("1", 1),
        ("-64", -64),
        ("64", 64),
        ("i32::MIN", i32::MIN as i64),
        ("i32::MAX", i32::MAX as i64),
    ];

    for (name, value) in signed_values {
        let bytes = to_allocvec(value).expect("serialization failed");
        print_hex(name, &bytes);
    }
}

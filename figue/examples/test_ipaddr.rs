use facet::Facet;
use facet_core::{Type, UserType};
use std::net::{IpAddr, Ipv4Addr};

fn main() {
    // Check IpAddr's shape
    let shape = IpAddr::SHAPE;
    println!("IpAddr shape: {:?}", shape.type_identifier);
    println!("IpAddr scalar_type: {:?}", shape.scalar_type());
    println!("IpAddr type_ops: {:?}", shape.type_ops.is_some());
    println!("IpAddr ty: {:?}", shape.ty);

    // Check if it's an enum
    if let Type::User(UserType::Enum(e)) = &shape.ty {
        println!("IpAddr is an enum with {} variants", e.variants.len());
        for v in e.variants {
            println!("  Variant: {}", v.name);
        }
    }

    // Check if IpAddr can be serialized
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Try to serialize it
    match facet_json::to_string(&ip) {
        Ok(s) => println!("Serialized IpAddr: {}", s),
        Err(e) => println!("Failed to serialize IpAddr: {:?}", e),
    }

    // Check what happens with a string
    let _ip_str = "127.0.0.1";
    println!("\nNow checking string -> IpAddr conversion:");
    println!(
        "String shape scalar_type: {:?}",
        <&str>::SHAPE.scalar_type()
    );
}

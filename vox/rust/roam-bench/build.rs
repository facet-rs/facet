fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "protobuf")]
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/adder.proto"], &["proto"])?;
    Ok(())
}

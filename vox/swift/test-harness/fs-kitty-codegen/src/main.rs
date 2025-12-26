//! Generate Swift code for fs-kitty-proto

use rapace::registry::ServiceRegistry;
use rapace_swift_codegen::SwiftCodegen;

fn main() {
    println!("=== fs-kitty Swift Codegen ===\n");

    // Register the Vfs service
    ServiceRegistry::with_global_mut(|registry| {
        fs_kitty_proto::vfs_register(registry);
    });

    // Generate Swift code
    let mut codegen = SwiftCodegen::new();
    ServiceRegistry::with_global(|registry| {
        println!(
            "Generating Swift for {} services, {} methods\n",
            registry.service_count(),
            registry.method_count()
        );
        codegen.generate_from_registry(registry);
    });

    // Output the generated Swift
    let swift_code = codegen.into_output();
    println!("Generated Swift code:\n");
    println!("================================================================================");
    println!("{}", swift_code);
    println!("================================================================================");

    // Also write to file
    let output_path = "VfsClient.swift";
    std::fs::write(output_path, &swift_code).expect("Failed to write output file");
    println!("\n✓ Written to {}", output_path);
}

//! Code generation tool for dibs services.
//!
//! Generates TypeScript client code for SquelService and DibsService.

use clap::Parser;
use dibs_proto::{dibs_service_service_detail, squel_service_service_detail};
use roam_codegen::targets::typescript::generate_service;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dibs-codegen")]
#[command(about = "Generate client code for dibs services")]
struct Args {
    /// Output directory for generated files
    #[arg(short, long, default_value = ".")]
    output: PathBuf,

    /// Generate TypeScript client
    #[arg(long)]
    typescript: bool,

    /// Which service to generate (squel, dibs, or all)
    #[arg(long, default_value = "all")]
    service: String,
}

fn main() {
    let args = Args::parse();

    if !args.typescript {
        eprintln!("No output format specified. Use --typescript");
        std::process::exit(1);
    }

    // Create output directory if needed
    if !args.output.exists() {
        fs::create_dir_all(&args.output).expect("Failed to create output directory");
    }

    let generate_squel = args.service == "all" || args.service == "squel";
    let generate_dibs = args.service == "all" || args.service == "dibs";

    if args.typescript {
        if generate_squel {
            let squel_detail = squel_service_service_detail();
            let squel_ts = generate_service(&squel_detail);
            let squel_path = args.output.join("squel-service.ts");
            fs::write(&squel_path, &squel_ts).expect("Failed to write squel-service.ts");
            println!("Generated {}", squel_path.display());
        }

        if generate_dibs {
            let dibs_detail = dibs_service_service_detail();
            let dibs_ts = generate_service(&dibs_detail);
            let dibs_path = args.output.join("dibs-service.ts");
            fs::write(&dibs_path, &dibs_ts).expect("Failed to write dibs-service.ts");
            println!("Generated {}", dibs_path.display());
        }
    }
}

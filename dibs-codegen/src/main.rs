//! Code generation tool for dibs services.
//!
//! Generates TypeScript client code for SquelService and DibsService.

use dibs_proto::{dibs_service_service_descriptor, squel_service_service_descriptor};
use facet::Facet;
use figue as args;
use std::fs;
use std::path::PathBuf;
use vox_codegen::targets::typescript::generate_service;

#[derive(Facet)]
struct Args {
    /// Standard CLI options (--help, --version, --completions)
    #[facet(flatten)]
    builtins: args::FigueBuiltins,

    /// Output directory for generated files
    #[facet(args::named, args::short = 'o', default = ".")]
    output: PathBuf,

    /// Generate TypeScript client
    #[facet(args::named)]
    typescript: bool,

    /// Which service to generate (squel, dibs, or all)
    #[facet(args::named, default = "all")]
    service: String,
}

fn main() {
    let args: Args = args::from_std_args().unwrap();

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
            let squel_descriptor = squel_service_service_descriptor();
            let squel_ts = generate_service(squel_descriptor);
            let squel_path = args.output.join("squel-service.ts");
            fs::write(&squel_path, &squel_ts).expect("Failed to write squel-service.ts");
            println!("Generated {}", squel_path.display());
        }

        if generate_dibs {
            let dibs_descriptor = dibs_service_service_descriptor();
            let dibs_ts = generate_service(dibs_descriptor);
            let dibs_path = args.output.join("dibs-service.ts");
            fs::write(&dibs_path, &dibs_ts).expect("Failed to write dibs-service.ts");
            println!("Generated {}", dibs_path.display());
        }
    }
}

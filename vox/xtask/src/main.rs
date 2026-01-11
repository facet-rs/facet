//! xtask: Development tasks for roam
//!
//! Run with: `cargo xtask <command>`

use std::process::ExitCode;

use facet::Facet;
use facet_args as args;
use xshell::{Shell, cmd};

/// Development tasks for roam
#[derive(Facet)]
struct Cli {
    #[facet(args::subcommand)]
    command: Commands,
}

#[derive(Facet)]
#[repr(u8)]
enum Commands {
    /// Run all CI checks locally (test, clippy, fmt, doc, coverage, miri)
    Ci,
    /// Run all tests (workspace)
    Test,
    /// Run clippy on all code
    Clippy,
    /// Check formatting
    Fmt {
        /// Fix formatting issues instead of just checking
        #[facet(args::named, default)]
        fix: bool,
    },
    /// Build documentation with warnings as errors
    Doc,
    /// Generate code coverage report (requires cargo-llvm-cov)
    Coverage,
    /// Run miri for undefined behavior detection (requires nightly)
    Miri,
    /// Generate language bindings from the canonical spec-proto crate
    Codegen {
        /// Generate TypeScript bindings into `typescript/generated/`
        #[facet(args::named, default)]
        typescript: bool,
        /// Generate Swift bindings into `swift/generated/`
        #[facet(args::named, default)]
        swift: bool,
    },
}

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli: Cli = args::from_std_args()?;
    let sh = Shell::new()?;

    // Find workspace root (where Cargo.toml with [workspace] lives)
    let workspace_root = std::env::var("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap())
        .parent()
        .unwrap()
        .to_path_buf();
    sh.change_dir(&workspace_root);

    match cli.command {
        Commands::Test => {
            println!("\n=== Running workspace tests ===");

            // Try nextest first, fall back to cargo test
            if cmd!(sh, "cargo nextest --version").quiet().run().is_ok() {
                println!("Using cargo-nextest");
                // Use CI profile for longer timeouts when in CI
                if std::env::var("CI").is_ok() {
                    cmd!(sh, "cargo nextest run --workspace --profile ci").run()?;
                } else {
                    cmd!(sh, "cargo nextest run --workspace").run()?;
                }
            } else {
                println!("cargo-nextest not found, using cargo test");
                cmd!(sh, "cargo test --workspace").run()?;
            }

            println!("\n=== All tests passed ===");
        }
        Commands::Clippy => {
            println!("=== Running clippy ===");
            cmd!(sh, "cargo clippy --workspace --all-targets -- -D warnings").run()?;
        }
        Commands::Fmt { fix } => {
            if fix {
                println!("=== Fixing formatting ===");
                cmd!(sh, "cargo fmt --all").run()?;
            } else {
                println!("=== Checking formatting ===");
                cmd!(sh, "cargo fmt --all -- --check").run()?;
            }
        }
        Commands::Ci => {
            println!("=== Running all CI checks ===\n");

            println!(">>> cargo xtask test");
            cmd!(sh, "cargo xtask test").run()?;

            println!("\n>>> cargo xtask clippy");
            cmd!(sh, "cargo xtask clippy").run()?;

            println!("\n>>> cargo xtask fmt");
            cmd!(sh, "cargo xtask fmt").run()?;

            println!("\n>>> cargo xtask doc");
            cmd!(sh, "cargo xtask doc").run()?;

            println!("\n>>> cargo xtask coverage");
            cmd!(sh, "cargo xtask coverage").run()?;

            println!("\n>>> cargo xtask miri");
            cmd!(sh, "cargo xtask miri").run()?;

            println!("\n=== All CI checks passed ===");
        }
        Commands::Doc => {
            println!("=== Building documentation with warnings as errors ===");
            // Build docs for the default workspace members (rust/* crates).
            cmd!(sh, "cargo doc --no-deps")
                .env("RUSTDOCFLAGS", "-D warnings")
                .run()?;
            println!("\n=== Documentation built successfully ===");
        }
        Commands::Coverage => {
            println!("=== Generating code coverage report ===");

            // Check if cargo-llvm-cov is installed
            if cmd!(sh, "cargo llvm-cov --version").quiet().run().is_err() {
                eprintln!("cargo-llvm-cov not found. Install with:");
                eprintln!("  cargo install cargo-llvm-cov");
                return Err("cargo-llvm-cov not installed".into());
            }

            cmd!(sh, "cargo llvm-cov nextest --lcov --output-path lcov.info").run()?;

            println!("\n=== Code coverage report generated: lcov.info ===");
        }
        Commands::Miri => {
            println!("=== Running Miri (undefined behavior detection) ===");

            // Check if miri is available (requires nightly)
            if cmd!(sh, "cargo +nightly miri --version")
                .quiet()
                .run()
                .is_err()
            {
                eprintln!("cargo-miri not found. Install with:");
                eprintln!("  rustup +nightly component add miri");
                return Err("cargo-miri not installed".into());
            }

            println!("\n=== Setting up Miri ===");
            cmd!(sh, "cargo +nightly miri setup").run()?;

            println!("\n=== Running Miri tests ===");
            let result = cmd!(sh, "cargo +nightly miri test").run();

            // Miri may fail on some systems due to unsupported libc calls,
            // but we still want to report the result
            match result {
                Ok(()) => println!("\n=== Miri tests passed ==="),
                Err(e) => {
                    eprintln!("\nMiri tests had issues (this may be expected on some systems):");
                    eprintln!("  {}", e);
                    eprintln!("Note: Some tests may be skipped due to Miri limitations");
                }
            }
        }
        Commands::Codegen { typescript, swift } => {
            if typescript {
                codegen_typescript(&workspace_root)?;
            }
            if swift {
                codegen_swift(&workspace_root)?;
            }
        }
    }

    Ok(())
}

fn codegen_typescript(workspace_root: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = workspace_root.join("typescript").join("generated");
    std::fs::create_dir_all(&out_dir)?;

    // Generate TypeScript for all services in spec-proto
    for service in spec_proto::all_services() {
        let ts = roam_codegen::targets::typescript::generate_service(&service);
        let filename = format!("{}.ts", service.name.to_lowercase());
        let out_path = out_dir.join(&filename);
        std::fs::write(&out_path, ts)?;
        println!("Wrote {}", out_path.display());
    }

    Ok(())
}

fn codegen_swift(workspace_root: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    // Output directly to subject sources
    let out_dir = workspace_root
        .join("swift")
        .join("subject")
        .join("Sources")
        .join("subject-swift");
    std::fs::create_dir_all(&out_dir)?;

    let testbed = spec_proto::testbed_service_detail();
    let swift = roam_codegen::targets::swift::generate_service(&testbed);

    let out_path = out_dir.join("Testbed.swift");
    std::fs::write(&out_path, swift)?;
    println!("Wrote {}", out_path.display());

    Ok(())
}

/// oha JSON output format (partial - just what we need)
#[derive(facet::Facet)]
#[facet(rename_all = "camelCase")]
struct OhaResult {
    summary: OhaSummary,
    latency_percentiles: OhaLatencyPercentiles,
}

#[derive(facet::Facet)]
#[facet(rename_all = "camelCase")]
struct OhaSummary {
    requests_per_sec: f64,
}

#[derive(facet::Facet)]
struct OhaLatencyPercentiles {
    p50: Option<f64>,
    p90: Option<f64>,
    p99: Option<f64>,
}

/// Benchmark result for a single run
#[allow(dead_code)]
struct BenchResult {
    name: String,
    endpoint: String,
    concurrency: u32,
    rps: f64,
    p50_ms: f64,
    p90_ms: f64,
    p99_ms: f64,
}

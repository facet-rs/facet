use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        print_help();
        return ExitCode::SUCCESS;
    }

    match args[0].as_str() {
        "fuzz" => fuzz(&args[1..]),
        "fuzz-list" => fuzz_list(),
        "fuzz-coverage" => fuzz_coverage(&args[1..]),
        "test" => test(&args[1..]),
        "help" | "--help" | "-h" => {
            print_help();
            ExitCode::SUCCESS
        }
        cmd => {
            eprintln!("Unknown command: {cmd}");
            eprintln!();
            print_help();
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    eprintln!(
        r#"rapace xtask

USAGE:
    cargo xtask <COMMAND> [OPTIONS]

COMMANDS:
    fuzz [TARGET] [-- ARGS]    Run fuzz testing (requires nightly)
                               TARGET: fuzz_header_decode, fuzz_descriptor_validation
                               Without TARGET, runs all fuzz targets sequentially
                               ARGS are passed to cargo-fuzz

    fuzz-list                  List available fuzz targets

    fuzz-coverage [TARGET]     Generate coverage report for a fuzz target

    test                       Run all tests (unit + property)

    help                       Print this help message

EXAMPLES:
    cargo xtask fuzz                              # Run all fuzz targets
    cargo xtask fuzz fuzz_header_decode           # Run specific target
    cargo xtask fuzz fuzz_header_decode -- -max_total_time=60
    cargo xtask fuzz-coverage fuzz_header_decode
"#
    );
}

fn project_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).parent().unwrap().to_path_buf()
}

fn fuzz(args: &[String]) -> ExitCode {
    let root = project_root();
    let fuzz_dir = root.join("fuzz");

    // Check if cargo-fuzz is installed
    let check = Command::new("cargo")
        .args(["+nightly", "fuzz", "--version"])
        .output();

    if check.is_err() || !check.unwrap().status.success() {
        eprintln!("cargo-fuzz not found. Install with:");
        eprintln!("  cargo install cargo-fuzz");
        return ExitCode::FAILURE;
    }

    // Parse arguments
    let (target, fuzz_args) = if args.is_empty() {
        (None, Vec::new())
    } else if args[0] == "--" {
        (None, args[1..].to_vec())
    } else {
        let dash_pos = args.iter().position(|a| a == "--");
        match dash_pos {
            Some(pos) => (Some(args[0].clone()), args[pos + 1..].to_vec()),
            None => (Some(args[0].clone()), Vec::new()),
        }
    };

    let targets = match target {
        Some(t) => vec![t],
        None => {
            // Run all targets
            vec![
                "fuzz_header_decode".to_string(),
                "fuzz_descriptor_validation".to_string(),
            ]
        }
    };

    for target in targets {
        eprintln!("==> Fuzzing target: {target}");

        let mut cmd = Command::new("cargo");
        cmd.current_dir(&fuzz_dir)
            .args(["+nightly", "fuzz", "run", &target]);

        if !fuzz_args.is_empty() {
            cmd.arg("--");
            cmd.args(&fuzz_args);
        }

        let status = cmd.status();

        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                eprintln!("Fuzz target {target} exited with: {s}");
                // Don't fail immediately - fuzzer exits non-zero when it finds bugs
            }
            Err(e) => {
                eprintln!("Failed to run fuzz target {target}: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}

fn fuzz_list() -> ExitCode {
    let root = project_root();
    let fuzz_dir = root.join("fuzz");

    let status = Command::new("cargo")
        .current_dir(&fuzz_dir)
        .args(["+nightly", "fuzz", "list"])
        .status();

    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("Failed to list fuzz targets: {e}");
            ExitCode::FAILURE
        }
    }
}

fn fuzz_coverage(args: &[String]) -> ExitCode {
    let root = project_root();
    let fuzz_dir = root.join("fuzz");

    let target = match args.first() {
        Some(t) => t,
        None => {
            eprintln!("Usage: cargo xtask fuzz-coverage <TARGET>");
            eprintln!("Run 'cargo xtask fuzz-list' to see available targets");
            return ExitCode::FAILURE;
        }
    };

    let status = Command::new("cargo")
        .current_dir(&fuzz_dir)
        .args(["+nightly", "fuzz", "coverage", target])
        .status();

    match status {
        Ok(s) if s.success() => {
            eprintln!("Coverage generated in fuzz/coverage/{target}/");
            ExitCode::SUCCESS
        }
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("Failed to generate coverage: {e}");
            ExitCode::FAILURE
        }
    }
}

fn test(args: &[String]) -> ExitCode {
    let root = project_root();

    let mut cmd = Command::new("cargo");
    cmd.current_dir(&root).arg("test");

    if !args.is_empty() {
        cmd.args(args);
    }

    let status = cmd.status();

    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("Failed to run tests: {e}");
            ExitCode::FAILURE
        }
    }
}

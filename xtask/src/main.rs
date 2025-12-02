//! xtask for facet workspace

use std::{
    env, fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("showcases") => generate_showcases(),
        Some("help") | None => print_help(),
        Some(cmd) => {
            eprintln!("Unknown command: {cmd}");
            eprintln!();
            print_help();
            std::process::exit(1);
        }
    }
}

fn print_help() {
    eprintln!("Usage: cargo xtask <command>");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  showcases    Generate all showcase markdown files for the website");
    eprintln!("  help         Show this help message");
}

fn generate_showcases() {
    let workspace_root = workspace_root();
    let output_dir = workspace_root.join("docs/content/learn/showcases");

    fs::create_dir_all(&output_dir).expect("Failed to create output directory");

    // Find all *_showcase.rs examples
    let mut showcases = Vec::new();
    for entry in fs::read_dir(&workspace_root).expect("Failed to read workspace root") {
        let entry = entry.expect("Failed to read entry");
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let examples_dir = path.join("examples");
        if !examples_dir.exists() {
            continue;
        }

        let pkg_name = path.file_name().unwrap().to_str().unwrap().to_string();

        for example in fs::read_dir(&examples_dir).expect("Failed to read examples dir") {
            let example = example.expect("Failed to read example");
            let example_path = example.path();

            if let Some(name) = example_path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with("_showcase.rs") {
                    let example_name = name.trim_end_matches(".rs").to_string();
                    let output_name = example_name.trim_end_matches("_showcase").to_string();
                    showcases.push((pkg_name.clone(), example_name, output_name));
                }
            }
        }
    }

    showcases.sort();

    let total = showcases.len();
    println!("Generating {total} showcases in parallel...");

    // Channel to collect results
    let (tx, rx) = mpsc::channel();

    // Spawn threads for each showcase
    let handles: Vec<_> = showcases
        .into_iter()
        .map(|(pkg, example, output)| {
            let tx = tx.clone();
            let output_dir = output_dir.clone();

            thread::spawn(move || {
                let output_path = output_dir.join(format!("{output}.md"));

                let result = Command::new("cargo")
                    .args(["run", "-p", &pkg, "--example", &example, "--all-features"])
                    .env("FACET_SHOWCASE_OUTPUT", "markdown")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .output();

                let status = match result {
                    Ok(output_result) if output_result.status.success() => {
                        fs::write(&output_path, &output_result.stdout)
                            .expect("Failed to write output file");
                        Ok(())
                    }
                    Ok(_) => Err("failed".to_string()),
                    Err(e) => Err(e.to_string()),
                };

                tx.send((pkg, example, output, status)).unwrap();
            })
        })
        .collect();

    // Drop the original sender so rx.iter() terminates
    drop(tx);

    // Collect and print results
    let mut successes = 0;
    let mut failures = Vec::new();

    for (pkg, example, output, status) in rx {
        match status {
            Ok(()) => {
                println!("  {pkg}::{example} -> {output}.md");
                successes += 1;
            }
            Err(e) => {
                failures.push(format!("{pkg}::{example}: {e}"));
            }
        }
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    println!();
    println!("Generated {successes}/{total} showcases");

    if !failures.is_empty() {
        println!();
        println!("Failures:");
        for failure in failures {
            println!("  {failure}");
        }
    }
}

fn workspace_root() -> PathBuf {
    let output = Command::new("cargo")
        .args(["locate-project", "--workspace", "--message-format=plain"])
        .output()
        .expect("Failed to run cargo locate-project");

    let path = String::from_utf8(output.stdout).expect("Invalid UTF-8");
    PathBuf::from(path.trim())
        .parent()
        .expect("No parent directory")
        .to_path_buf()
}

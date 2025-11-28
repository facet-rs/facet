#!/usr/bin/env -S cargo +nightly -Zscript --quiet

---
[package]
edition = "2024"

[dependencies]
---

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;

fn main() {
    // Find the repo root by looking for Cargo.toml
    let cwd = env::current_dir().unwrap();
    let repo_root = find_repo_root(&cwd).expect("Could not find repo root (no Cargo.toml found)");
    let docs_dir = repo_root.join("docs");
    let meta_dir = docs_dir.join("meta");
    let static_dir = docs_dir.join("static");

    println!("Building facet website...");
    println!("  docs dir: {}", docs_dir.display());
    println!("  repo root: {}", repo_root.display());

    // Step 1: Build highlight.js bundle
    step("Building highlight.js bundle", || {
        run_in(&meta_dir, "pnpm", &["install"])?;
        run_in(&meta_dir, "pnpm", &["run", "build"])?;

        // Copy built assets to static directory
        let hljs_dir = static_dir.join("hljs");
        fs::create_dir_all(&hljs_dir)?;

        let dist_dir = meta_dir.join("dist");
        fs::copy(
            dist_dir.join("facet-hljs.iife.js"),
            hljs_dir.join("facet-hljs.iife.js"),
        )?;
        fs::copy(
            dist_dir.join("facet-hljs.css"),
            hljs_dir.join("facet-hljs.css"),
        )?;

        Ok(())
    });

    // Step 2: Build showcase Markdown files (in parallel)
    step("Building showcase Markdown files", || {
        let showcases_dir = docs_dir.join("content/showcases");
        fs::create_dir_all(&showcases_dir)?;

        // (package, example_name, output_filename)
        let showcases = [
            ("facet-kdl", "kdl_showcase", "kdl"),
            ("facet-json", "json_showcase", "json"),
            ("facet-yaml", "yaml_showcase", "yaml"),
            ("facet-assert", "assert_showcase", "assert"),
        ];

        let handles: Vec<_> = showcases
            .into_iter()
            .map(|(package, example, output_name)| {
                let repo_root = repo_root.clone();
                let showcases_dir = showcases_dir.clone();
                thread::spawn(move || -> Result<&str, String> {
                    let output = Command::new("cargo")
                        .current_dir(&repo_root)
                        .args(["run", "--example", example, "-p", package])
                        .env("FACET_SHOWCASE_OUTPUT", "markdown")
                        .stderr(Stdio::null())
                        .output()
                        .map_err(|e| format!("{}: {}", example, e))?;

                    if !output.status.success() {
                        return Err(format!("{}: build failed", example));
                    }

                    let md_path = showcases_dir.join(format!("{}.md", output_name));
                    fs::write(&md_path, &output.stdout)
                        .map_err(|e| format!("{}: {}", example, e))?;
                    Ok(example)
                })
            })
            .collect();

        let mut failed = Vec::new();
        let mut succeeded = Vec::new();
        for handle in handles {
            match handle.join().unwrap() {
                Ok(name) => succeeded.push(name),
                Err(e) => failed.push(e),
            }
        }

        for name in &succeeded {
            println!("  {} ... ok", name);
        }

        if !failed.is_empty() {
            for e in &failed {
                println!("  FAILED: {}", e);
            }
            return Err("Some showcases failed to build".into());
        }

        Ok(())
    });

    // Step 3: Build with Zola
    step("Building site with Zola", || {
        run_in(&docs_dir, "zola", &["build"])?;
        Ok(())
    });

    // Step 4: Build search index with Pagefind
    step("Building search index with Pagefind", || {
        run_in(&docs_dir, "npx", &["-y", "pagefind", "--site", "public"])?;
        Ok(())
    });

    // Step 5: Check for dead links with lychee
    step("Checking for dead links", || {
        // Try to install lychee with cargo binstall first (faster), fallback to cargo install
        let lychee_available = Command::new("lychee")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if !lychee_available {
            println!("  Installing lychee...");
            let binstall_result = Command::new("cargo")
                .args(["binstall", "-y", "lychee"])
                .status();

            if binstall_result.is_err() || !binstall_result.unwrap().success() {
                println!("  cargo binstall failed, falling back to cargo install...");
                run_in(&repo_root, "cargo", &["install", "lychee"])?;
            }
        }

        let public_dir = docs_dir.join("public");
        run_in(
            &docs_dir,
            "lychee",
            &[
                "--verbose",
                "--root-dir",
                &public_dir.to_string_lossy(),
                "--remap",
                &format!("https://facet.rs file://{}", public_dir.to_string_lossy()),
                "public/**/*.html",
            ],
        )?;
        Ok(())
    });

    println!(
        "\nDone! Site built at {}",
        docs_dir.join("public").display()
    );
    println!("Run `zola serve` in the docs/ directory to preview it.");
}

fn step<F>(name: &str, f: F)
where
    F: FnOnce() -> Result<(), Box<dyn std::error::Error>>,
{
    println!("\n==> {}", name);
    if let Err(e) = f() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run_in(dir: &PathBuf, cmd: &str, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new(cmd).current_dir(dir).args(args).status()?;

    if !status.success() {
        return Err(format!("{} failed with {}", cmd, status).into());
    }
    Ok(())
}

fn find_repo_root(start: &PathBuf) -> Option<PathBuf> {
    let mut current = start.clone();
    loop {
        if current.join("Cargo.toml").exists() && current.join("docs").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

#!/usr/bin/env -S cargo +nightly -Zscript --quiet

---
[package]
edition = "2024"

[dependencies]
cargo_metadata = "0.19"
---

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;

/// A discovered showcase example
#[derive(Clone, Debug)]
struct Showcase {
    /// Package name (e.g., "facet-kdl")
    package: String,
    /// Example name (e.g., "kdl_showcase")
    example: String,
    /// Output filename without extension (e.g., "kdl")
    output_name: String,
    /// Display name for nav (e.g., "KDL")
    display_name: String,
    /// Required features for the example
    required_features: Vec<String>,
}

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

    // Step 1: Discover showcases
    let showcases = step_result("Discovering showcases", || discover_showcases(&repo_root));

    // Step 2: Build highlight.js bundle
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

    // Step 3: Build showcase Markdown files (in parallel)
    step("Building showcase Markdown files", || {
        let showcases_dir = docs_dir.join("content/showcases");
        fs::create_dir_all(&showcases_dir)?;

        let handles: Vec<_> = showcases
            .iter()
            .map(|showcase| {
                let repo_root = repo_root.clone();
                let showcases_dir = showcases_dir.clone();
                let showcase = showcase.clone();
                thread::spawn(move || -> Result<String, String> {
                    let mut args = vec![
                        "run".to_string(),
                        "--example".to_string(),
                        showcase.example.clone(),
                        "-p".to_string(),
                        showcase.package.clone(),
                    ];
                    if !showcase.required_features.is_empty() {
                        args.push("--features".to_string());
                        args.push(showcase.required_features.join(","));
                    }
                    let output = Command::new("cargo")
                        .current_dir(&repo_root)
                        .args(&args)
                        .env("FACET_SHOWCASE_OUTPUT", "markdown")
                        .stderr(Stdio::null())
                        .output()
                        .map_err(|e| format!("{}: {}", showcase.example, e))?;

                    if !output.status.success() {
                        return Err(format!("{}: build failed", showcase.example));
                    }

                    let md_path = showcases_dir.join(format!("{}.md", showcase.output_name));
                    fs::write(&md_path, &output.stdout)
                        .map_err(|e| format!("{}: {}", showcase.example, e))?;
                    Ok(showcase.example)
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

    // Step 4: Update showcase nav in template
    step("Updating showcase navigation", || {
        update_showcase_nav(&docs_dir, &showcases)
    });

    // Step 5: Build with Zola
    step("Building site with Zola", || {
        run_in(&docs_dir, "zola", &["build"])?;
        Ok(())
    });

    // Step 6: Build search index with Pagefind
    step("Building search index with Pagefind", || {
        run_in(&docs_dir, "npx", &["-y", "pagefind", "--site", "public"])?;
        Ok(())
    });

    // Step 7: Check for dead links with lychee
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

/// Discover showcase examples using cargo metadata
fn discover_showcases(repo_root: &PathBuf) -> Result<Vec<Showcase>, Box<dyn std::error::Error>> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(repo_root.join("Cargo.toml"))
        .no_deps()
        .exec()?;

    let mut showcases = Vec::new();

    for package in &metadata.packages {
        for target in &package.targets {
            if target.kind.contains(&cargo_metadata::TargetKind::Example)
                && target.name.ends_with("_showcase")
            {
                let (output_name, display_name) = showcase_names(&target.name);
                showcases.push(Showcase {
                    package: package.name.clone(),
                    example: target.name.clone(),
                    output_name,
                    display_name,
                    required_features: target.required_features.clone(),
                });
            }
        }
    }

    // Sort by display name for consistent ordering
    showcases.sort_by(|a, b| a.display_name.cmp(&b.display_name));

    println!("  Found {} showcases:", showcases.len());
    for s in &showcases {
        println!("    {} ({}) -> {}.md", s.example, s.package, s.output_name);
    }

    Ok(showcases)
}

/// Derive output filename and display name from example name
fn showcase_names(example: &str) -> (String, String) {
    // Strip _showcase suffix
    let base = example.strip_suffix("_showcase").unwrap_or(example);

    // Special cases for better naming
    let (output_name, display_name) = match base {
        "compile_errors" => ("diagnostics", "Diagnostics"),
        "kdl" => ("kdl", "KDL"),
        "json" => ("json", "JSON"),
        "yaml" => ("yaml", "YAML"),
        "assert" => ("assert", "Assert"),
        other => {
            // Default: convert underscores to hyphens for URL, title case for display
            let output = other.replace('_', "-");
            let display = title_case(other);
            return (output, display);
        }
    };

    (output_name.to_string(), display_name.to_string())
}

/// Convert snake_case to Title Case
fn title_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Update the showcase navigation in page.html
fn update_showcase_nav(
    docs_dir: &PathBuf,
    showcases: &[Showcase],
) -> Result<(), Box<dyn std::error::Error>> {
    let template_path = docs_dir.join("templates/page.html");
    let content = fs::read_to_string(&template_path)?;

    // Build the new nav links
    let nav_links: Vec<String> = showcases
        .iter()
        .map(|s| {
            format!(
                r#"            <a href="/showcases/{output}/" {{% if page.path == "/showcases/{output}/" %}}class="active"{{% endif %}}>{display}</a>"#,
                output = s.output_name,
                display = s.display_name
            )
        })
        .collect();

    let new_nav = nav_links.join("\n");

    // Find and replace the nav section
    // Look for the pattern between <nav class="format-nav"> and </nav>
    let start_marker = r#"<nav class="format-nav">"#;
    let end_marker = "</nav>";

    let start = content
        .find(start_marker)
        .ok_or("Could not find format-nav start")?;
    let nav_content_start = start + start_marker.len();
    let end = content[nav_content_start..]
        .find(end_marker)
        .ok_or("Could not find format-nav end")?
        + nav_content_start;

    let new_content = format!(
        "{}{}\n{}\n        {}{}",
        &content[..start],
        start_marker,
        new_nav,
        end_marker,
        &content[end + end_marker.len()..]
    );

    fs::write(&template_path, new_content)?;
    println!("  Updated {}", template_path.display());

    Ok(())
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

fn step_result<T, F>(name: &str, f: F) -> T
where
    F: FnOnce() -> Result<T, Box<dyn std::error::Error>>,
{
    println!("\n==> {}", name);
    match f() {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
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

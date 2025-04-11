use std::path::Path;
use std::process;
use std::{io::Write, time::Instant};

use facet_ansi::Stylize as _;
use log::{error, info, warn};
use minijinja::Environment;
use similar::{ChangeTag, TextDiff};

fn main() {
    facet_testhelpers::setup();

    let opts = Options {
        check: std::env::args().any(|arg| arg == "--check"),
    };
    let mut has_diffs = false;

    // Check if current directory has a Cargo.toml with [workspace]
    let cargo_toml_path = std::env::current_dir().unwrap().join("Cargo.toml");
    let cargo_toml_content =
        fs_err::read_to_string(cargo_toml_path).expect("Failed to read Cargo.toml");
    if !cargo_toml_content.contains("[workspace]") {
        error!("🚫 {}", "Cargo.toml does not contain [workspace] (you must run codegen from the workspace root)".red());
        panic!();
    }

    // Run all three code generation tasks in parallel
    let opts_clone1 = opts.clone();
    let tuple_impls_result = std::thread::spawn(move || {
        let mut local_has_diffs = false;
        generate_tuple_impls(&mut local_has_diffs, opts_clone1);
        local_has_diffs
    });

    let opts_clone2 = opts.clone();
    let readme_files_result = std::thread::spawn(move || {
        let mut local_has_diffs = false;
        generate_readme_files(&mut local_has_diffs, opts_clone2);
        local_has_diffs
    });

    let opts_clone3 = opts.clone();
    let sample_code_result = std::thread::spawn(move || {
        let mut local_has_diffs = false;
        copy_cargo_expand_output(&mut local_has_diffs, &opts_clone3);
        local_has_diffs
    });

    // Collect results and update has_diffs
    has_diffs |= tuple_impls_result
        .join()
        .expect("tuple_impls thread panicked");
    has_diffs |= readme_files_result
        .join()
        .expect("readme_files thread panicked");
    has_diffs |= sample_code_result
        .join()
        .expect("sample_code thread panicked");

    if opts.check && has_diffs {
        process::exit(1);
    }
}

fn copy_cargo_expand_output(has_diffs: &mut bool, opts: &Options) {
    let workspace_dir = std::env::current_dir().unwrap();
    let sample_dir = workspace_dir.join("sample");

    // Run cargo expand command and measure execution time
    let start_time = std::time::Instant::now();

    // Command 1: cargo rustc for expansion
    let cargo_expand_output = std::process::Command::new("cargo")
        .env("RUSTC_BOOTSTRAP", "1") // Necessary for -Z flags
        .current_dir(&sample_dir) // Set working directory instead of changing it
        .arg("rustc")
        .arg("--target-dir")
        .arg("/tmp/facet-codegen-expand") // Use a temporary, less intrusive target dir
        .arg("--lib") // Expand the library crate in the current directory
        .arg("--") // Separator for rustc flags
        .arg("-Zunpretty=expanded") // The flag to expand macros
        .output() // Execute and capture output
        .expect("Failed to execute cargo rustc for expansion");

    // Check if cargo rustc succeeded
    if !cargo_expand_output.status.success() {
        error!(
            "🚫 {}:\n--- stderr ---\n{}\n--- stdout ---\n{}",
            "cargo rustc expansion failed".red(),
            String::from_utf8_lossy(&cargo_expand_output.stderr).trim(),
            String::from_utf8_lossy(&cargo_expand_output.stdout).trim()
        );
        std::process::exit(1);
    }

    // Prepare the code for rustfmt: prepend the necessary lines
    let expanded_code = String::from_utf8(cargo_expand_output.stdout)
        .expect("Failed to convert cargo expand output to UTF-8 string");

    // Command 2: rustfmt to format the expanded code
    let mut rustfmt_cmd = std::process::Command::new("rustfmt")
        .arg("--edition")
        .arg("2024")
        .arg("--emit")
        .arg("stdout")
        .stdin(std::process::Stdio::piped()) // Prepare to pipe stdin
        .stdout(std::process::Stdio::piped()) // Capture stdout
        .stderr(std::process::Stdio::piped()) // Capture stderr
        .spawn()
        .expect("Failed to spawn rustfmt");

    // Write the combined code to rustfmt's stdin in a separate scope
    // to ensure stdin is closed, signaling EOF to rustfmt.
    {
        let mut stdin = rustfmt_cmd
            .stdin
            .take()
            .expect("Failed to open rustfmt stdin");
        stdin
            .write_all(expanded_code.as_bytes())
            .expect("Failed to write to rustfmt stdin");
    } // stdin is closed here

    // Wait for rustfmt to finish and collect its output
    let output = rustfmt_cmd
        .wait_with_output()
        .expect("Failed to wait for rustfmt");

    // Check if rustfmt succeeded (using the final 'output' variable)
    // Note: The original code only checked the final status, which might hide
    // the cargo expand error if rustfmt succeeds. We now check both stages.
    if !output.status.success() {
        error!(
            "🚫 {}:\n--- stderr ---\n{}\n--- stdout ---\n{}",
            "rustfmt failed".red(),
            String::from_utf8_lossy(&output.stderr).trim(),
            String::from_utf8_lossy(&output.stdout).trim()
        );
        // We still need to check the final status for the rest of the function
        // but the process might have already exited if cargo expand failed.
        // If rustfmt itself fails, exit here.
        std::process::exit(1);
    }
    let execution_time = start_time.elapsed();

    if !output.status.success() {
        error!("🚫 {}", "Cargo expand command failed".red());
        std::process::exit(1);
    }

    let expanded_code =
        String::from_utf8(output.stdout).expect("Failed to convert output to string");

    // First collect doc comments, then filter out lines we don't want
    let doc_comments = expanded_code
        .lines()
        .filter(|line| line.trim_start().starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n");

    let expanded_code = expanded_code
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("#![")
                && !trimmed.starts_with("#[facet(")
                && !trimmed.starts_with("#[macro_use]")
                && !trimmed.starts_with("//!")
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Ensure a trailing newline for consistency
    let expanded_code = if expanded_code.is_empty() {
        String::new()
    } else {
        format!("{}\n", expanded_code)
    };

    let expanded_code = format!(
        "{}\n#![allow(warnings)]\nextern crate self as facet;\nextern crate self as facet_core;\n{}",
        doc_comments, expanded_code
    );

    let expanded_code = expanded_code.replace(
        "::impls::_core::marker::PhantomData",
        "::core::marker::PhantomData",
    );

    // Write the expanded code to the target file
    let target_path = workspace_dir
        .join("facet")
        .join("src")
        .join("sample_generated_code.rs");

    *has_diffs |= write_if_different(&target_path, expanded_code.into_bytes(), opts.check);

    if opts.check {
        info!(
            "✅ Checked {} (took {:?})",
            "sample_generated_code.rs".blue().green(),
            execution_time
        );
    } else {
        info!(
            "🔧 Generated {} (took {:?})",
            "sample_generated_code.rs".blue().green(),
            execution_time
        );
    }
}

#[derive(Debug, Clone)]
struct Options {
    check: bool,
}

fn check_diff(path: &Path, new_content: &[u8]) -> bool {
    if !path.exists() {
        warn!(
            "📁 {}: {}",
            path.display(),
            "would create new file".yellow()
        );
        return true;
    }

    let old_content = fs_err::read(path).unwrap();
    if old_content != new_content {
        let old_str = String::from_utf8_lossy(&old_content);
        let new_str = String::from_utf8_lossy(new_content);

        let diff = TextDiff::from_lines(&old_str, &new_str);
        info!("📝 {}", format!("Diff for {}:", path.display()).blue());

        // Track consecutive equal lines
        let mut equal_count = 0;
        let mut last_tag = None;

        for change in diff.iter_all_changes() {
            let tag = change.tag();

            // If we're switching from Equal to another tag, and we have >=4 equal lines, show the count
            if last_tag == Some(ChangeTag::Equal) && tag != ChangeTag::Equal && equal_count > 3 {
                info!(" {} lines omitted.", equal_count - 1);
                equal_count = 0;
            }

            match tag {
                ChangeTag::Equal => {
                    if equal_count == 0 {
                        // Always show the first equal line
                        info!(" {}", change);
                    } else if equal_count < 3 {
                        // Show the 2nd and 3rd equal lines
                        info!(" {}", change);
                    }
                    equal_count += 1;
                }
                ChangeTag::Delete => {
                    equal_count = 0;
                    info!("-{}", change.red());
                }
                ChangeTag::Insert => {
                    equal_count = 0;
                    info!("+{}", change.green());
                }
            }

            last_tag = Some(tag);
        }

        // Handle case where diff ends with equal lines
        if last_tag == Some(ChangeTag::Equal) && equal_count > 3 {
            info!(" {} lines omitted.", equal_count - 1);
        }

        return true;
    }
    false
}

fn write_if_different(path: &Path, content: Vec<u8>, check_mode: bool) -> bool {
    if check_mode {
        check_diff(path, &content)
    } else {
        fs_err::write(path, content).expect("Failed to write file");
        false
    }
}

// copied from https://docs.rs/minijinja/latest/src/minijinja/functions.rs.html#305-335
fn range(
    lower: u32,
    upper: Option<u32>,
    step: Option<u32>,
) -> Result<minijinja::Value, minijinja::Error> {
    fn to_result<I: ExactSizeIterator<Item = u32> + Send + Sync + Clone + 'static>(
        i: I,
    ) -> Result<minijinja::Value, minijinja::Error> {
        if i.len() > 100000 {
            Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                "range has too many elements",
            ))
        } else {
            Ok(minijinja::Value::make_iterable(move || i.clone()))
        }
    }

    let rng = match upper {
        Some(upper) => lower..upper,
        None => 0..lower,
    };
    if let Some(step) = step {
        if step == 0 {
            Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                "cannot create range with step of 0",
            ))
        } else {
            to_result(rng.step_by(step as usize))
        }
    } else {
        to_result(rng)
    }
}

fn generate_tuple_impls(has_diffs: &mut bool, opts: Options) {
    // Start timer to measure execution time
    let start_time = std::time::Instant::now();

    // Initialize minijinja environment
    let mut env = Environment::empty();
    env.add_function("range", range);

    // Define the base path and template path
    let base_path = Path::new("facet-core/src/_trait/impls/tuples_impls.rs");
    let template_path = base_path.with_extension("rs.j2");

    // Load the template from file
    let template_content = fs_err::read_to_string(&template_path)
        .unwrap_or_else(|_| panic!("Failed to read template file: {:?}", template_path));

    // Add the template to the environment
    env.add_template("tuples_impls", &template_content)
        .expect("Failed to add template");

    // Get the template
    let template = env
        .get_template("tuples_impls")
        .expect("Failed to get template");

    // Render the template with context
    let output = template
        .render(minijinja::context! {
            max_tuple_size => 12
        })
        .expect("Failed to render template");

    // Format the generated code using rustfmt
    let mut fmt = std::process::Command::new("rustfmt")
        .arg("--edition")
        .arg("2024")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn rustfmt");

    // Write to rustfmt's stdin
    fmt.stdin
        .take()
        .expect("Failed to get stdin")
        .write_all(output.as_bytes())
        .expect("Failed to write to rustfmt stdin");

    // Get formatted output
    let formatted_output = fmt.wait_with_output().expect("Failed to wait for rustfmt");
    if !formatted_output.status.success() {
        error!(
            "🚫 {}",
            format!("rustfmt failed with exit code: {}", formatted_output.status).red()
        );
        std::process::exit(1);
    }

    *has_diffs |= write_if_different(base_path, formatted_output.stdout, opts.check);

    // Calculate execution time
    let execution_time = start_time.elapsed();

    // Print success message with execution time
    if opts.check {
        info!(
            "✅ Checked {} (took {:?})",
            "tuple implementations".blue().green(),
            execution_time
        );
    } else {
        info!(
            "🔧 Generated {} (took {:?})",
            "tuple implementations".blue().green(),
            execution_time
        );
    }
}

fn generate_readme_files(has_diffs: &mut bool, opts: Options) {
    let start = Instant::now();

    // Get all crate directories in the workspace
    let workspace_dir = std::env::current_dir().unwrap();
    let entries = fs_err::read_dir(&workspace_dir).expect("Failed to read workspace directory");

    // Create a new MiniJinja environment for README templates
    let mut env = Environment::empty();

    // Keep track of all crates we generate READMEs for
    let mut generated_crates = Vec::new();

    // Add template functions
    env.add_function("header", |crate_name: String| {
        format!(r#"

<h1>
<picture>
<source srcset="https://github.com/facet-rs/facet/raw/main/static/logo-v2/logo-only.webp">
<img src="https://github.com/facet-rs/facet/raw/main/static/logo-v2/logo-only.png" height="35" alt="Facet logo - a reflection library for Rust">
</picture> &nbsp; {0}
</h1>

[![experimental](https://img.shields.io/badge/status-experimental-yellow)](https://github.com/fasterthanlime/facet)
[![free of syn](https://img.shields.io/badge/free%20of-syn-hotpink)](https://github.com/fasterthanlime/free-of-syn)
[![crates.io](https://img.shields.io/crates/v/{0}.svg)](https://crates.io/crates/{0})
[![documentation](https://docs.rs/{0}/badge.svg)](https://docs.rs/{0})
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/{0}.svg)](./LICENSE)

_Logo by [Misiasart](https://misiasart.com/)_

Thanks to all individual and corporate sponsors, without whom this work could not exist:

<p> <a href="https://ko-fi.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/ko-fi-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/ko-fi-light.svg" height="40" alt="Ko-fi">
    </picture>
</a> <a href="https://github.com/sponsors/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/github-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/github-light.svg" height="40" alt="GitHub Sponsors">
    </picture>
</a> <a href="https://patreon.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/patreon-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/patreon-light.svg" height="40" alt="Patreon">
    </picture>
</a> <a href="https://zed.dev">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/zed-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v2/zed-light.svg" height="40" alt="Zed">
    </picture>
</a> </p>
             "#,
            crate_name
        )
    });

    env.add_function("footer", || {
        r#"
## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option."#.to_string()
    });

    // Process each crate in the workspace
    for entry in entries {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();

        // Skip non-directories and entries starting with '.' or '_'
        if !path.is_dir()
            || path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with('.') || name.starts_with('_')
            })
        {
            continue;
        }

        // Skip target directory and facet-codegen itself
        let dir_name = path.file_name().unwrap().to_string_lossy();
        if dir_name == "target" {
            continue;
        }

        // Check if this is a crate directory (has a Cargo.toml)
        let cargo_toml_path = path.join("Cargo.toml");
        if !cargo_toml_path.exists() {
            continue;
        }

        // Get crate name from directory name
        let crate_name = dir_name.to_string();

        // Special case for the main facet crate
        let template_path = if crate_name == "facet" {
            Path::new("README.md.j2").to_path_buf()
        } else {
            path.join("README.md.j2")
        };
        if template_path.exists() {
            process_readme_template(&mut env, &path, &template_path, has_diffs, opts.clone());
            generated_crates.push(crate_name);
        } else {
            error!("🚫 Missing template: {}", template_path.display().red());
            panic!();
        }
    }

    // Generate workspace README, too (which is the same as the `facet` crate)
    let workspace_template_path = workspace_dir.join("README.md.j2");
    if !workspace_template_path.exists() {
        error!(
            "🚫 {}",
            format!(
                "Template file README.md.j2 not found for workspace. We looked at {}",
                workspace_template_path.display()
            )
            .red()
        );
        panic!();
    }

    process_readme_template(
        &mut env,
        &workspace_dir,
        &workspace_template_path,
        has_diffs,
        opts.clone(),
    );

    // Add workspace to the list of generated READMEs
    generated_crates.push("workspace".to_string());

    // Print a comma-separated list of all crates we generated READMEs for
    let execution_time = start.elapsed();
    if opts.check {
        info!(
            "📚 Checked READMEs for: {} (took {:?})",
            generated_crates.join(", ").blue(),
            execution_time
        );
    } else {
        info!(
            "📚 Generated READMEs for: {} (took {:?})",
            generated_crates.join(", ").blue(),
            execution_time
        );
    }
}

fn process_readme_template(
    env: &mut Environment,
    crate_path: &Path,
    template_path: &Path,
    has_diffs: &mut bool,
    opts: Options,
) {
    // Get crate name from directory name
    let crate_name = crate_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    // Read the template
    let template_content = fs_err::read_to_string(template_path)
        .unwrap_or_else(|_| panic!("Failed to read template file: {:?}", template_path));

    // Create a template ID
    let template_id = format!("{}_readme", crate_name);

    // Add template to environment
    env.add_template(
        Box::leak(template_id.clone().into_boxed_str()),
        Box::leak(template_content.into_boxed_str()),
    )
    .unwrap_or_else(|_| panic!("Failed to add template: {}", template_id));

    // Get the template
    let template = env
        .get_template(&template_id)
        .expect("Failed to get template");

    // Render the template with context
    let output = template
        .render(minijinja::context! {
            crate_name => crate_name
        })
        .expect("Failed to render template");

    // Save the rendered content to README.md
    let readme_path = crate_path.join("README.md");
    *has_diffs |= write_if_different(&readme_path, output.into_bytes(), opts.check);
}

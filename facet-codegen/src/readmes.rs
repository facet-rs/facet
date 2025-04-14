use facet_ansi::Stylize as _;
use log::{error, info};
use std::path::Path;
use std::time::Instant;

use crate::Options;
use crate::write_if_different;

pub(crate) fn generate_readme_files(opts: Options) -> bool {
    let mut has_diffs = false;

    let start = Instant::now();

    // Get all crate directories in the workspace
    let workspace_dir = std::env::current_dir().unwrap();
    let entries = fs_err::read_dir(&workspace_dir).expect("Failed to read workspace directory");

    // Keep track of all crates we generate READMEs for
    let mut generated_crates = Vec::new();

    let template_name = "README.md.in";

    // Process each crate in the workspace
    for entry in entries {
        let entry = entry.expect("Failed to read directory entry");
        let crate_path = entry.path();

        // Skip non-directories and entries starting with '.' or '_'
        if !crate_path.is_dir()
            || crate_path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with('.') || name.starts_with('_')
            })
        {
            continue;
        }

        // Skip target directory
        let dir_name = crate_path.file_name().unwrap().to_string_lossy();
        if dir_name == "target" {
            continue;
        }

        // Check if this is a crate directory (has a Cargo.toml)
        let cargo_toml_path = crate_path.join("Cargo.toml");
        if !cargo_toml_path.exists() {
            continue;
        }

        // Get crate name from directory name
        let crate_name = dir_name.to_string();

        // Check for templates
        let template_path = if crate_name == "facet" {
            Path::new(template_name).to_path_buf()
        } else {
            crate_path.join(template_name)
        };

        if template_path.exists() {
            // Get crate name from directory name
            let crate_name = crate_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            process_readme_template(
                &crate_name,
                &crate_path,
                &template_path,
                &mut has_diffs,
                opts.clone(),
            );
            generated_crates.push(crate_name);
        } else {
            error!("🚫 Missing template: {}", template_path.display().red());
            panic!();
        }
    }

    // Generate workspace README, too (which is the same as the `facet` crate)
    let workspace_template_path = workspace_dir.join(template_name);
    if !workspace_template_path.exists() {
        error!(
            "🚫 {}",
            format!(
                "Template file {} not found for workspace. We looked at {}",
                template_name,
                workspace_template_path.display()
            )
            .red()
        );
        panic!();
    }

    process_readme_template(
        "facet",
        &workspace_dir,
        &workspace_template_path,
        &mut has_diffs,
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
    } else if has_diffs {
        info!(
            "📚 Generated READMEs for: {} (took {:?})",
            generated_crates.join(", ").blue(),
            execution_time
        );
    } else {
        info!(
            "✅ No changes to READMEs for: {} (took {:?})",
            generated_crates.join(", ").blue(),
            execution_time
        );
    }
    has_diffs
}

fn process_readme_template(
    crate_name: &str,
    crate_path: &Path,
    template_path: &Path,
    has_diffs: &mut bool,
    opts: Options,
) {
    // Read the template
    let template_content = fs_err::read_to_string(template_path)
        .unwrap_or_else(|_| panic!("Failed to read template file: {:?}", template_path));

    // Combine header, template content, and footer
    let header = generate_header(crate_name);
    let footer = generate_footer();
    let content = format!("{}\n{}\n{}", header, template_content, footer);

    // Save the rendered content to README.md
    let readme_path = crate_path.join("README.md");
    *has_diffs |= write_if_different(&readme_path, content.into_bytes(), opts.check);
}

// Define header and footer templates
fn generate_header(crate_name: &str) -> String {
    let template = include_str!("header.md");
    template.replace("{CRATE}", crate_name)
}

fn generate_footer() -> String {
    include_str!("footer.md").to_string()
}

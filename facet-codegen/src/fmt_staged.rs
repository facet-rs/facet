use std::fs;
use std::io::{self};
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub fn collect_staged_files() -> io::Result<Vec<PathBuf>> {
    // Run `git status --porcelain`
    let output = Command::new("git")
        .arg("status")
        .arg("--porcelain")
        .output()?;

    if !output.status.success() {
        panic!("Failed to run `git status --porcelain`");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut files = Vec::new();
    for line in stdout.lines() {
        // e.g. "M  src/main.rs", "A  foo.rs", "AM foo/bar.rs"
        if line.len() < 3 {
            continue;
        }
        let x = line.chars().next().unwrap();
        let y = line.chars().nth(1).unwrap();
        // We want staged (X != ' ' and X != '?'), and not dirty (Y == ' ')
        if x != ' ' && x != '?' && y == ' ' {
            // File path is after status (after 3rd char)
            let path = line[3..].to_string();
            files.push(PathBuf::from(path));
        }
    }
    Ok(files)
}

use super::write_if_different;

pub fn format_and_stage_files(files: &[PathBuf], check_mode: bool) {
    let n_workers = std::cmp::min(num_cpus::get(), files.len().max(1));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(n_workers)
        .build()
        .unwrap();

    pool.scope(|s| {
        for path in files {
            let path = path.clone();
            s.spawn(move |_| {
                // Read original contents
                let original = fs::read(&path).expect("Failed to read file");
                // Run rustfmt
                let cmd = Command::new("rustfmt")
                    .arg("--emit")
                    .arg("stdout")
                    .arg(&path)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .expect("Failed to spawn rustfmt");
                let output = cmd.wait_with_output().expect("Failed to wait on rustfmt");
                if !output.status.success() {
                    eprintln!(
                        "rustfmt failed on {}:\nstderr:\n{}\nstdout:\n{}",
                        path.display(),
                        String::from_utf8_lossy(&output.stderr),
                        String::from_utf8_lossy(&output.stdout),
                    );
                    panic!("rustfmt failed");
                }
                let formatted = output.stdout;
                if formatted != original {
                    if check_mode {
                        // In check mode, just error if there are differences
                        eprintln!("File not formatted (would change): {}", path.display());
                        panic!(
                            "rustfmt check failed: {} would be reformatted",
                            path.display()
                        );
                    } else {
                        // Write and stage the file using write_if_different
                        if write_if_different(&path, formatted, check_mode) {
                            let status = Command::new("git")
                                .arg("add")
                                .arg(&path)
                                .status()
                                .expect("Failed to run git add");
                            if !status.success() {
                                panic!("git add failed for {}", path.display());
                            }
                        }
                    }
                }
            });
        }
    });
}

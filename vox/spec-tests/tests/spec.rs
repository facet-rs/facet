//! Test orchestrator for rapace spec conformance.
//!
//! This binary:
//! 1. Lists test cases from rapace-spec-tester
//! 2. For each test, spawns both tester and subject
//! 3. Proxies stdin/stdout between them
//! 4. Reports pass/fail based on tester exit code

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;

use libtest_mimic::{Arguments, Failed, Trial};

/// Test case from the spec tester
#[derive(facet::Facet)]
struct TestCase {
    name: String,
    rules: Vec<String>,
}

fn main() {
    let args = Arguments::from_args();

    // Find binaries - they're in target/debug or target/release,
    // while we might be in target/debug/deps
    let self_exe = std::env::current_exe().expect("failed to get current exe");
    let mut bin_dir = self_exe.parent().expect("exe has no parent").to_path_buf();

    // If we're in deps/, go up one level
    if bin_dir.ends_with("deps") {
        bin_dir = bin_dir.parent().expect("deps has no parent").to_path_buf();
    }

    let tester_bin = bin_dir.join("rapace-spec-tester");
    let subject_bin = bin_dir.join("rapace-spec-subject");

    // Get test list from tester
    let output = match Command::new(&tester_bin)
        .args(["--list", "--format", "json"])
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            eprintln!("Warning: spec-tester not found ({e}), returning empty test list");
            eprintln!("Run `cargo build -p rapace-spec-tester -p rapace-spec-subject` first");
            libtest_mimic::run(&args, vec![]).exit();
        }
    };

    if !output.status.success() {
        eprintln!("spec-tester --list failed:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        std::process::exit(1);
    }

    let tests: Vec<TestCase> =
        facet_json::from_slice(&output.stdout).expect("failed to parse test list");

    eprintln!("Found {} test cases", tests.len());

    // Create trials
    let trials: Vec<Trial> = tests
        .into_iter()
        .map(|test| {
            let name = test.name.clone();
            let tester = tester_bin.clone();
            let subject = subject_bin.clone();

            Trial::test(name.clone(), move || run_test(&tester, &subject, &name))
        })
        .collect();

    libtest_mimic::run(&args, trials).exit();
}

fn run_test(tester: &std::path::Path, subject: &std::path::Path, case: &str) -> Result<(), Failed> {
    // Spawn tester
    let mut tester_proc = Command::new(tester)
        .args(["--case", case])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to spawn tester: {}", e))?;

    // Spawn subject
    let mut subject_proc = Command::new(subject)
        .args(["--case", case])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to spawn subject: {}", e))?;

    // Get handles
    let tester_stdout = tester_proc.stdout.take().unwrap();
    let mut tester_stdin = tester_proc.stdin.take().unwrap();
    let subject_stdout = subject_proc.stdout.take().unwrap();
    let mut subject_stdin = subject_proc.stdin.take().unwrap();

    // Proxy: tester.stdout -> subject.stdin
    let t2s = thread::spawn(move || {
        proxy_stream(tester_stdout, &mut subject_stdin, "tester->subject");
    });

    // Proxy: subject.stdout -> tester.stdin
    let s2t = thread::spawn(move || {
        proxy_stream(subject_stdout, &mut tester_stdin, "subject->tester");
    });

    // Wait for proxy threads
    let _ = t2s.join();
    let _ = s2t.join();

    // Wait for processes
    let tester_status = tester_proc
        .wait()
        .map_err(|e| format!("failed to wait for tester: {}", e))?;
    let subject_status = subject_proc
        .wait()
        .map_err(|e| format!("failed to wait for subject: {}", e))?;

    // Pass if tester exits 0
    if tester_status.success() {
        Ok(())
    } else {
        Err(Failed::from(format!(
            "tester exited {:?}, subject exited {:?}",
            tester_status.code(),
            subject_status.code()
        )))
    }
}

fn proxy_stream<R: Read, W: Write>(mut reader: R, writer: &mut W, _label: &str) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break, // EOF
            Ok(n) => {
                // Optional: log traffic here for debugging
                // eprintln!("[{}] {} bytes", label, n);
                if writer.write_all(&buf[..n]).is_err() {
                    break;
                }
                if writer.flush().is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

//! Real-process build of an actual upstream C project: DaveGamble/cJSON 1.7.18.
//!
//! Unlike the lua substrate test (which compiles a handful of 1–3 line stub C
//! files), this stages the *real* cJSON sources as a `Tree` and drives real host
//! `cc`/`ar` through the open real-process backend, then asserts the linked
//! output is a real ELF/Mach-O executable.
//!
//! The fetch is faked (`FakeFetchBackend` serves the pinned tree by URL); every
//! step downstream — glob, per-unit compile, static-archive, link — is the
//! production machine path running on a real toolchain.
#![cfg(all(feature = "real-process", not(target_arch = "wasm32")))]

use std::process::Command;
use std::sync::Arc;

use vix::exec::Tree;
use vix::fetch::FakeFetchBackend;
use vix::machine::Machine;
use vix::real_process::RealProcessBackend;

const CJSON_VIX: &str = include_str!("../../playgrounds/snark/src/bundled/vix/samples/cjson.vix");
const CJSON_URL: &str = "https://github.com/DaveGamble/cJSON/archive/refs/tags/v1.7.18.tar.gz";

const CJSON_H: &str = include_str!("fixtures/cjson/cJSON.h");
const CJSON_C: &str = include_str!("fixtures/cjson/cJSON.c");
const CJSON_UTILS_H: &str = include_str!("fixtures/cjson/cJSON_Utils.h");
const CJSON_UTILS_C: &str = include_str!("fixtures/cjson/cJSON_Utils.c");
const TEST_C: &str = include_str!("fixtures/cjson/test.c");

/// Serve the real cJSON tree under the pinned URL. The archive bytes are a
/// fixture stand-in (the fake backend keys on URL, not content), but the served
/// tree is the genuine upstream source.
fn cjson_fetch_backend() -> FakeFetchBackend {
    FakeFetchBackend::new().with_archive(
        CJSON_URL,
        b"cjson-1.7.18 fixture archive",
        Tree::of(&[
            ("cJSON-1.7.18/cJSON.h", CJSON_H),
            ("cJSON-1.7.18/cJSON.c", CJSON_C),
            ("cJSON-1.7.18/cJSON_Utils.h", CJSON_UTILS_H),
            ("cJSON-1.7.18/cJSON_Utils.c", CJSON_UTILS_C),
            ("cJSON-1.7.18/test.c", TEST_C),
        ]),
    )
}

#[test]
fn cjson_builds_and_links_a_real_binary() -> Result<(), String> {
    if !host_cc_available() || !host_command_available("ar") {
        return Ok(());
    }

    let backend = Arc::new(RealProcessBackend::new());
    let mut machine = Machine::load(CJSON_VIX)?
        .with_fetch_backend(cjson_fetch_backend())
        .with_exec_backend(backend);
    let target = machine.linux_target_handle();
    let out = machine.demand_i64("cjson", vec![target])?;
    let bytes = tree_bytes(&mut machine, out, "cjson_test")?;

    assert!(!bytes.is_empty(), "cjson build produced an empty output tree");
    #[cfg(target_os = "linux")]
    assert!(
        bytes.starts_with(b"\x7fELF"),
        "cjson_test is not a real ELF binary: first bytes {:02x?}",
        &bytes[..bytes.len().min(16)]
    );

    // Close the loop: the binary vix built must actually run. Write it out,
    // execute it, and confirm it prints cJSON's version banner and exits 0.
    #[cfg(target_os = "linux")]
    {
        use std::io::Write as _;
        use std::os::unix::fs::PermissionsExt as _;

        let dir = std::env::temp_dir().join("vix-cjson-demo");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let bin = dir.join("cjson_test");
        {
            let mut f = std::fs::File::create(&bin).map_err(|e| e.to_string())?;
            f.write_all(&bytes).map_err(|e| e.to_string())?;
            f.flush().map_err(|e| e.to_string())?;
        } // drop the write handle before exec — Linux refuses ETXTBSY otherwise
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;

        let output = Command::new(&bin).output().map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "vix-built cjson_test exited nonzero");
        assert!(
            stdout.contains("Version:"),
            "vix-built cjson_test did not print the cJSON version banner; stdout: {stdout}"
        );
        eprintln!("vix-built cJSON binary ran: {}", stdout.lines().next().unwrap_or(""));
    }
    Ok(())
}

fn host_cc_available() -> bool {
    Command::new("cc")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn host_command_available(name: &str) -> bool {
    Command::new(name).output().is_ok()
}

fn tree_bytes(machine: &mut Machine, handle: i64, path: &str) -> Result<Vec<u8>, String> {
    if let Some(bytes) = machine.tree_blob_entries(handle)?.remove(path) {
        return Ok(bytes);
    }
    machine
        .tree_entries(handle)?
        .remove(path)
        .map(String::into_bytes)
        .ok_or_else(|| format!("missing `{path}` in cjson output tree"))
}

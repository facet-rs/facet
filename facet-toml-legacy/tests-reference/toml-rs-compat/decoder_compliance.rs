mod decoder;

#[cfg(all(feature = "parse", feature = "display", feature = "serde"))]
fn main() {
    let invalid_ext = walkdir::WalkDir::new("tests/fixtures/invalid")
        .sort_by_file_name()
        .into_iter()
        .map(Result::unwrap)
        .filter(|e| e.path().extension() == Some(std::ffi::OsStr::new("toml")))
        .map(|e| {
            let name = e
                .path()
                .strip_prefix("tests/fixtures")
                .unwrap()
                .to_owned()
                .into();
            let fixture = std::fs::read(e.path()).unwrap().into();
            toml_test_data::Invalid { name, fixture }
        })
        .collect::<Vec<_>>();

    let decoder = decoder::Decoder;
    let mut harness = toml_test_harness::DecoderHarness::new(decoder);
    harness.version("1.0.0");
    harness.ignore([]).unwrap();
    harness.snapshot_root("tests/snapshots");
    harness.extend_invalid(invalid_ext);
    harness.test();
}

#[cfg(not(all(feature = "parse", feature = "display", feature = "serde")))]
fn main() {}

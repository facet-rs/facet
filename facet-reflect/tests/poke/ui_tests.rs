#[test]
fn does_not_compile() {
    let source = r#"
    use facet::Facet;

    fn main() {
        #[derive(Debug, Facet)]
        struct Foo<'a> {
            s: &'a str,
        }

        let (poke, _guard) = PokeValueUninit::alloc::<Foo>();
        let v = {
            let s = "abc".to_string();
            let foo = Foo { s: &s };
            poke.put(foo)
        };
        dbg!(v);
    }
    "#;

    // Create a temp directory for the Rust file
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let file_path = temp_dir.path().join("test_file.rs");

    // Write the source code to the temp file
    std::fs::write(&file_path, source).expect("Failed to write source to file");

    // Run rustc on the file
    let output = std::process::Command::new("rustc")
        .arg(&file_path)
        .output()
        .expect("Failed to execute rustc");

    // Check if compilation failed (as expected)
    let exit_code = output.status.code().unwrap_or(0);
    assert_ne!(
        exit_code, 0,
        "The code compiled successfully, but it should have failed"
    );

    // Get the error message
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Check if the error message contains "Kalamazoo"
    if !stderr.contains("Kalamazoo") {
        println!("Standard output:");
        println!("{}", String::from_utf8_lossy(&output.stdout));
        println!("Standard error:");
        println!("{}", stderr);
        panic!("The error message did not contain 'Kalamazoo'");
    }
}

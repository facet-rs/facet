use camino::Utf8PathBuf;
use facet::Facet;
use facet_postcard_legacy::{from_slice, to_vec};

#[derive(Facet, Debug, PartialEq)]
struct Config {
    name: String,
    install_dir: Utf8PathBuf,
    cache_dir: Utf8PathBuf,
    log_files: Vec<Utf8PathBuf>,
}

fn main() {
    facet_testhelpers::setup();

    let config = Config {
        name: "MyApp".to_string(),
        install_dir: Utf8PathBuf::from("/usr/local/myapp"),
        cache_dir: Utf8PathBuf::from("/var/cache/myapp"),
        log_files: vec![
            Utf8PathBuf::from("/var/log/myapp/app.log"),
            Utf8PathBuf::from("/var/log/myapp/error.log"),
        ],
    };

    println!("Original config: {:#?}", config);

    // Serialize to postcard bytes
    let bytes = to_vec(&config).unwrap();
    println!("\nSerialized to {} bytes", bytes.len());
    println!("Bytes: {:?}", bytes);

    // Deserialize back
    let deserialized: Config = from_slice(&bytes).unwrap();
    println!("\nDeserialized config: {:#?}", deserialized);

    // Verify round-trip
    assert_eq!(config, deserialized);
    println!("\n✓ Round-trip successful!");
}

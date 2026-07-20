use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::thread;
use std::time::Duration;

use flate2::Compression;
use flate2::write::GzEncoder;
use vix::fetch::FetchBackend;
use vix_fetch::HttpArchiveFetchBackend;

const ITOA_URL: &str = "https://static.crates.io/crates/itoa/itoa-1.0.15.crate";
const ITOA_SHA256: &str = "4a5f13b858c8d314ee3e8f639011f7ccefe71f97f96e50151fb991f267928e2c";

fn archive_bytes(entries: &[(&str, &str)]) -> Vec<u8> {
    let gz = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = tar::Builder::new(gz);
    for (path, contents) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, *path, contents.as_bytes())
            .expect("append fixture entry");
    }
    let gz = builder.into_inner().expect("finish tar");
    gz.finish().expect("finish gzip")
}

fn serve_once(body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind localhost fixture server");
    let addr = listener.local_addr().expect("fixture server address");
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept fixture request");
        let mut request = [0u8; 1024];
        let _ = stream.read(&mut request).expect("read fixture request");
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/gzip\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .expect("write fixture response headers");
        stream.write_all(&body).expect("write fixture body");
    });
    format!("http://{addr}/fixture.tar.gz")
}

#[test]
fn fetches_local_gzip_tar_archive() {
    let body = archive_bytes(&[
        ("mini-0.1.0/Cargo.toml", "[package]\nname = \"mini\"\n"),
        ("mini-0.1.0/src/lib.rs", "pub fn answer() -> i32 { 42 }\n"),
    ]);
    let sha256 = vix::fetch::sha256_hex(&body);
    let url = serve_once(body);

    let fetched = HttpArchiveFetchBackend::default()
        .fetch(&url, Some(&sha256))
        .expect("fetch local archive");

    assert_eq!(fetched.actual_sha256, sha256);
    assert_eq!(
        fetched.tree.entries["mini-0.1.0/src/lib.rs"],
        "pub fn answer() -> i32 { 42 }\n"
    );
}

#[test]
fn rejects_checksum_mismatch_before_tree_is_accepted() {
    let body = archive_bytes(&[("src/lib.rs", "pub fn answer() -> i32 { 42 }\n")]);
    let url = serve_once(body);

    let err = HttpArchiveFetchBackend::default()
        .fetch(&url, Some("not-the-sha256"))
        .expect_err("checksum mismatch should fail");

    assert!(err.contains("checksum mismatch"), "{err}");
}

#[test]
fn fetches_real_crates_io_archive_with_checksum() -> Result<(), String> {
    if !crates_io_reachable() {
        return Ok(());
    }

    let fetched = HttpArchiveFetchBackend::default().fetch(ITOA_URL, Some(ITOA_SHA256))?;

    assert_eq!(fetched.actual_sha256, ITOA_SHA256);
    assert!(fetched.tree.entries.contains_key("itoa-1.0.15/Cargo.toml"));
    assert!(fetched.tree.entries.contains_key("itoa-1.0.15/src/lib.rs"));
    assert!(fetched.tree.entries["itoa-1.0.15/Cargo.toml"].contains("name = \"itoa\""));
    assert!(fetched.tree.entries["itoa-1.0.15/src/lib.rs"].contains("pub struct Buffer"));

    Ok(())
}

fn crates_io_reachable() -> bool {
    let Ok(addrs) = ("static.crates.io", 443).to_socket_addrs() else {
        return false;
    };
    addrs
        .into_iter()
        .any(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok())
}

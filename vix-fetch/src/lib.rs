use std::io::Read;
use std::path::Component;

use flate2::read::GzDecoder;
use vix::exec::Tree;
use vix::fetch::{FetchBackend, FetchOutput};

#[derive(Clone)]
pub struct HttpArchiveFetchBackend {
    output: ArchiveFetchOutput,
}

#[derive(Clone)]
enum ArchiveFetchOutput {
    ExtractedTree,
    SingleFile { path: String },
}

impl Default for HttpArchiveFetchBackend {
    fn default() -> Self {
        Self::extracted_tree()
    }
}

impl HttpArchiveFetchBackend {
    pub fn extracted_tree() -> Self {
        Self {
            output: ArchiveFetchOutput::ExtractedTree,
        }
    }

    pub fn single_file(path: impl Into<String>) -> Self {
        Self {
            output: ArchiveFetchOutput::SingleFile { path: path.into() },
        }
    }
}

impl FetchBackend for HttpArchiveFetchBackend {
    fn fetch(&self, url: &str, expected_sha256: Option<&str>) -> Result<FetchOutput, String> {
        let mut response = ureq::get(url)
            .call()
            .map_err(|err| format!("fetch `{url}` failed: {err}"))?;
        let bytes = response
            .body_mut()
            .read_to_vec()
            .map_err(|err| format!("fetch `{url}` body read failed: {err}"))?;
        fetch_archive_bytes_with_output(url, expected_sha256, &bytes, &self.output)
    }
}

pub fn fetch_archive_bytes(
    url: &str,
    expected_sha256: Option<&str>,
    archive_bytes: &[u8],
) -> Result<FetchOutput, String> {
    fetch_archive_bytes_with_output(
        url,
        expected_sha256,
        archive_bytes,
        &ArchiveFetchOutput::ExtractedTree,
    )
}

fn fetch_archive_bytes_with_output(
    url: &str,
    expected_sha256: Option<&str>,
    archive_bytes: &[u8],
    output: &ArchiveFetchOutput,
) -> Result<FetchOutput, String> {
    let actual_sha256 = vix::fetch::sha256_hex(archive_bytes);
    if let Some(expected) = expected_sha256
        && expected != actual_sha256
    {
        return Err(format!(
            "fetch checksum mismatch for `{url}`: expected {expected}, got {actual_sha256}"
        ));
    }
    let tree = match output {
        ArchiveFetchOutput::ExtractedTree => extract_gzip_tar(url, archive_bytes)?,
        ArchiveFetchOutput::SingleFile { path } => {
            if path.is_empty() {
                return Err("single-file archive fetch path is empty".to_string());
            }
            Tree::of_blobs(&[(path.as_str(), archive_bytes)])
        }
    };
    Ok(FetchOutput {
        tree,
        actual_sha256,
    })
}

fn extract_gzip_tar(url: &str, archive_bytes: &[u8]) -> Result<Tree, String> {
    let decoder = GzDecoder::new(archive_bytes);
    let mut archive = tar::Archive::new(decoder);
    let mut tree = Tree::default();
    let entries = archive
        .entries()
        .map_err(|err| format!("read tar entries from `{url}` failed: {err}"))?;
    for entry in entries {
        let mut entry =
            entry.map_err(|err| format!("read tar entry from `{url}` failed: {err}"))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = normalize_archive_path(url, &entry)?;
        let mut contents = String::new();
        entry
            .read_to_string(&mut contents)
            .map_err(|err| format!("read `{path}` from `{url}` as utf-8 failed: {err}"))?;
        tree.entries.insert(path, contents);
    }
    Ok(tree)
}

fn normalize_archive_path<R: Read>(url: &str, entry: &tar::Entry<'_, R>) -> Result<String, String> {
    let path = entry
        .path()
        .map_err(|err| format!("read tar path from `{url}` failed: {err}"))?;
    let mut out = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("unsafe tar path `{}` from `{url}`", path.display()));
            }
        }
    }
    if out.is_empty() {
        return Err(format!("empty tar path from `{url}`"));
    }
    Ok(out.join("/"))
}

use std::collections::{BTreeMap, BTreeSet};

use crate::exec::Tree;

pub(super) fn archive_to_tree(bytes: &[u8]) -> Result<Tree, String> {
    let tar = gzip_decode(bytes)?;
    let mut files = Vec::new();
    super::tar::walk(&tar, "crate archive", |entry| {
        if matches!(entry.typeflag, 0 | b'0') && !entry.name.is_empty() {
            files.push((normalize_path(&entry.name), entry.contents.to_vec()));
        }
        Ok(())
    })?;

    let root = single_root(&files)?;
    let mut entries = BTreeMap::new();
    let mut blobs = BTreeMap::new();
    for (path, contents) in files {
        let path = strip_root(&path, &root).to_string();
        if path.is_empty() {
            continue;
        }
        match String::from_utf8(contents) {
            Ok(text) => {
                entries.insert(path, text);
            }
            Err(err) => {
                blobs.insert(path, err.into_bytes());
            }
        }
    }
    Ok(Tree { entries, blobs })
}

fn gzip_decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() < 18 {
        return Err("crate archive gzip stream is too short".into());
    }
    if bytes[0] != 0x1f || bytes[1] != 0x8b {
        return Err("crate archive is not gzip data".into());
    }
    if bytes[2] != 8 {
        return Err(format!(
            "crate archive gzip method {} is not deflate",
            bytes[2]
        ));
    }

    let flags = bytes[3];
    if flags & 0b1110_0000 != 0 {
        return Err(format!(
            "crate archive gzip has reserved flags 0x{flags:02x}"
        ));
    }

    let mut offset = 10usize;
    if flags & 0x04 != 0 {
        if offset + 2 > bytes.len() {
            return Err("crate archive gzip extra field is truncated".into());
        }
        let len = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) as usize;
        offset = offset
            .checked_add(2 + len)
            .ok_or_else(|| "crate archive gzip extra field overflows".to_string())?;
    }
    if flags & 0x08 != 0 {
        offset = skip_c_string(bytes, offset, "file name")?;
    }
    if flags & 0x10 != 0 {
        offset = skip_c_string(bytes, offset, "comment")?;
    }
    if flags & 0x02 != 0 {
        offset = offset
            .checked_add(2)
            .ok_or_else(|| "crate archive gzip header crc overflows".to_string())?;
    }
    if offset + 8 > bytes.len() {
        return Err("crate archive gzip payload is truncated".into());
    }

    let footer = bytes.len() - 8;
    let inflated = miniz_oxide::inflate::decompress_to_vec(&bytes[offset..footer])
        .map_err(|err| format!("crate archive deflate decode failed: {err}"))?;
    let expected_len = u32::from_le_bytes([
        bytes[footer + 4],
        bytes[footer + 5],
        bytes[footer + 6],
        bytes[footer + 7],
    ]);
    if (inflated.len() as u32) != expected_len {
        return Err(format!(
            "crate archive gzip size mismatch: decoded {} bytes, footer says {expected_len}",
            inflated.len()
        ));
    }
    Ok(inflated)
}

fn skip_c_string(bytes: &[u8], offset: usize, field: &str) -> Result<usize, String> {
    let rest = bytes
        .get(offset..)
        .ok_or_else(|| format!("crate archive gzip {field} is truncated"))?;
    let len = rest
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| format!("crate archive gzip {field} is unterminated"))?;
    offset
        .checked_add(len + 1)
        .ok_or_else(|| format!("crate archive gzip {field} overflows"))
}

fn single_root(files: &[(String, Vec<u8>)]) -> Result<String, String> {
    let roots = files
        .iter()
        .filter_map(|(path, _)| path.split_once('/').map(|(root, _)| root))
        .collect::<BTreeSet<_>>();
    match roots.len() {
        0 => Ok(String::new()),
        1 => Ok(roots.into_iter().next().expect("one root").to_string()),
        _ => Err(format!(
            "crate archive has multiple top-level roots: {roots:?}"
        )),
    }
}

fn strip_root<'a>(path: &'a str, root: &str) -> &'a str {
    if root.is_empty() {
        path
    } else {
        path.strip_prefix(root)
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or(path)
    }
}

fn normalize_path(path: &str) -> String {
    path.trim_start_matches('/').to_string()
}

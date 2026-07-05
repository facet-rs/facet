use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::exec::Tree;
use crate::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Projection {
    Layers,
    Config,
    Env,
    Entrypoint,
    Cmd,
    Files,
}

impl Projection {
    pub(super) const ALL: [Projection; 6] = [
        Projection::Layers,
        Projection::Config,
        Projection::Env,
        Projection::Entrypoint,
        Projection::Cmd,
        Projection::Files,
    ];

    pub(super) fn name(self) -> &'static str {
        match self {
            Projection::Layers => "layers",
            Projection::Config => "config",
            Projection::Env => "env",
            Projection::Entrypoint => "entrypoint",
            Projection::Cmd => "cmd",
            Projection::Files => "files",
        }
    }

    pub(super) fn to_word(self) -> i64 {
        match self {
            Projection::Layers => 0,
            Projection::Config => 1,
            Projection::Env => 2,
            Projection::Entrypoint => 3,
            Projection::Cmd => 4,
            Projection::Files => 5,
        }
    }

    pub(super) fn from_word(word: i64) -> Result<Self, String> {
        Ok(match word {
            0 => Projection::Layers,
            1 => Projection::Config,
            2 => Projection::Env,
            3 => Projection::Entrypoint,
            4 => Projection::Cmd,
            5 => Projection::Files,
            other => return Err(format!("unknown OCI projection {other}")),
        })
    }
}

#[derive(Clone)]
pub(super) struct Layout {
    tree: Tree,
    config: Value,
    layers: Vec<LayerDescriptor>,
}

#[derive(Clone)]
struct LayerDescriptor {
    digest: String,
    size: i64,
    media_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileProjection {
    pub layer_digest: String,
    pub contents: FileContents,
    pub size: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FileContents {
    Text(String),
    Blob(Vec<u8>),
}

pub(super) fn parse_layout(tree: Tree) -> Result<Layout, String> {
    let index = parse_json_blob(&tree, "index.json")?;
    let manifest_digest = first_manifest_digest(&index)?;
    let manifest = parse_json_blob(&tree, &blob_path(manifest_digest)?)?;
    let config_digest = string_field(object_field(&manifest, "config")?, "digest")?;
    let config = parse_json_blob(&tree, &blob_path(config_digest)?)?;
    let layers = array_field(&manifest, "layers")?
        .iter()
        .map(|layer| {
            Ok(LayerDescriptor {
                digest: string_field(layer, "digest")?.to_string(),
                size: int_field(layer, "size")?,
                media_type: string_field(layer, "mediaType").unwrap_or("").to_string(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Layout {
        tree,
        config,
        layers,
    })
}

pub(super) fn archive_to_tree(bytes: &[u8]) -> Result<Tree, String> {
    let mut entries = BTreeMap::new();
    super::tar::walk(bytes, "OCI archive", |entry| {
        let name = normalize_path(&entry.name);
        if matches!(entry.typeflag, 0 | b'0') && !name.is_empty() {
            let contents = String::from_utf8(entry.contents.to_vec())
                .map_err(|err| format!("OCI archive entry `{name}` is not UTF-8: {err}"))?;
            entries.insert(name, contents);
        }
        Ok(())
    })?;
    Ok(Tree {
        entries,
        blobs: BTreeMap::new(),
    })
}

pub(super) fn project(layout: &Layout, projection: Projection) -> Result<Value, String> {
    match projection {
        Projection::Layers => Ok(Value::Array(
            layout
                .layers
                .iter()
                .map(|layer| {
                    Value::Map(BTreeMap::from([
                        (
                            Value::Str("digest".to_string()),
                            Value::Str(layer.digest.clone()),
                        ),
                        (Value::Str("size".to_string()), Value::Int(layer.size)),
                        (
                            Value::Str("mediaType".to_string()),
                            Value::Str(layer.media_type.clone()),
                        ),
                    ]))
                })
                .collect(),
        )),
        Projection::Config => Ok(layout.config.clone()),
        Projection::Env => config_array(layout, "Env"),
        Projection::Entrypoint => config_array(layout, "Entrypoint"),
        Projection::Cmd => config_array(layout, "Cmd"),
        Projection::Files => Err("OCI files projection is a virtual Doc".to_string()),
    }
}

pub(super) fn project_file(layout: &Layout, path: &str) -> Result<Option<FileProjection>, String> {
    let path = normalize_path(path);
    for layer in layout.layers.iter().rev() {
        let tar = layer_bytes(layout, &layer.digest)?;
        match find_in_layer(tar, &path)? {
            LayerHit::Found(contents) => {
                return Ok(Some(FileProjection {
                    layer_digest: layer.digest.clone(),
                    size: i64::try_from(contents.len()).unwrap_or(i64::MAX),
                    contents: match String::from_utf8(contents) {
                        Ok(text) => FileContents::Text(text),
                        Err(err) => FileContents::Blob(err.into_bytes()),
                    },
                }));
            }
            LayerHit::Whiteout => return Ok(None),
            LayerHit::Miss => {}
        }
    }
    Ok(None)
}

pub(super) fn input_hash(tree: &Tree) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"vix-oci-layout-tree");
    for (path, contents) in &tree.entries {
        hasher.update([0]);
        hasher.update(
            i64::try_from(path.len())
                .expect("path length fits i64")
                .to_le_bytes(),
        );
        hasher.update(path.as_bytes());
        hasher.update(
            i64::try_from(contents.len())
                .expect("contents length fits i64")
                .to_le_bytes(),
        );
        hasher.update(contents.as_bytes());
    }
    for (path, contents) in &tree.blobs {
        hasher.update([1]);
        hasher.update(
            i64::try_from(path.len())
                .expect("path length fits i64")
                .to_le_bytes(),
        );
        hasher.update(path.as_bytes());
        hasher.update(
            i64::try_from(contents.len())
                .expect("contents length fits i64")
                .to_le_bytes(),
        );
        hasher.update(contents);
    }
    hasher.finalize().into()
}

fn parse_json_blob(tree: &Tree, path: &str) -> Result<Value, String> {
    let text = tree
        .entries
        .get(path)
        .ok_or_else(|| format!("OCI layout is missing `{path}`"))?;
    crate::data::parse_json(Value::Str(text.clone()))
}

fn first_manifest_digest(index: &Value) -> Result<&str, String> {
    let manifests = array_field(index, "manifests")?;
    let manifest = manifests
        .first()
        .ok_or_else(|| "OCI index has no manifests".to_string())?;
    string_field(manifest, "digest")
}

fn config_array(layout: &Layout, key: &str) -> Result<Value, String> {
    let config = object_field(&layout.config, "config")?;
    Ok(match map_get(config, key) {
        Some(Value::Array(values)) => Value::Array(values.clone()),
        Some(Value::Variant { .. }) | None => Value::Array(Vec::new()),
        Some(other) => {
            return Err(format!(
                "OCI config `{key}` must be an array or null, got {other:?}"
            ));
        }
    })
}

fn object_field<'a>(value: &'a Value, key: &str) -> Result<&'a Value, String> {
    map_get(value, key).ok_or_else(|| format!("JSON object is missing `{key}`"))
}

fn array_field<'a>(value: &'a Value, key: &str) -> Result<&'a [Value], String> {
    match object_field(value, key)? {
        Value::Array(values) => Ok(values),
        other => Err(format!(
            "JSON field `{key}` must be an array, got {other:?}"
        )),
    }
}

fn string_field<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    match object_field(value, key)? {
        Value::Str(value) => Ok(value),
        other => Err(format!(
            "JSON field `{key}` must be a string, got {other:?}"
        )),
    }
}

fn int_field(value: &Value, key: &str) -> Result<i64, String> {
    match object_field(value, key)? {
        Value::Int(value) => Ok(*value),
        other => Err(format!("JSON field `{key}` must be an int, got {other:?}")),
    }
}

fn map_get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    let Value::Map(map) = value else {
        return None;
    };
    map.get(&Value::Str(key.to_string()))
}

fn blob_path(digest: &str) -> Result<String, String> {
    let (algorithm, hex) = digest
        .split_once(':')
        .ok_or_else(|| format!("OCI digest `{digest}` has no algorithm"))?;
    Ok(format!("blobs/{algorithm}/{hex}"))
}

fn layer_bytes<'a>(layout: &'a Layout, digest: &str) -> Result<&'a [u8], String> {
    let path = blob_path(digest)?;
    if let Some(contents) = layout.tree.entries.get(&path) {
        return Ok(contents.as_bytes());
    }
    if let Some(contents) = layout.tree.blobs.get(&path) {
        return Ok(contents);
    }
    Err(format!("OCI layout is missing layer blob `{path}`"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LayerHit {
    Found(Vec<u8>),
    Whiteout,
    Miss,
}

fn find_in_layer(bytes: &[u8], path: &str) -> Result<LayerHit, String> {
    let mut hit = LayerHit::Miss;
    super::tar::walk(bytes, "OCI layer", |entry| {
        if !matches!(hit, LayerHit::Miss) {
            return Ok(());
        }
        if whiteout_covers(&entry.name, path) {
            hit = LayerHit::Whiteout;
        } else if normalize_path(&entry.name) == path && matches!(entry.typeflag, 0 | b'0') {
            hit = LayerHit::Found(entry.contents.to_vec());
        }
        Ok(())
    })?;
    Ok(hit)
}

fn normalize_path(path: &str) -> String {
    path.trim_start_matches('/').to_string()
}

fn whiteout_covers(entry: &str, path: &str) -> bool {
    let entry = normalize_path(entry);
    let (dir, name) = split_parent(&entry);
    if name == ".wh..wh..opq" {
        let (path_dir, _) = split_parent(path);
        return path_dir == dir;
    }
    let Some(removed) = name.strip_prefix(".wh.") else {
        return false;
    };
    let (path_dir, path_name) = split_parent(path);
    path_dir == dir && path_name == removed
}

fn split_parent(path: &str) -> (&str, &str) {
    match path.rsplit_once('/') {
        Some((dir, name)) => (dir, name),
        None => ("", path),
    }
}

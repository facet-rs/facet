//! Downloads wordfreq data and generates ie_exceptions.bin
//!
//! Run with: cargo run -p facet-singularize --bin gen-ie-exceptions

use std::{
    collections::BTreeSet,
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use facet::Facet;

fn main() {
    let count = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(316_000);

    let workspace_root = find_workspace_root();
    let word_list_path = workspace_root.join("facet-singularize/data/wordfreq_top50k.txt");
    let bin_path = workspace_root.join("facet-singularize/data/ie_exceptions.bin");
    let manual_path = workspace_root.join("facet-singularize/data/ie_exceptions.txt");

    eprintln!("→ downloading wordfreq data from PyPI");
    let word_weights = load_wordfreq_weights("en", "large");
    eprintln!("→ building frequency map ({} entries)", word_weights.len());
    let wf = wordfreq::WordFreq::new(word_weights);
    let map = wf.word_frequency_map();
    let mut entries: Vec<(&String, &wordfreq::Float)> = map.iter().collect();
    entries.sort_by(|a, b| {
        b.1.partial_cmp(a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });

    let mut file = fs::File::create(&word_list_path).expect("Failed to create wordfreq list file");
    writeln!(file, "# Generated from wordfreq word_frequency_map").unwrap();
    writeln!(file, "# count={}", count).unwrap();
    let mut top_words = BTreeSet::new();
    for (word, _) in entries.into_iter().take(count) {
        let word = word.to_ascii_lowercase();
        writeln!(file, "{}", word).unwrap();
        top_words.insert(word);
    }
    eprintln!("→ wrote top list to {}", word_list_path.display());

    let word_list = top_words;
    let manual = read_word_list(&manual_path);
    let freqs = wf.word_frequency_map();
    let exceptions = build_exceptions(&word_list, &manual, |word| freqs.get(word).copied());
    write_bin_frontcoded(&bin_path, &exceptions, 8);
    eprintln!(
        "→ generated {} -ies exceptions ({} manual) into {}",
        exceptions.len(),
        manual.len(),
        bin_path.display()
    );
}

fn find_workspace_root() -> PathBuf {
    let mut dir = std::env::current_dir().expect("Failed to get current dir");
    loop {
        if dir.join("Cargo.toml").exists() {
            let content = fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
            if content.contains("[workspace]") {
                return dir;
            }
        }
        if !dir.pop() {
            panic!("Could not find workspace root");
        }
    }
}

fn read_word_list(path: &Path) -> BTreeSet<String> {
    let content = fs::read_to_string(path).expect("Failed to read word list");
    let mut words = BTreeSet::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let word = match line.split_whitespace().next() {
            Some(word) => word,
            None => continue,
        };
        if !word.is_ascii() {
            continue;
        }
        words.insert(word.to_ascii_lowercase());
    }
    words
}

fn build_exceptions<F>(
    words: &BTreeSet<String>,
    manual: &BTreeSet<String>,
    freq_of: F,
) -> BTreeSet<String>
where
    F: Fn(&str) -> Option<wordfreq::Float>,
{
    let mut exceptions = BTreeSet::new();
    for plural in words
        .iter()
        .filter(|word| word.ends_with("ies") && word.len() > 3)
    {
        let stem = &plural[..plural.len() - 3];
        let singular_ie = format!("{stem}ie");
        let freq_ie = freq_of(&singular_ie);
        if freq_ie.is_some() {
            let singular_y = format!("{stem}y");
            let freq_y = freq_of(&singular_y);
            let include = match (freq_ie, freq_y) {
                (Some(freq_ie), Some(freq_y)) => freq_ie >= freq_y,
                (Some(_), None) => true,
                _ => false,
            };
            if include {
                exceptions.insert(plural.clone());
            }
        }
    }
    exceptions.extend(manual.iter().cloned());
    exceptions
}

fn write_bin_frontcoded(out_path: &PathBuf, words: &BTreeSet<String>, block_size: usize) {
    let mut data = Vec::new();
    let mut offsets = Vec::new();

    let mut iter = words.iter();
    let mut remaining = words.len();
    while remaining > 0 {
        offsets.push(data.len() as u32);

        let first = iter.next().expect("missing first word");
        remaining -= 1;
        write_word(&mut data, first.as_bytes());
        let mut prev = first.as_bytes();

        let in_block = block_size.saturating_sub(1).min(remaining);
        for _ in 0..in_block {
            let word = iter.next().expect("missing word");
            remaining -= 1;
            let word_bytes = word.as_bytes();
            let prefix_len = common_prefix_len(prev, word_bytes);
            let suffix = &word_bytes[prefix_len..];
            data.push(prefix_len as u8);
            data.push(suffix.len() as u8);
            data.extend_from_slice(suffix);
            prev = word_bytes;
        }
    }

    let count = words.len() as u32;
    let num_blocks = offsets.len() as u32;
    let mut out = Vec::new();
    out.extend_from_slice(b"IEF2");

    let use_u16_offsets = data.len() <= u16::MAX as usize
        && offsets
            .last()
            .map(|&offset| offset <= u16::MAX as u32)
            .unwrap_or(true);
    let flags = if use_u16_offsets { 1u8 } else { 0u8 };

    out.push(block_size as u8);
    out.push(flags);
    write_varint_u32(&mut out, count);
    write_varint_u32(&mut out, num_blocks);

    if use_u16_offsets {
        for offset in offsets {
            out.extend_from_slice(&(offset as u16).to_le_bytes());
        }
    } else {
        let mut prev = 0u32;
        for offset in offsets {
            let delta = offset - prev;
            write_varint_u32(&mut out, delta);
            prev = offset;
        }
    }

    out.extend_from_slice(&data);

    fs::write(out_path, out).expect("Failed to write ie_exceptions.bin");
}

fn write_word(out: &mut Vec<u8>, word: &[u8]) {
    if word.len() > u8::MAX as usize {
        panic!("word too long for front-coded encoding");
    }
    out.push(word.len() as u8);
    out.extend_from_slice(word);
}

fn common_prefix_len(a: &[u8], b: &[u8]) -> usize {
    let max = a.len().min(b.len());
    let mut i = 0;
    while i < max && a[i] == b[i] {
        i += 1;
    }
    i
}

fn write_varint_u32(out: &mut Vec<u8>, mut value: u32) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn load_wordfreq_weights(lang: &str, list: &str) -> Vec<(String, wordfreq::Float)> {
    eprintln!("→ fetching wordfreq wheel");
    let (wheel_data, filename) = download_wordfreq_wheel();
    eprintln!("→ extracting wordfreq/{list}_{lang}.msgpack.gz from {filename}");
    let pack = read_wordfreq_cbpack(&wheel_data, lang, list);
    let mut weights = Vec::new();
    for (index, bucket) in pack.into_iter().enumerate() {
        let freq = 10f32.powf(-(index as f32) / 100.0);
        for word in bucket {
            if word.is_ascii() {
                weights.push((word.to_ascii_lowercase(), freq));
            }
        }
    }
    weights
}

fn download_wordfreq_wheel() -> (Vec<u8>, String) {
    let url = "https://pypi.org/pypi/wordfreq/json";
    eprintln!("→ requesting {url}");
    let response = ureq::get(url)
        .call()
        .expect("Failed to fetch wordfreq metadata");
    let json = response
        .into_string()
        .expect("Failed to read wordfreq metadata");
    let meta: PypiResponse =
        facet_json::from_str(&json).expect("Failed to parse wordfreq metadata");
    let version = &meta.info.version;
    eprintln!("→ latest wordfreq version: {}", version);
    let files = meta
        .releases
        .get(version)
        .expect("Missing release files for wordfreq");
    let wheel = files
        .iter()
        .find(|file| {
            file.packagetype == "bdist_wheel" && file.filename.ends_with("py3-none-any.whl")
        })
        .expect("No py3-none-any wheel found for wordfreq");
    eprintln!("→ downloading {}", wheel.filename);

    let mut data = Vec::new();
    ureq::get(&wheel.url)
        .call()
        .expect("Failed to download wordfreq wheel")
        .into_reader()
        .read_to_end(&mut data)
        .expect("Failed to read wheel data");

    (data, wheel.filename.clone())
}

fn read_wordfreq_cbpack(wheel_data: &[u8], lang: &str, list: &str) -> Vec<Vec<String>> {
    use rc_zip_sync::ReadZip;

    let archive = wheel_data.read_zip().expect("Failed to read wheel as zip");

    let path = format!("wordfreq/data/{}_{}.msgpack.gz", list, lang);

    let entry = archive
        .entries()
        .find(|e| e.name == path)
        .unwrap_or_else(|| panic!("Missing {path} in wordfreq wheel"));

    let mut entry_data = Vec::new();
    entry
        .reader()
        .read_to_end(&mut entry_data)
        .expect("Failed to read zip entry");

    let mut gz = flate2::read::GzDecoder::new(Cursor::new(entry_data));
    let mut data = Vec::new();
    gz.read_to_end(&mut data)
        .expect("Failed to decompress wordfreq data");

    // The wordfreq cB format is: [header_map, bucket0, bucket1, ...]
    // where header_map = {format: "cB", version: 1}
    // and each bucket is an array of strings
    //
    // This is a heterogeneous array that we can't directly model with facet,
    // so we parse it manually using rmp (raw msgpack).
    use rmp::decode::{read_array_len, read_map_len};
    use std::io::Cursor;

    let mut reader = Cursor::new(&data[..]);

    // Read outer array
    let total_len = read_array_len(&mut reader).expect("expected array") as usize;
    if total_len == 0 {
        return Vec::new();
    }

    // Read and validate header map
    let header_len = read_map_len(&mut reader).expect("expected header map");
    let mut format_ok = false;
    let mut version_ok = false;
    for _ in 0..header_len {
        let key = read_str(&mut reader);
        match key.as_str() {
            "format" => {
                let val = read_str(&mut reader);
                format_ok = val == "cB";
            }
            "version" => {
                let val = read_int(&mut reader);
                version_ok = val == 1;
            }
            _ => {
                skip_value(&mut reader);
            }
        }
    }
    assert!(format_ok && version_ok, "Invalid wordfreq header");

    // Read buckets (arrays of strings)
    let mut buckets = Vec::with_capacity(total_len - 1);
    for _ in 1..total_len {
        let bucket_len = read_array_len(&mut reader).expect("expected bucket array") as usize;
        let mut words = Vec::with_capacity(bucket_len);
        for _ in 0..bucket_len {
            words.push(read_str(&mut reader));
        }
        buckets.push(words);
    }

    buckets
}

fn read_str(reader: &mut Cursor<&[u8]>) -> String {
    use rmp::decode::read_str_len;
    let len = read_str_len(reader).expect("expected string") as usize;
    let pos = reader.position() as usize;
    let slice = &reader.get_ref()[pos..pos + len];
    reader.set_position((pos + len) as u64);
    String::from_utf8_lossy(slice).into_owned()
}

fn read_int(reader: &mut Cursor<&[u8]>) -> i64 {
    use rmp::decode::read_int;
    read_int(reader).expect("expected int")
}

fn skip_value(reader: &mut Cursor<&[u8]>) {
    use rmp::Marker;
    use rmp::decode::{read_array_len, read_bin_len, read_map_len, read_str_len};

    let marker = rmp::decode::read_marker(reader).expect("expected marker");
    match marker {
        Marker::Null | Marker::True | Marker::False => {}
        Marker::FixPos(_) | Marker::FixNeg(_) | Marker::U8 | Marker::I8 => {
            if matches!(marker, Marker::U8 | Marker::I8) {
                reader.set_position(reader.position() + 1);
            }
        }
        Marker::U16 | Marker::I16 => reader.set_position(reader.position() + 2),
        Marker::U32 | Marker::I32 | Marker::F32 => reader.set_position(reader.position() + 4),
        Marker::U64 | Marker::I64 | Marker::F64 => reader.set_position(reader.position() + 8),
        Marker::FixStr(len) => reader.set_position(reader.position() + len as u64),
        Marker::Str8 | Marker::Str16 | Marker::Str32 => {
            let len = read_str_len(reader).expect("str len") as u64;
            reader.set_position(reader.position() + len);
        }
        Marker::FixArray(len) => {
            for _ in 0..len {
                skip_value(reader);
            }
        }
        Marker::Array16 | Marker::Array32 => {
            let len = read_array_len(reader).expect("array len");
            for _ in 0..len {
                skip_value(reader);
            }
        }
        Marker::FixMap(len) => {
            for _ in 0..len {
                skip_value(reader); // key
                skip_value(reader); // value
            }
        }
        Marker::Map16 | Marker::Map32 => {
            let len = read_map_len(reader).expect("map len");
            for _ in 0..len {
                skip_value(reader);
                skip_value(reader);
            }
        }
        Marker::Bin8 | Marker::Bin16 | Marker::Bin32 => {
            let len = read_bin_len(reader).expect("bin len") as u64;
            reader.set_position(reader.position() + len);
        }
        _ => panic!("unsupported marker: {:?}", marker),
    }
}

#[derive(Facet, Debug)]
struct PypiResponse {
    info: PypiInfo,
    releases: std::collections::HashMap<String, Vec<PypiFile>>,
}

#[derive(Facet, Debug)]
struct PypiInfo {
    version: String,
}

#[derive(Facet, Debug)]
struct PypiFile {
    filename: String,
    url: String,
    packagetype: String,
}

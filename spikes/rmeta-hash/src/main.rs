use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Command;

const BASE_SOURCE: &str = r#"#![allow(dead_code)]

pub struct PublicStruct {
    pub value: u32,
}

pub trait PublicTrait {
    fn trait_method(&self) -> u32;
}

impl PublicTrait for PublicStruct {
    fn trait_method(&self) -> u32 {
        self.value
    }
}

pub fn public_add(x: u32) -> u32 {
    helper_body_noise(x) + 1
}

fn helper_body_noise(x: u32) -> u32 {
    let doubled = x * 2;
    doubled + 3
}

pub fn signature_target(x: u32) -> u32 {
    x + 10
}

#[inline]
pub fn inline_generic<T: Into<u64> + Copy>(value: T) -> u64 {
    let base = value.into();
    base + 7
}

pub fn calls_generic(x: u16) -> u64 {
    inline_generic(x)
}
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SemanticExpectation {
    Stable,
    Changes,
}

#[derive(Clone, Copy, Debug)]
struct VariantSpec {
    name: &'static str,
    edit_kind: &'static str,
    expectation: SemanticExpectation,
}

const VARIANTS: &[VariantSpec] = &[
    VariantSpec {
        name: "base",
        edit_kind: "base",
        expectation: SemanticExpectation::Stable,
    },
    VariantSpec {
        name: "top_comment",
        edit_kind: "add a comment at the top",
        expectation: SemanticExpectation::Stable,
    },
    VariantSpec {
        name: "mid_whitespace",
        edit_kind: "reformat whitespace mid-file",
        expectation: SemanticExpectation::Stable,
    },
    VariantSpec {
        name: "private_body",
        edit_kind: "change a non-generic private fn body",
        expectation: SemanticExpectation::Stable,
    },
    VariantSpec {
        name: "pub_signature",
        edit_kind: "change a pub fn signature",
        expectation: SemanticExpectation::Changes,
    },
    VariantSpec {
        name: "inline_generic_body",
        edit_kind: "change a generic #[inline] fn body",
        expectation: SemanticExpectation::Changes,
    },
];

#[derive(Clone, Debug)]
struct CompilePlan {
    name: &'static str,
    toolchain_arg: Option<&'static str>,
    remap: bool,
    extra_args: &'static [&'static str],
}

const PLANS: &[CompilePlan] = &[
    CompilePlan {
        name: "stable-default",
        toolchain_arg: None,
        remap: false,
        extra_args: &[],
    },
    CompilePlan {
        name: "stable-remap-path-prefix",
        toolchain_arg: None,
        remap: true,
        extra_args: &[],
    },
    CompilePlan {
        name: "nightly-default",
        toolchain_arg: Some("+nightly-2026-05-25"),
        remap: false,
        extra_args: &[],
    },
    CompilePlan {
        name: "nightly-z-incremental-ignore-spans",
        toolchain_arg: Some("+nightly-2026-05-25"),
        remap: false,
        extra_args: &["-Z", "incremental-ignore-spans=yes"],
    },
    CompilePlan {
        name: "nightly-z-remap-cwd-prefix",
        toolchain_arg: Some("+nightly-2026-05-25"),
        remap: false,
        extra_args: &["-Z", "remap-cwd-prefix=/rmeta-spike"],
    },
    CompilePlan {
        name: "nightly-z-location-detail-none",
        toolchain_arg: Some("+nightly-2026-05-25"),
        remap: false,
        extra_args: &["-Z", "location-detail=none"],
    },
    CompilePlan {
        name: "nightly-z-span-free-formats",
        toolchain_arg: Some("+nightly-2026-05-25"),
        remap: false,
        extra_args: &["-Z", "span-free-formats=yes"],
    },
];

#[derive(Clone, Debug)]
struct Artifact {
    variant: VariantSpec,
    rmeta_path: PathBuf,
    bytes: Vec<u8>,
    info: RmetaInfo,
}

#[derive(Clone, Debug)]
struct RmetaInfo {
    len: usize,
    header_ok: bool,
    metadata_version: Option<u8>,
    crate_root_pos: Option<usize>,
    ascii: Vec<AsciiRun>,
}

#[derive(Clone, Debug)]
struct AsciiRun {
    offset: usize,
    text: String,
}

#[derive(Clone, Debug)]
struct DiffSummary {
    same: bool,
    lhs_len: usize,
    rhs_len: usize,
    common_prefix: usize,
    common_suffix: usize,
    equal_offset_ranges: Vec<Range<usize>>,
    lhs_window: Range<usize>,
    rhs_window: Range<usize>,
}

#[derive(Clone, Debug)]
struct ProjectionMask {
    baseline_len: usize,
    ranges: Vec<Range<usize>>,
}

#[derive(Clone, Debug)]
struct ProjectionResult {
    whole_hash: u64,
    projected_hash: u64,
    projected_len: usize,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("measure") | None => {
            let out_dir = args
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(default_out_dir);
            measure(&out_dir)
        }
        Some("hash") => {
            let path = args
                .next()
                .ok_or("usage: rmeta-hash-spike hash <file.rmeta>")?;
            let bytes = fs::read(&path).map_err(|err| format!("failed to read {path}: {err}"))?;
            let hash = fnv1a(&bytes);
            let info = inspect_rmeta(&bytes);
            println!("whole_hash={hash:016x}");
            println!("len={}", info.len);
            println!("header_ok={}", info.header_ok);
            if let Some(pos) = info.crate_root_pos {
                println!("crate_root_pos={pos}");
            }
            Ok(())
        }
        Some("project") => {
            let base = args.next().ok_or(
                "usage: rmeta-hash-spike project <base.rmeta> <keep-mask.txt> <target.rmeta>",
            )?;
            let mask = args.next().ok_or(
                "usage: rmeta-hash-spike project <base.rmeta> <keep-mask.txt> <target.rmeta>",
            )?;
            let target = args.next().ok_or(
                "usage: rmeta-hash-spike project <base.rmeta> <keep-mask.txt> <target.rmeta>",
            )?;
            let base_bytes =
                fs::read(&base).map_err(|err| format!("failed to read {base}: {err}"))?;
            let target_bytes =
                fs::read(&target).map_err(|err| format!("failed to read {target}: {err}"))?;
            let mask = read_mask(Path::new(&mask))?;
            let projection = projection_hash_against_base(&base_bytes, &target_bytes, &mask);
            println!("projection_hash={:016x}", projection.projected_hash);
            println!("projection_bytes={}", projection.projected_len);
            println!("whole_hash={:016x}", projection.whole_hash);
            Ok(())
        }
        Some("diff") => {
            let lhs = args
                .next()
                .ok_or("usage: rmeta-hash-spike diff <lhs.rmeta> <rhs.rmeta>")?;
            let rhs = args
                .next()
                .ok_or("usage: rmeta-hash-spike diff <lhs.rmeta> <rhs.rmeta>")?;
            let lhs_bytes = fs::read(&lhs).map_err(|err| format!("failed to read {lhs}: {err}"))?;
            let rhs_bytes = fs::read(&rhs).map_err(|err| format!("failed to read {rhs}: {err}"))?;
            let diff = diff_bytes(&lhs_bytes, &rhs_bytes);
            println!("{}", format_diff_summary(&diff, &inspect_rmeta(&lhs_bytes)));
            Ok(())
        }
        Some(other) => Err(format!(
            "unknown command {other:?}; use measure, hash, project, or diff"
        )),
    }
}

fn default_out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out")
}

fn measure(out_dir: &Path) -> Result<(), String> {
    recreate_dir(out_dir)?;
    let fixture_root = out_dir.join("fixtures");
    let artifact_root = out_dir.join("rmeta");
    let data_root = out_dir.join("data");
    fs::create_dir_all(&fixture_root)
        .map_err(|err| format!("failed to create {}: {err}", fixture_root.display()))?;
    fs::create_dir_all(&artifact_root)
        .map_err(|err| format!("failed to create {}: {err}", artifact_root.display()))?;
    fs::create_dir_all(&data_root)
        .map_err(|err| format!("failed to create {}: {err}", data_root.display()))?;

    let mut all_rows = Vec::new();
    let mut projection_rows = Vec::new();
    let mut section_rows = Vec::new();
    let mut artifact_map: BTreeMap<&'static str, Vec<Artifact>> = BTreeMap::new();

    for plan in PLANS {
        let mut artifacts = Vec::new();
        let plan_fixture_root = fixture_root.join(plan.name);
        let plan_artifact_root = artifact_root.join(plan.name);
        fs::create_dir_all(&plan_fixture_root)
            .map_err(|err| format!("failed to create {}: {err}", plan_fixture_root.display()))?;
        fs::create_dir_all(&plan_artifact_root)
            .map_err(|err| format!("failed to create {}: {err}", plan_artifact_root.display()))?;

        let mut compile_errors = Vec::new();
        for variant in VARIANTS {
            match compile_variant(plan, *variant, &plan_fixture_root, &plan_artifact_root) {
                Ok(artifact) => artifacts.push(artifact),
                Err(err) => compile_errors.push(format!("{}: {err}", variant.name)),
            }
        }

        if !compile_errors.is_empty() {
            all_rows.push(format!(
                "{}\tCOMPILE-FAILED\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                plan.name,
                "-",
                "-",
                "-",
                "-",
                "-",
                "-",
                compile_errors.join(" | ").replace('\n', " ")
            ));
            continue;
        }

        let base = artifacts
            .iter()
            .find(|artifact| artifact.variant.name == "base")
            .ok_or_else(|| format!("{} did not produce a base artifact", plan.name))?;

        for artifact in artifacts
            .iter()
            .filter(|artifact| artifact.variant.name != "base")
        {
            let diff = diff_bytes(&base.bytes, &artifact.bytes);
            let label = format_diff_summary(&diff, &base.info);
            all_rows.push(format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}..{}\t{}",
                plan.name,
                artifact.variant.name,
                artifact.variant.edit_kind,
                if diff.same { "no" } else { "yes" },
                artifact.bytes.len(),
                format_ranges(&diff.equal_offset_ranges),
                diff.lhs_window.start,
                diff.lhs_window.end,
                label.replace('\n', " ")
            ));
            for range in &diff.equal_offset_ranges {
                section_rows.push(format!(
                    "{}\t{}\t{}..{}\t{}",
                    plan.name,
                    artifact.variant.name,
                    range.start,
                    range.end,
                    section_label(range, &base.info)
                ));
            }
        }

        let mask = derive_projection_mask(base, &artifacts);
        write_mask(
            &data_root.join(format!("{}-projection-mask.txt", plan.name)),
            &mask,
        )?;

        for artifact in artifacts
            .iter()
            .filter(|artifact| artifact.variant.name != "base")
        {
            let base_projection = projection_hash_against_base(&base.bytes, &base.bytes, &mask);
            let artifact_projection =
                projection_hash_against_base(&base.bytes, &artifact.bytes, &mask);
            let projected_changed =
                base_projection.projected_hash != artifact_projection.projected_hash;
            projection_rows.push(format!(
                "{}\t{}\t{}\t{}\t{}\t{:016x}\t{:016x}\t{}",
                plan.name,
                artifact.variant.name,
                artifact.variant.edit_kind,
                expectation_name(artifact.variant.expectation),
                if projected_changed { "yes" } else { "no" },
                artifact_projection.whole_hash,
                artifact_projection.projected_hash,
                artifact_projection.projected_len
            ));
        }

        artifact_map.insert(plan.name, artifacts);
    }

    write_table(
        &data_root.join("noise-matrix.tsv"),
        "plan\tvariant\tedit_kind\trmeta_changed\trmeta_len\tequal_offset_diff_ranges\tlhs_replacement_window\tregion_summary\n",
        &all_rows,
    )?;
    write_table(
        &data_root.join("projection-matrix.tsv"),
        "plan\tvariant\tedit_kind\texpected_semantic\tprojection_changed\twhole_hash\tprojection_hash\tprojection_bytes\n",
        &projection_rows,
    )?;
    write_table(
        &data_root.join("diff-regions.tsv"),
        "plan\tvariant\trange\tregion\n",
        &section_rows,
    )?;
    write_summary(&data_root.join("summary.md"), &artifact_map)?;

    println!("wrote {}", data_root.join("noise-matrix.tsv").display());
    println!(
        "wrote {}",
        data_root.join("projection-matrix.tsv").display()
    );
    println!("wrote {}", data_root.join("diff-regions.tsv").display());
    println!("wrote {}", data_root.join("summary.md").display());
    Ok(())
}

fn compile_variant(
    plan: &CompilePlan,
    variant: VariantSpec,
    fixture_root: &Path,
    artifact_root: &Path,
) -> Result<Artifact, String> {
    let variant_dir = fixture_root.join(variant.name);
    fs::create_dir_all(&variant_dir)
        .map_err(|err| format!("failed to create {}: {err}", variant_dir.display()))?;
    let source_path = variant_dir.join("lib.rs");
    fs::write(&source_path, source_for_variant(variant.name))
        .map_err(|err| format!("failed to write {}: {err}", source_path.display()))?;

    let rmeta_path = artifact_root.join(format!("{}.rmeta", variant.name));
    let mut args = Vec::<OsString>::new();
    if let Some(toolchain_arg) = plan.toolchain_arg {
        args.push(toolchain_arg.into());
    }
    args.extend([
        "--crate-name".into(),
        "rmeta_fixture".into(),
        "--crate-type".into(),
        "lib".into(),
        "--edition".into(),
        "2024".into(),
        "--emit".into(),
        format!("metadata={}", rmeta_path.display()).into(),
        "-C".into(),
        "metadata=rmeta_spike".into(),
        "-C".into(),
        "debuginfo=0".into(),
        source_path.as_os_str().to_os_string(),
    ]);

    if plan.remap {
        args.push(
            format!(
                "--remap-path-prefix={}=/rmeta-spike",
                fixture_root.display()
            )
            .into(),
        );
    }
    for extra in plan.extra_args {
        args.push((*extra).into());
    }

    let output = Command::new("rustc")
        .args(&args)
        .output()
        .map_err(|err| format!("failed to spawn rustc for {}: {err}", variant.name))?;
    if !output.status.success() {
        return Err(format!(
            "rustc failed for {} ({})\nstdout:\n{}\nstderr:\n{}",
            variant.name,
            plan.name,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let bytes = fs::read(&rmeta_path)
        .map_err(|err| format!("failed to read {}: {err}", rmeta_path.display()))?;
    let info = inspect_rmeta(&bytes);
    Ok(Artifact {
        variant,
        rmeta_path,
        bytes,
        info,
    })
}

fn source_for_variant(name: &str) -> String {
    match name {
        "base" => BASE_SOURCE.to_owned(),
        "top_comment" => format!("// harmless top comment that shifts every following line\n{BASE_SOURCE}"),
        "mid_whitespace" => BASE_SOURCE.replace(
            "pub fn public_add(x: u32) -> u32 {\n    helper_body_noise(x) + 1\n}\n",
            "pub fn public_add( x: u32 ) -> u32\n{\n\n        helper_body_noise( x )\n            + 1\n}\n",
        ),
        "private_body" => BASE_SOURCE.replace("doubled + 3", "doubled + 5"),
        "pub_signature" => BASE_SOURCE.replace(
            "pub fn signature_target(x: u32) -> u32 {\n    x + 10\n}\n",
            "pub fn signature_target(x: u64) -> u64 {\n    x + 10\n}\n",
        ),
        "inline_generic_body" => BASE_SOURCE.replace("base + 7", "base + 11"),
        other => panic!("unknown source variant {other}"),
    }
}

fn inspect_rmeta(bytes: &[u8]) -> RmetaInfo {
    let header_ok = bytes.len() >= 8 && bytes[0..4] == *b"rust" && bytes[4..7] == [0, 0, 0];
    let metadata_version = header_ok.then_some(bytes[7]);
    let crate_root_pos = if bytes.len() >= 16 && header_ok {
        let mut raw = [0_u8; 8];
        raw.copy_from_slice(&bytes[8..16]);
        Some(u64::from_le_bytes(raw) as usize)
    } else {
        None
    };

    RmetaInfo {
        len: bytes.len(),
        header_ok,
        metadata_version,
        crate_root_pos,
        ascii: ascii_runs(bytes),
    }
}

fn ascii_runs(bytes: &[u8]) -> Vec<AsciiRun> {
    let mut runs = Vec::new();
    let mut start = None;
    for (index, byte) in bytes.iter().copied().enumerate() {
        let is_ascii = byte.is_ascii_graphic() || byte == b' ';
        match (start, is_ascii) {
            (None, true) => start = Some(index),
            (Some(run_start), false) => {
                push_ascii_run(bytes, run_start, index, &mut runs);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(run_start) = start {
        push_ascii_run(bytes, run_start, bytes.len(), &mut runs);
    }
    runs
}

fn push_ascii_run(bytes: &[u8], start: usize, end: usize, runs: &mut Vec<AsciiRun>) {
    if end.saturating_sub(start) < 6 {
        return;
    }
    if let Ok(text) = std::str::from_utf8(&bytes[start..end]) {
        runs.push(AsciiRun {
            offset: start,
            text: text.to_owned(),
        });
    }
}

fn diff_bytes(lhs: &[u8], rhs: &[u8]) -> DiffSummary {
    if lhs == rhs {
        return DiffSummary {
            same: true,
            lhs_len: lhs.len(),
            rhs_len: rhs.len(),
            common_prefix: lhs.len(),
            common_suffix: 0,
            equal_offset_ranges: Vec::new(),
            lhs_window: lhs.len()..lhs.len(),
            rhs_window: rhs.len()..rhs.len(),
        };
    }

    let common_prefix = lhs
        .iter()
        .zip(rhs.iter())
        .take_while(|(lhs_byte, rhs_byte)| lhs_byte == rhs_byte)
        .count();
    let max_suffix = lhs.len().min(rhs.len()).saturating_sub(common_prefix);
    let common_suffix = lhs
        .iter()
        .rev()
        .zip(rhs.iter().rev())
        .take(max_suffix)
        .take_while(|(lhs_byte, rhs_byte)| lhs_byte == rhs_byte)
        .count();

    let compare_len = lhs.len().min(rhs.len());
    let mut ranges = Vec::new();
    let mut current_start = None;
    for index in 0..compare_len {
        if lhs[index] != rhs[index] {
            if current_start.is_none() {
                current_start = Some(index);
            }
        } else if let Some(start) = current_start.take() {
            ranges.push(start..index);
        }
    }
    if let Some(start) = current_start {
        ranges.push(start..compare_len);
    }
    if lhs.len() != rhs.len() {
        ranges.push(compare_len..lhs.len().max(rhs.len()));
    }

    DiffSummary {
        same: false,
        lhs_len: lhs.len(),
        rhs_len: rhs.len(),
        common_prefix,
        common_suffix,
        equal_offset_ranges: merge_ranges(ranges),
        lhs_window: common_prefix..lhs.len().saturating_sub(common_suffix),
        rhs_window: common_prefix..rhs.len().saturating_sub(common_suffix),
    }
}

fn derive_projection_mask(base: &Artifact, artifacts: &[Artifact]) -> ProjectionMask {
    let mut keep = vec![true; base.bytes.len()];
    for index in 8..16.min(keep.len()) {
        keep[index] = false;
    }

    for artifact in artifacts {
        if artifact.variant.name == "base"
            || artifact.variant.expectation != SemanticExpectation::Stable
        {
            continue;
        }
        let mut matched = vec![false; base.bytes.len()];
        for (base_index, _artifact_index) in lcs_alignment(&base.bytes, &artifact.bytes) {
            matched[base_index] = true;
        }
        for (base_index, keep_byte) in keep.iter_mut().enumerate() {
            *keep_byte &= matched[base_index];
        }
    }

    ProjectionMask {
        baseline_len: base.bytes.len(),
        ranges: bools_to_ranges(&keep),
    }
}

fn projection_hash_against_base(
    base: &[u8],
    target: &[u8],
    mask: &ProjectionMask,
) -> ProjectionResult {
    let mut hasher = Fnv1a::new();
    let mut projected_len = 0;

    let alignment = if base == target {
        (0..base.len()).map(Some).collect::<Vec<_>>()
    } else {
        let mut by_base = vec![None; base.len()];
        for (base_index, target_index) in lcs_alignment(base, target) {
            by_base[base_index] = Some(target_index);
        }
        by_base
    };

    for range in &mask.ranges {
        for base_index in range.clone() {
            let Some(target_index) = alignment.get(base_index).and_then(|index| *index) else {
                hasher.update(&[0xff]);
                projected_len += 1;
                continue;
            };
            hasher.update(&target[target_index..target_index + 1]);
            projected_len += 1;
        }
    }

    ProjectionResult {
        whole_hash: fnv1a(target),
        projected_hash: hasher.finish(),
        projected_len,
    }
}

fn merge_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.retain(|range| range.start < range.end);
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
            continue;
        }
        merged.push(range);
    }
    merged
}

fn write_mask(path: &Path, mask: &ProjectionMask) -> Result<(), String> {
    let mut out = String::new();
    out.push_str(&format!("baseline_len={}\n", mask.baseline_len));
    for range in &mask.ranges {
        out.push_str(&format!("{}..{}\n", range.start, range.end));
    }
    fs::write(path, out).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn read_mask(path: &Path) -> Result<ProjectionMask, String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let mut baseline_len = None;
    let mut ranges = Vec::new();
    for line in text.lines() {
        if let Some(raw_len) = line.strip_prefix("baseline_len=") {
            baseline_len = Some(
                raw_len
                    .parse::<usize>()
                    .map_err(|err| format!("invalid baseline_len in {}: {err}", path.display()))?,
            );
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let Some((start, end)) = line.split_once("..") else {
            return Err(format!("invalid mask range {line:?} in {}", path.display()));
        };
        ranges.push(
            start
                .parse::<usize>()
                .map_err(|err| format!("invalid mask range start in {}: {err}", path.display()))?
                ..end.parse::<usize>().map_err(|err| {
                    format!("invalid mask range end in {}: {err}", path.display())
                })?,
        );
    }
    Ok(ProjectionMask {
        baseline_len: baseline_len
            .ok_or_else(|| format!("missing baseline_len in {}", path.display()))?,
        ranges,
    })
}

fn lcs_alignment(lhs: &[u8], rhs: &[u8]) -> Vec<(usize, usize)> {
    let rows = lhs.len() + 1;
    let cols = rhs.len() + 1;
    let mut dp = vec![0_u16; rows * cols];

    for row in 1..rows {
        for col in 1..cols {
            let index = row * cols + col;
            dp[index] = if lhs[row - 1] == rhs[col - 1] {
                dp[(row - 1) * cols + (col - 1)] + 1
            } else {
                dp[(row - 1) * cols + col].max(dp[row * cols + (col - 1)])
            };
        }
    }

    let mut row = lhs.len();
    let mut col = rhs.len();
    let mut pairs = Vec::with_capacity(usize::from(dp[row * cols + col]));
    while row > 0 && col > 0 {
        if lhs[row - 1] == rhs[col - 1] {
            pairs.push((row - 1, col - 1));
            row -= 1;
            col -= 1;
        } else if dp[(row - 1) * cols + col] >= dp[row * cols + (col - 1)] {
            row -= 1;
        } else {
            col -= 1;
        }
    }
    pairs.reverse();
    pairs
}

fn bools_to_ranges(keep: &[bool]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = None;
    for (index, keep_byte) in keep.iter().copied().enumerate() {
        match (start, keep_byte) {
            (None, true) => start = Some(index),
            (Some(run_start), false) => {
                ranges.push(run_start..index);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(run_start) = start {
        ranges.push(run_start..keep.len());
    }
    ranges
}

fn section_label(range: &Range<usize>, info: &RmetaInfo) -> String {
    if range.end <= 8 {
        return "metadata_header".to_owned();
    }
    if range.start < 16 && range.end > 8 {
        return "crate_root_pointer".to_owned();
    }
    let Some(root) = info.crate_root_pos else {
        return "unknown_no_root_pointer".to_owned();
    };
    if range.end <= root {
        "lazy_metadata_payload".to_owned()
    } else if range.start >= root {
        "crate_root_and_table_directory".to_owned()
    } else {
        "crosses_lazy_payload_and_crate_root".to_owned()
    }
}

fn format_diff_summary(diff: &DiffSummary, base_info: &RmetaInfo) -> String {
    if diff.same {
        return "identical".to_owned();
    }
    let mut out = String::new();
    out.push_str(&format!(
        "len {} -> {}; prefix {}; suffix {}; lhs-window {}..{}; rhs-window {}..{}",
        diff.lhs_len,
        diff.rhs_len,
        diff.common_prefix,
        diff.common_suffix,
        diff.lhs_window.start,
        diff.lhs_window.end,
        diff.rhs_window.start,
        diff.rhs_window.end
    ));
    let mut labels = BTreeMap::<String, usize>::new();
    for range in &diff.equal_offset_ranges {
        *labels.entry(section_label(range, base_info)).or_default() += 1;
    }
    if !labels.is_empty() {
        out.push_str("; equal-offset-regions ");
        let parts = labels
            .into_iter()
            .map(|(label, count)| format!("{label}:{count}"))
            .collect::<Vec<_>>();
        out.push_str(&parts.join(","));
    }
    out
}

fn format_ranges(ranges: &[Range<usize>]) -> String {
    if ranges.is_empty() {
        return "-".to_owned();
    }
    ranges
        .iter()
        .take(20)
        .map(|range| format!("{}..{}", range.start, range.end))
        .collect::<Vec<_>>()
        .join(",")
}

fn expectation_name(expectation: SemanticExpectation) -> &'static str {
    match expectation {
        SemanticExpectation::Stable => "stable",
        SemanticExpectation::Changes => "changes",
    }
}

fn write_table(path: &Path, header: &str, rows: &[String]) -> Result<(), String> {
    let mut file = fs::File::create(path)
        .map_err(|err| format!("failed to create {}: {err}", path.display()))?;
    file.write_all(header.as_bytes())
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    for row in rows {
        file.write_all(row.as_bytes())
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        file.write_all(b"\n")
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }
    Ok(())
}

fn write_summary(
    path: &Path,
    artifacts: &BTreeMap<&'static str, Vec<Artifact>>,
) -> Result<(), String> {
    let mut out = String::new();
    out.push_str("# rmeta hash spike summary\n\n");
    for (plan, plan_artifacts) in artifacts {
        out.push_str(&format!("## {plan}\n\n"));
        if let Some(base) = plan_artifacts
            .iter()
            .find(|artifact| artifact.variant.name == "base")
        {
            out.push_str(&format!(
                "- base len: {}\n- metadata version byte: {:?}\n- crate root position: {:?}\n- base artifact: {}\n",
                base.info.len,
                base.info.metadata_version,
                base.info.crate_root_pos,
                base.rmeta_path.display()
            ));
            if !base.info.ascii.is_empty() {
                out.push_str("- visible ASCII runs:\n");
                for run in base.info.ascii.iter().take(12) {
                    out.push_str(&format!(
                        "  - {}: `{}`\n",
                        run.offset,
                        run.text.replace('`', "\\`")
                    ));
                }
            }
        }
        out.push('\n');
    }
    fs::write(path, out).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn recreate_dir(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_dir_all(path)
            .map_err(|err| format!("failed to remove {}: {err}", path.display()))?;
    }
    fs::create_dir_all(path).map_err(|err| format!("failed to create {}: {err}", path.display()))
}

struct Fnv1a {
    state: u64,
}

impl Fnv1a {
    fn new() -> Self {
        Self {
            state: 0xcbf29ce484222325,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(0x100000001b3);
        }
    }

    fn finish(self) -> u64 {
        self.state
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hasher = Fnv1a::new();
    hasher.update(bytes);
    hasher.finish()
}

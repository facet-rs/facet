use object::read::archive::ArchiveFile;
use object::{
    Object, ObjectSection, ObjectSymbol, RelocationTarget, SectionIndex, SectionKind, SymbolKind,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const TESTS: &[&str] = &[
    "local_arithmetic_is_isolated",
    "local_string_is_isolated",
    "local_table_is_isolated",
    "hash_direct_uses_lib_a",
    "hash_pipeline_uses_lib_a",
    "generic_instantiation_uses_lib_a",
];

#[derive(Clone, Copy, Debug)]
enum EditKind {
    Baseline,
    HashBody,
    CommentOnly,
    LineCommentOnly,
    GenericBody,
}

#[derive(Clone, Debug)]
struct Scenario {
    name: &'static str,
    edit: EditKind,
    debuginfo: u8,
}

#[derive(Clone, Debug)]
struct Atom {
    id: String,
    object_label: String,
    section_name: String,
    segment_name: String,
    section_kind: SectionKind,
    aliases: Vec<String>,
    display_name: String,
    range_start: u64,
    range_end: u64,
    bytes: Vec<u8>,
    relocations: Vec<RelocRef>,
    hash: String,
}

#[derive(Clone, Debug)]
struct RelocRef {
    offset: u64,
    width: usize,
    target_name: String,
    target_display: String,
    kind: String,
    size_bits: u8,
    addend: i64,
}

#[derive(Clone, Debug)]
struct TestReach {
    root_atom: String,
    reachable_atoms: BTreeSet<String>,
    reachable_hashes: BTreeSet<String>,
    registrar_atoms: Vec<String>,
}

#[derive(Clone, Debug)]
struct Analysis {
    scenario: String,
    debuginfo: u8,
    objects: usize,
    archive_objects: usize,
    loose_objects: usize,
    duplicate_objects_skipped: usize,
    physical_sections: usize,
    atoms: BTreeMap<String, Atom>,
    defs_by_name: BTreeMap<String, BTreeSet<String>>,
    graph: BTreeMap<String, BTreeSet<String>>,
    unresolved_edges: Vec<String>,
    tests: BTreeMap<String, TestReach>,
    debug_atom_hashes: BTreeSet<String>,
    loadable_atom_hashes: BTreeSet<String>,
    physical_text_sections: usize,
    text_atoms: usize,
}

#[derive(Default)]
struct AnalysisBuilder {
    atoms: BTreeMap<String, Atom>,
    defs_by_name: BTreeMap<String, BTreeSet<String>>,
    section_aliases: BTreeMap<String, String>,
    physical_sections: usize,
    physical_text_sections: usize,
    text_atoms: usize,
}

#[derive(Clone)]
struct SectionInfo {
    ordinal: usize,
    name: String,
    segment_name: String,
    kind: SectionKind,
    address: u64,
    data: Vec<u8>,
}

#[derive(Clone)]
struct SymbolCandidate {
    raw_name: String,
    display_name: String,
    offset: u64,
    is_global: bool,
    kind: SymbolKind,
}

#[derive(Clone)]
struct AtomRange {
    id: String,
    start: u64,
    end: u64,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "reloc_walk=info".into()),
        )
        .with_target(false)
        .init();

    let root = env::current_dir()?;
    let spike_root = root.join("spikes/reloc-walk");
    let scenarios = [
        Scenario {
            name: "baseline-nodebug",
            edit: EditKind::Baseline,
            debuginfo: 0,
        },
        Scenario {
            name: "baseline-debug",
            edit: EditKind::Baseline,
            debuginfo: 2,
        },
        Scenario {
            name: "hash-body-nodebug",
            edit: EditKind::HashBody,
            debuginfo: 0,
        },
        Scenario {
            name: "comment-nodebug",
            edit: EditKind::CommentOnly,
            debuginfo: 0,
        },
        Scenario {
            name: "comment-debug",
            edit: EditKind::CommentOnly,
            debuginfo: 2,
        },
        Scenario {
            name: "line-comment-nodebug",
            edit: EditKind::LineCommentOnly,
            debuginfo: 0,
        },
        Scenario {
            name: "line-comment-debug",
            edit: EditKind::LineCommentOnly,
            debuginfo: 2,
        },
        Scenario {
            name: "generic-body-nodebug",
            edit: EditKind::GenericBody,
            debuginfo: 0,
        },
    ];

    let mut analyses = BTreeMap::new();
    for scenario in scenarios {
        let analysis = build_and_analyze(&spike_root, &scenario)?;
        info!(
            scenario = analysis.scenario,
            objects = analysis.objects,
            atoms = analysis.atoms.len(),
            unresolved_edges = analysis.unresolved_edges.len(),
            "analyzed scenario"
        );
        analyses.insert(scenario.name.to_string(), analysis);
    }

    assert_matrix(&analyses)?;
    write_observed_report(&spike_root, &analyses)?;
    Ok(())
}

fn build_and_analyze(spike_root: &Path, scenario: &Scenario) -> Result<Analysis> {
    let fixture_src = spike_root.join("fixture");
    let scenario_dir = spike_root.join("target/scenarios").join(scenario.name);
    if scenario_dir.exists() {
        fs::remove_dir_all(&scenario_dir)?;
    }
    copy_dir(&fixture_src, &scenario_dir)?;
    apply_scenario_edit(&scenario_dir, scenario.edit)?;
    run_fixture_build(&scenario_dir, scenario)?;
    analyze_target_dir(&scenario_dir, scenario)
}

fn apply_scenario_edit(scenario_dir: &Path, edit: EditKind) -> Result<()> {
    let lib_path = scenario_dir.join("lib_a/src/lib.rs");
    let mut source = fs::read_to_string(&lib_path)?;
    match edit {
        EditKind::Baseline => {}
        EditKind::HashBody => {
            let old = "let rotated = input.rotate_left(7);\n    rotated ^ 0x9e37_79b9_7f4a_7c15";
            let new = "let rotated = input.rotate_left(13);\n    rotated.wrapping_add(0xa076_1d64_78bd_642f)";
            source = source.replace(old, new);
        }
        EditKind::CommentOnly => {
            source = source.replace(
                "let rotated = input.rotate_left(7);",
                "let rotated = input.rotate_left(7); // COMMENT_ONLY_EDIT: same-line comment",
            );
        }
        EditKind::LineCommentOnly => {
            source = source.replace(
                "#![allow(clippy::identity_op)]\n",
                "#![allow(clippy::identity_op)]\n// LINE_COMMENT_ONLY_EDIT: shifts source lines without changing tokens.\n",
            );
        }
        EditKind::GenericBody => {
            let old = "let value = input.into();\n    value.wrapping_mul(41).rotate_left(3) ^ 0xfeed_face_cafe_babe";
            let new = "let value = input.into();\n    value.wrapping_mul(97).rotate_left(5) ^ 0x0123_4567_89ab_cdef";
            source = source.replace(old, new);
        }
    }
    fs::write(lib_path, source)?;
    Ok(())
}

fn run_fixture_build(scenario_dir: &Path, scenario: &Scenario) -> Result<()> {
    let target_dir = scenario_dir.join("target");
    let rustflags = format!(
        "-C debuginfo={} -C split-debuginfo=off -C save-temps=yes -C codegen-units=8 -C link-dead-code=no -C symbol-mangling-version=v0",
        scenario.debuginfo
    );
    let output = Command::new("cargo")
        .arg("test")
        .arg("--no-run")
        .arg("-p")
        .arg("test_crate")
        .arg("--test")
        .arg("selection")
        .env("CARGO_TARGET_DIR", &target_dir)
        .env("CARGO_INCREMENTAL", "0")
        .env("RUSTFLAGS", rustflags)
        .current_dir(scenario_dir)
        .output()?;

    if !output.status.success() {
        return Err(format!(
            "fixture build failed for {}:\nstdout:\n{}\nstderr:\n{}",
            scenario.name,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

fn analyze_target_dir(scenario_dir: &Path, scenario: &Scenario) -> Result<Analysis> {
    let deps_dir = scenario_dir.join("target/debug/deps");
    let mut object_inputs = Vec::new();
    collect_files_with_ext(&deps_dir, OsStr::new("rlib"), &mut object_inputs)?;
    collect_files_with_ext(&deps_dir, OsStr::new("o"), &mut object_inputs)?;

    let mut seen_hashes = HashSet::new();
    let mut builder = AnalysisBuilder::default();
    let mut objects = 0;
    let mut archive_objects = 0;
    let mut loose_objects = 0;
    let mut duplicates = 0;

    object_inputs.sort();
    for path in object_inputs {
        let data = fs::read(&path)?;
        if path.extension() == Some(OsStr::new("rlib")) {
            let archive = ArchiveFile::parse(&*data)?;
            for member in archive.members() {
                let member = member?;
                let name = String::from_utf8_lossy(member.name()).to_string();
                if !name.ends_with(".o") {
                    continue;
                }
                let member_data = member.data(&*data)?;
                let object_hash = fnv_hex(member_data);
                if !seen_hashes.insert(object_hash) {
                    duplicates += 1;
                    continue;
                }
                let label = format!("{}({})", path_label(scenario_dir, &path), name);
                parse_object(&label, member_data, &mut builder)?;
                objects += 1;
                archive_objects += 1;
            }
        } else {
            let object_hash = fnv_hex(&data);
            if !seen_hashes.insert(object_hash) {
                duplicates += 1;
                continue;
            }
            let label = path_label(scenario_dir, &path);
            parse_object(&label, &data, &mut builder)?;
            objects += 1;
            loose_objects += 1;
        }
    }

    compute_atom_hashes(&mut builder);
    let (graph, unresolved_edges) = build_graph(&builder);
    let tests = identify_tests(&builder, &graph)?;
    let debug_atom_hashes = builder
        .atoms
        .values()
        .filter(|atom| atom.section_kind == SectionKind::Debug)
        .map(|atom| atom.hash.clone())
        .collect();
    let loadable_atom_hashes = builder
        .atoms
        .values()
        .filter(|atom| is_loadable_kind(atom.section_kind))
        .map(|atom| atom.hash.clone())
        .collect();

    Ok(Analysis {
        scenario: scenario.name.to_string(),
        debuginfo: scenario.debuginfo,
        objects,
        archive_objects,
        loose_objects,
        duplicate_objects_skipped: duplicates,
        physical_sections: builder.physical_sections,
        atoms: builder.atoms,
        defs_by_name: builder.defs_by_name,
        graph,
        unresolved_edges,
        tests,
        debug_atom_hashes,
        loadable_atom_hashes,
        physical_text_sections: builder.physical_text_sections,
        text_atoms: builder.text_atoms,
    })
}

fn parse_object(label: &str, data: &[u8], builder: &mut AnalysisBuilder) -> Result<()> {
    let file = match object::File::parse(data) {
        Ok(file) => file,
        Err(err) => {
            warn!(label, error = %err, "skipping non-object archive member");
            return Ok(());
        }
    };

    let mut sections = Vec::new();
    let mut section_ord_by_index = HashMap::new();
    for (ordinal, section) in file.sections().enumerate() {
        let index = section.index();
        section_ord_by_index.insert(index, ordinal);
        let name = section
            .name()
            .unwrap_or("<invalid-section-name>")
            .to_string();
        let segment_name = section
            .segment_name()
            .ok()
            .flatten()
            .unwrap_or("<no-segment>")
            .to_string();
        let kind = section.kind();
        if kind == SectionKind::Text {
            builder.physical_text_sections += 1;
        }
        builder.physical_sections += 1;
        sections.push(SectionInfo {
            ordinal,
            name,
            segment_name,
            kind,
            address: section.address(),
            data: section.data().unwrap_or(&[]).to_vec(),
        });
    }

    let mut symbols_by_section: BTreeMap<usize, Vec<SymbolCandidate>> = BTreeMap::new();
    for symbol in file.symbols() {
        if symbol.is_undefined() {
            continue;
        }
        if matches!(symbol.kind(), SymbolKind::File | SymbolKind::Section) {
            continue;
        }
        let Some(section_index) = symbol.section_index() else {
            continue;
        };
        let Some(&section_ordinal) = section_ord_by_index.get(&section_index) else {
            continue;
        };
        let section = &sections[section_ordinal];
        let address = symbol.address();
        if address < section.address {
            continue;
        }
        let offset = address - section.address;
        if offset > section.data.len() as u64 {
            continue;
        }
        let raw_name = symbol.name().unwrap_or("<invalid-symbol-name>").to_string();
        if raw_name.is_empty() {
            continue;
        }
        symbols_by_section
            .entry(section_ordinal)
            .or_default()
            .push(SymbolCandidate {
                display_name: demangle_symbol(&raw_name),
                raw_name,
                offset,
                is_global: symbol.is_global(),
                kind: symbol.kind(),
            });
    }

    let mut atom_ranges_by_section: BTreeMap<usize, Vec<AtomRange>> = BTreeMap::new();
    for section in &sections {
        let mut symbols = symbols_by_section
            .remove(&section.ordinal)
            .unwrap_or_default();
        symbols.sort_by(|a, b| {
            a.offset
                .cmp(&b.offset)
                .then_with(|| b.is_global.cmp(&a.is_global))
                .then_with(|| a.raw_name.cmp(&b.raw_name))
        });
        let mut starts: Vec<u64> = symbols.iter().map(|symbol| symbol.offset).collect();
        starts.sort_unstable();
        starts.dedup();

        let mut ranges = Vec::new();
        if starts.is_empty() {
            if should_keep_symbolless_section(section.kind, &section.data) {
                let id = format!("{}::section{}@0", label, section.ordinal);
                let alias = format!("{}::__section_{}", label, section.ordinal);
                let atom = Atom {
                    id: id.clone(),
                    object_label: label.to_string(),
                    section_name: section.name.clone(),
                    segment_name: section.segment_name.clone(),
                    section_kind: section.kind,
                    aliases: vec![alias.clone()],
                    display_name: alias.clone(),
                    range_start: 0,
                    range_end: section.data.len() as u64,
                    bytes: section.data.clone(),
                    relocations: Vec::new(),
                    hash: String::new(),
                };
                builder
                    .defs_by_name
                    .entry(alias)
                    .or_default()
                    .insert(id.clone());
                builder.atoms.insert(id.clone(), atom);
                ranges.push(AtomRange {
                    id,
                    start: 0,
                    end: section.data.len() as u64,
                });
            }
            atom_ranges_by_section.insert(section.ordinal, ranges);
            continue;
        }

        if starts[0] > 0 && should_keep_symbolless_section(section.kind, &section.data) {
            let id = format!("{}::section{}@0", label, section.ordinal);
            let alias = format!("{}::__section_{}_prefix", label, section.ordinal);
            let end = starts[0];
            let atom = Atom {
                id: id.clone(),
                object_label: label.to_string(),
                section_name: section.name.clone(),
                segment_name: section.segment_name.clone(),
                section_kind: section.kind,
                aliases: vec![alias.clone()],
                display_name: alias.clone(),
                range_start: 0,
                range_end: end,
                bytes: section.data[..end as usize].to_vec(),
                relocations: Vec::new(),
                hash: String::new(),
            };
            builder
                .defs_by_name
                .entry(alias)
                .or_default()
                .insert(id.clone());
            builder.atoms.insert(id.clone(), atom);
            ranges.push(AtomRange { id, start: 0, end });
        }

        for (start_index, start) in starts.iter().enumerate() {
            let end = starts
                .get(start_index + 1)
                .copied()
                .unwrap_or(section.data.len() as u64);
            if end <= *start {
                continue;
            }
            let aliases: Vec<SymbolCandidate> = symbols
                .iter()
                .filter(|symbol| symbol.offset == *start)
                .cloned()
                .collect();
            let primary = choose_primary_alias(&aliases);
            let id = format!(
                "{}::section{}@{}:{}",
                label, section.ordinal, start, primary.raw_name
            );
            let atom_aliases: Vec<String> = aliases
                .iter()
                .map(|symbol| symbol.raw_name.clone())
                .collect();
            let atom = Atom {
                id: id.clone(),
                object_label: label.to_string(),
                section_name: section.name.clone(),
                segment_name: section.segment_name.clone(),
                section_kind: section.kind,
                aliases: atom_aliases.clone(),
                display_name: primary.display_name.clone(),
                range_start: *start,
                range_end: end,
                bytes: section.data[*start as usize..end as usize].to_vec(),
                relocations: Vec::new(),
                hash: String::new(),
            };
            if section.kind == SectionKind::Text {
                builder.text_atoms += 1;
            }
            for alias in atom_aliases {
                builder
                    .defs_by_name
                    .entry(alias)
                    .or_default()
                    .insert(id.clone());
            }
            builder.atoms.insert(id.clone(), atom);
            ranges.push(AtomRange {
                id,
                start: *start,
                end,
            });
        }
        atom_ranges_by_section.insert(section.ordinal, ranges);
    }

    for section in &sections {
        if let Some(first_atom) = atom_ranges_by_section
            .get(&section.ordinal)
            .and_then(|ranges| ranges.first())
        {
            let alias = section_alias(label, section.ordinal);
            builder
                .defs_by_name
                .entry(alias.clone())
                .or_default()
                .insert(first_atom.id.clone());
            builder.section_aliases.insert(alias, first_atom.id.clone());
        }
    }

    for section in file.sections() {
        let Some(&section_ordinal) = section_ord_by_index.get(&section.index()) else {
            continue;
        };
        let Some(ranges) = atom_ranges_by_section.get(&section_ordinal) else {
            continue;
        };
        for (offset, relocation) in section.relocations() {
            let Some(source_atom) = ranges.iter().find(|range| {
                (offset >= range.start && offset < range.end)
                    || (offset == range.start && range.start == range.end)
            }) else {
                continue;
            };
            let (target_name, target_display) = relocation_target_name(
                &file,
                label,
                &sections,
                &section_ord_by_index,
                relocation.target(),
            );
            let width = relocation_width(relocation.size());
            let reloc_ref = RelocRef {
                offset,
                width,
                target_display,
                target_name,
                kind: format!("{:?}", relocation.kind()),
                size_bits: relocation.size(),
                addend: relocation.addend(),
            };
            if let Some(atom) = builder.atoms.get_mut(&source_atom.id) {
                atom.relocations.push(reloc_ref);
            }
        }
    }

    Ok(())
}

fn choose_primary_alias(aliases: &[SymbolCandidate]) -> SymbolCandidate {
    aliases
        .iter()
        .min_by_key(|symbol| {
            let temp = symbol.raw_name.contains("Ltmp")
                || symbol.raw_name.contains("ltmp")
                || symbol.raw_name.contains("GCC_except_table");
            let kind_rank = match symbol.kind {
                SymbolKind::Text => 0,
                SymbolKind::Data => 1,
                SymbolKind::Label => 2,
                SymbolKind::Unknown => 3,
                _ => 4,
            };
            (
                temp,
                !symbol.is_global,
                kind_rank,
                symbol.display_name.len(),
                symbol.raw_name.clone(),
            )
        })
        .expect("atom aliases are non-empty")
        .clone()
}

fn relocation_target_name<'data>(
    file: &object::File<'data>,
    label: &str,
    sections: &[SectionInfo],
    section_ord_by_index: &HashMap<SectionIndex, usize>,
    target: RelocationTarget,
) -> (String, String) {
    match target {
        RelocationTarget::Symbol(index) => match file.symbol_by_index(index) {
            Ok(symbol) => {
                let name = symbol
                    .name()
                    .unwrap_or("<invalid-relocation-symbol>")
                    .to_string();
                let display = demangle_symbol(&name);
                (name, display)
            }
            Err(_) => (
                "<bad-relocation-symbol-index>".to_string(),
                "<bad-relocation-symbol-index>".to_string(),
            ),
        },
        RelocationTarget::Section(index) => {
            if let Some(&ordinal) = section_ord_by_index.get(&index) {
                let name = section_alias(label, ordinal);
                let display = sections
                    .get(ordinal)
                    .map(|section| format!("{}:{}", section.segment_name, section.name))
                    .unwrap_or_else(|| name.clone());
                (name, display)
            } else {
                (
                    "<bad-relocation-section-index>".to_string(),
                    "<bad-relocation-section-index>".to_string(),
                )
            }
        }
        RelocationTarget::Absolute => ("<absolute>".to_string(), "<absolute>".to_string()),
        _ => (
            "<unknown-relocation-target>".to_string(),
            "<unknown-relocation-target>".to_string(),
        ),
    }
}

fn compute_atom_hashes(builder: &mut AnalysisBuilder) {
    for atom in builder.atoms.values_mut() {
        atom.relocations.sort_by(|a, b| {
            a.offset
                .cmp(&b.offset)
                .then_with(|| a.target_name.cmp(&b.target_name))
        });
        let mut normalized = atom.bytes.clone();
        for relocation in &atom.relocations {
            if relocation.offset < atom.range_start {
                continue;
            }
            let local_offset = (relocation.offset - atom.range_start) as usize;
            let end = normalized.len().min(local_offset + relocation.width);
            if local_offset < end {
                for byte in &mut normalized[local_offset..end] {
                    *byte = 0;
                }
            }
        }

        let mut hash = Fnv64::new();
        hash.write(b"reloc-walk-atom-v1");
        hash.write(atom.section_name.as_bytes());
        hash.write(format!("{:?}", atom.section_kind).as_bytes());
        hash.write_u64(normalized.len() as u64);
        hash.write(&normalized);
        for relocation in &atom.relocations {
            hash.write_u64(relocation.offset - atom.range_start);
            hash.write(relocation.target_name.as_bytes());
        }
        atom.hash = hash.finish_hex();
    }
}

fn build_graph(builder: &AnalysisBuilder) -> (BTreeMap<String, BTreeSet<String>>, Vec<String>) {
    let mut graph = BTreeMap::new();
    let mut unresolved = Vec::new();
    for atom in builder.atoms.values() {
        let mut targets = BTreeSet::new();
        for relocation in &atom.relocations {
            if relocation.target_name == "<absolute>" {
                continue;
            }
            if let Some(defs) = builder.defs_by_name.get(&relocation.target_name) {
                targets.extend(defs.iter().cloned());
            } else {
                unresolved.push(format!(
                    "{} -> {} ({})",
                    atom.display_name, relocation.target_display, relocation.kind
                ));
            }
        }
        graph.insert(atom.id.clone(), targets);
    }
    (graph, unresolved)
}

fn identify_tests(
    builder: &AnalysisBuilder,
    graph: &BTreeMap<String, BTreeSet<String>>,
) -> Result<BTreeMap<String, TestReach>> {
    let mut incoming = BTreeMap::<String, Vec<String>>::new();
    for (source, targets) in graph {
        for target in targets {
            incoming
                .entry(target.clone())
                .or_default()
                .push(source.clone());
        }
    }

    let mut tests = BTreeMap::new();
    for test in TESTS {
        let root = find_test_root(builder, test)?;
        let reachable_atoms = reachable_from(graph, &root);
        let reachable_hashes = reachable_atoms
            .iter()
            .filter_map(|id| builder.atoms.get(id))
            .filter(|atom| is_loadable_kind(atom.section_kind))
            .map(|atom| atom.hash.clone())
            .collect();
        let registrar_atoms = incoming
            .get(&root)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|id| {
                builder
                    .atoms
                    .get(id)
                    .map(|atom| atom.section_kind != SectionKind::Text)
                    .unwrap_or(false)
            })
            .collect();
        tests.insert(
            (*test).to_string(),
            TestReach {
                root_atom: root,
                reachable_atoms,
                reachable_hashes,
                registrar_atoms,
            },
        );
    }
    Ok(tests)
}

fn find_test_root(builder: &AnalysisBuilder, test: &str) -> Result<String> {
    let needle = format!("selection::{test}");
    let mut candidates: Vec<&Atom> = builder
        .atoms
        .values()
        .filter(|atom| {
            atom.section_kind == SectionKind::Text
                && atom
                    .aliases
                    .iter()
                    .chain(std::iter::once(&atom.display_name))
                    .any(|name| demangle_symbol(name).contains(&needle) || name.contains(test))
        })
        .collect();
    candidates.sort_by_key(|atom| {
        (
            atom.display_name.contains("{{closure}}"),
            atom.display_name.len(),
            atom.id.clone(),
        )
    });
    candidates
        .first()
        .map(|atom| atom.id.clone())
        .ok_or_else(|| format!("could not identify test root for {test}").into())
}

fn reachable_from(graph: &BTreeMap<String, BTreeSet<String>>, root: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![root.to_string()];
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(targets) = graph.get(&id) {
            for target in targets {
                stack.push(target.clone());
            }
        }
    }
    seen
}

fn assert_matrix(analyses: &BTreeMap<String, Analysis>) -> Result<()> {
    let baseline = analyses
        .get("baseline-nodebug")
        .ok_or("missing baseline-nodebug analysis")?;
    let expectations = [
        (
            "hash-body-nodebug",
            [
                ("local_arithmetic_is_isolated", false),
                ("local_string_is_isolated", false),
                ("local_table_is_isolated", false),
                ("hash_direct_uses_lib_a", true),
                ("hash_pipeline_uses_lib_a", true),
                ("generic_instantiation_uses_lib_a", false),
            ],
        ),
        (
            "comment-nodebug",
            [
                ("local_arithmetic_is_isolated", false),
                ("local_string_is_isolated", false),
                ("local_table_is_isolated", false),
                ("hash_direct_uses_lib_a", false),
                ("hash_pipeline_uses_lib_a", false),
                ("generic_instantiation_uses_lib_a", false),
            ],
        ),
        (
            "generic-body-nodebug",
            [
                ("local_arithmetic_is_isolated", false),
                ("local_string_is_isolated", false),
                ("local_table_is_isolated", false),
                ("hash_direct_uses_lib_a", false),
                ("hash_pipeline_uses_lib_a", false),
                ("generic_instantiation_uses_lib_a", true),
            ],
        ),
    ];

    let mut failures = Vec::new();
    for (scenario_name, rows) in expectations {
        let scenario = analyses
            .get(scenario_name)
            .ok_or_else(|| format!("missing {scenario_name} analysis"))?;
        for (test, expected) in rows {
            let actual = test_invalidated(baseline, scenario, test)?;
            if actual != expected {
                failures.push(format!(
                    "{scenario_name}::{test}: expected invalidated={expected}, got {actual}"
                ));
            }
        }
    }

    let comment_debug = analyses
        .get("comment-debug")
        .ok_or("missing comment-debug analysis")?;
    let baseline_debug = analyses
        .get("baseline-debug")
        .ok_or("missing baseline-debug analysis")?;
    for test in TESTS {
        let actual = test_invalidated(baseline_debug, comment_debug, test)?;
        if actual {
            failures.push(format!(
                "comment-debug::{test}: loadable reachable hash set changed"
            ));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n").into())
    }
}

fn test_invalidated(baseline: &Analysis, scenario: &Analysis, test: &str) -> Result<bool> {
    let baseline_test = baseline
        .tests
        .get(test)
        .ok_or_else(|| format!("baseline missing test {test}"))?;
    let scenario_test = scenario
        .tests
        .get(test)
        .ok_or_else(|| format!("scenario {} missing test {test}", scenario.scenario))?;
    Ok(baseline_test.reachable_hashes != scenario_test.reachable_hashes)
}

fn write_observed_report(spike_root: &Path, analyses: &BTreeMap<String, Analysis>) -> Result<()> {
    let baseline = analyses
        .get("baseline-nodebug")
        .ok_or("missing baseline-nodebug analysis")?;
    let mut out = String::new();
    out.push_str("# Relocation Walk Observed Results\n\n");
    out.push_str("Generated by `cargo run --manifest-path spikes/reloc-walk/Cargo.toml`.\n\n");
    out.push_str("## Build Inputs\n\n");
    for analysis in analyses.values() {
        let relocation_count: usize = analysis
            .atoms
            .values()
            .map(|atom| atom.relocations.len())
            .sum();
        let relocations_with_addends: usize = analysis
            .atoms
            .values()
            .flat_map(|atom| atom.relocations.iter())
            .filter(|relocation| relocation.addend != 0)
            .count();
        let relocation_sizes: BTreeSet<u8> = analysis
            .atoms
            .values()
            .flat_map(|atom| atom.relocations.iter())
            .map(|relocation| relocation.size_bits)
            .collect();
        out.push_str(&format!(
            "- `{}`: debuginfo={}, objects={} (archive={}, loose={}, duplicate-skipped={}), physical-sections={}, physical-text-sections={}, text-atoms={}, atoms={}, defs={}, graph-nodes={}, relocations={}, relocation-sizes={:?}, relocations-with-addends={}\n",
            analysis.scenario,
            analysis.debuginfo,
            analysis.objects,
            analysis.archive_objects,
            analysis.loose_objects,
            analysis.duplicate_objects_skipped,
            analysis.physical_sections,
            analysis.physical_text_sections,
            analysis.text_atoms,
            analysis.atoms.len(),
            analysis.defs_by_name.len(),
            analysis.graph.len(),
            relocation_count,
            relocation_sizes,
            relocations_with_addends
        ));
    }
    out.push_str("\n## Test Roots\n\n");
    for test in TESTS {
        let reach = baseline
            .tests
            .get(*test)
            .ok_or_else(|| format!("missing baseline test {test}"))?;
        let atom = baseline
            .atoms
            .get(&reach.root_atom)
            .ok_or_else(|| format!("missing root atom {}", reach.root_atom))?;
        out.push_str(&format!(
            "- `{}` -> `{}` in `{}` `{}` bytes {}..{}; reachable atoms={}; reachable loadable hashes={}; registrar refs={}\n",
            test,
            atom.display_name,
            atom.object_label,
            atom.segment_name,
            atom.range_start,
            atom.range_end,
            reach.reachable_atoms.len(),
            reach.reachable_hashes.len(),
            reach.registrar_atoms.len()
        ));
    }

    out.push_str("\n## Invalidation Matrix\n\n");
    out.push_str("| scenario | test | invalidated | reachable loadable hashes |\n");
    out.push_str("| --- | --- | --- | ---: |\n");
    for scenario in analyses
        .values()
        .filter(|analysis| !analysis.scenario.starts_with("baseline-"))
    {
        let comparison_baseline = baseline_for_scenario(analyses, scenario)?;
        for test in TESTS {
            let invalidated = test_invalidated(comparison_baseline, scenario, test)?;
            let count = scenario
                .tests
                .get(*test)
                .map(|reach| reach.reachable_hashes.len())
                .unwrap_or_default();
            out.push_str(&format!(
                "| `{}` | `{}` | {} | {} |\n",
                scenario.scenario, test, invalidated, count
            ));
        }
    }

    out.push_str("\n## Changed Loadable Hash Inventory\n\n");
    for scenario in analyses
        .values()
        .filter(|analysis| !analysis.scenario.starts_with("baseline-"))
    {
        let comparison_baseline = baseline_for_scenario(analyses, scenario)?;
        let removed = comparison_baseline
            .loadable_atom_hashes
            .difference(&scenario.loadable_atom_hashes)
            .count();
        let added = scenario
            .loadable_atom_hashes
            .difference(&comparison_baseline.loadable_atom_hashes)
            .count();
        let debug_removed = comparison_baseline
            .debug_atom_hashes
            .difference(&scenario.debug_atom_hashes)
            .count();
        let debug_added = scenario
            .debug_atom_hashes
            .difference(&comparison_baseline.debug_atom_hashes)
            .count();
        out.push_str(&format!(
            "- `{}` vs `{}`: loadable removed={}, loadable added={}, debug removed={}, debug added={}\n",
            scenario.scenario, comparison_baseline.scenario, removed, added, debug_removed, debug_added
        ));
    }

    out.push_str("\n## Notable Changed Atoms\n\n");
    for scenario in analyses
        .values()
        .filter(|analysis| !analysis.scenario.starts_with("baseline-"))
    {
        let comparison_baseline = baseline_for_scenario(analyses, scenario)?;
        out.push_str(&format!("### {}\n\n", scenario.scenario));
        let notable = notable_changed_atoms(comparison_baseline, scenario);
        if notable.is_empty() {
            out.push_str("- No same-display loadable atom hash changes.\n");
        } else {
            for line in notable {
                out.push_str(&format!("- {line}\n"));
            }
        }
        out.push('\n');
    }

    let report_path = spike_root.join("target/observed-results.md");
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(report_path, out)?;
    Ok(())
}

fn baseline_for_scenario<'a>(
    analyses: &'a BTreeMap<String, Analysis>,
    scenario: &Analysis,
) -> Result<&'a Analysis> {
    let baseline_name = if scenario.debuginfo == 0 {
        "baseline-nodebug"
    } else {
        "baseline-debug"
    };
    analyses
        .get(baseline_name)
        .ok_or_else(|| format!("missing {baseline_name} analysis").into())
}

fn notable_changed_atoms(baseline: &Analysis, scenario: &Analysis) -> Vec<String> {
    let mut base_by_display = BTreeMap::<String, Vec<&Atom>>::new();
    for atom in baseline
        .atoms
        .values()
        .filter(|atom| is_loadable_kind(atom.section_kind))
    {
        base_by_display
            .entry(atom.display_name.clone())
            .or_default()
            .push(atom);
    }
    let mut lines = Vec::new();
    for atom in scenario
        .atoms
        .values()
        .filter(|atom| is_loadable_kind(atom.section_kind))
    {
        if let Some(base_atoms) = base_by_display.get(&atom.display_name) {
            if base_atoms.iter().all(|base| base.hash != atom.hash) {
                lines.push(format!(
                    "`{}` in `{}` `{}/{}` `{}` -> `{}`",
                    atom.display_name,
                    atom.object_label,
                    atom.segment_name,
                    atom.section_name,
                    base_atoms
                        .first()
                        .map(|base| base.hash.as_str())
                        .unwrap_or("<missing>"),
                    atom.hash
                ));
            }
        }
    }
    lines.sort();
    lines.truncate(24);
    lines
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            if entry.file_name() == OsStr::new("target") {
                continue;
            }
            copy_dir(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn collect_files_with_ext(dir: &Path, ext: &OsStr, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_files_with_ext(&path, ext, out)?;
        } else if path.extension() == Some(ext) {
            out.push(path);
        }
    }
    Ok(())
}

fn path_label(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn demangle_symbol(name: &str) -> String {
    let candidates = [
        name,
        name.strip_prefix('_').unwrap_or(name),
        name.strip_prefix("__").unwrap_or(name),
    ];
    for candidate in candidates {
        let demangled = rustc_demangle::demangle(candidate).to_string();
        if demangled != candidate {
            return demangled;
        }
    }
    name.to_string()
}

fn should_keep_symbolless_section(kind: SectionKind, data: &[u8]) -> bool {
    !data.is_empty() && (is_loadable_kind(kind) || kind == SectionKind::Debug)
}

fn is_loadable_kind(kind: SectionKind) -> bool {
    matches!(
        kind,
        SectionKind::Text
            | SectionKind::Data
            | SectionKind::ReadOnlyData
            | SectionKind::ReadOnlyDataWithRel
            | SectionKind::ReadOnlyString
            | SectionKind::UninitializedData
            | SectionKind::Common
            | SectionKind::Tls
            | SectionKind::UninitializedTls
            | SectionKind::TlsVariables
    )
}

fn relocation_width(size_bits: u8) -> usize {
    if size_bits == 0 {
        8
    } else {
        usize::from(size_bits).div_ceil(8).max(1)
    }
}

fn section_alias(label: &str, ordinal: usize) -> String {
    format!("{label}::__section_{ordinal}")
}

fn fnv_hex(bytes: &[u8]) -> String {
    let mut hash = Fnv64::new();
    hash.write(bytes);
    hash.finish_hex()
}

struct Fnv64 {
    state: u64,
}

impl Fnv64 {
    fn new() -> Self {
        Self {
            state: 0xcbf2_9ce4_8422_2325,
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn finish_hex(&self) -> String {
        format!("{:016x}", self.state)
    }
}

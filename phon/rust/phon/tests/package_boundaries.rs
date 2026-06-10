use std::collections::BTreeSet;

const WORKSPACE: &str = include_str!("../../Cargo.toml");
const PHON_SCHEMA: &str = include_str!("../../phon-schema/Cargo.toml");
const PHON_IR: &str = include_str!("../../phon-ir/Cargo.toml");
const PHON_ENGINE: &str = include_str!("../../phon-engine/Cargo.toml");
const PHON_JIT: &str = include_str!("../../phon-jit/Cargo.toml");
const PHON: &str = include_str!("../Cargo.toml");

// r[verify crates.concern-separation]
#[test]
fn rust_workspace_keeps_contract_engine_jit_and_binding_packages_split() {
    let members = workspace_members(WORKSPACE);
    for member in ["phon-schema", "phon-ir", "phon-engine", "phon-jit", "phon"] {
        assert!(
            members.contains(member),
            "workspace members missing {member}: {members:?}"
        );
    }

    let schema = dependency_names(PHON_SCHEMA);
    assert!(
        schema.is_disjoint(&set([
            "phon-ir",
            "phon-engine",
            "phon-jit",
            "phon",
            "facet"
        ])),
        "contract package must not depend on engine, JIT, binding, or facet: {schema:?}"
    );

    let ir = dependency_names(PHON_IR);
    assert!(ir.contains("phon-schema"), "phon-ir deps: {ir:?}");
    assert!(
        ir.is_disjoint(&set(["phon-engine", "phon-jit", "phon", "facet"])),
        "IR package must not depend upward on engine, JIT, binding, or facet: {ir:?}"
    );

    let engine = dependency_names(PHON_ENGINE);
    for dep in ["phon-schema", "phon-ir"] {
        assert!(engine.contains(dep), "phon-engine deps: {engine:?}");
    }
    assert!(
        engine.is_disjoint(&set(["phon-jit", "phon", "facet"])),
        "engine package must not depend upward on JIT, binding, or facet: {engine:?}"
    );

    let jit = dependency_names(PHON_JIT);
    assert!(jit.contains("phon-ir"), "phon-jit deps: {jit:?}");
    assert!(
        jit.is_disjoint(&set(["phon-engine", "phon", "facet"])),
        "JIT package must not depend on the engine, binding, or facet: {jit:?}"
    );

    let binding = dependency_names(PHON);
    for dep in ["phon-schema", "phon-ir", "phon-engine", "facet"] {
        assert!(binding.contains(dep), "phon deps: {binding:?}");
    }
    assert!(
        binding.contains("phon-jit"),
        "front door must own the optional JIT edge: {binding:?}"
    );
}

// r[verify crates.engine-is-binding-free]
#[test]
fn rust_engine_ir_and_jit_do_not_depend_on_reflection_or_derive_crates() {
    for (name, manifest) in [
        ("phon-ir", PHON_IR),
        ("phon-engine", PHON_ENGINE),
        ("phon-jit", PHON_JIT),
    ] {
        let deps = dependency_names(manifest);
        let reflection_deps: Vec<_> = deps
            .iter()
            .copied()
            .filter(|dep| dep.starts_with("facet") && *dep != "facet-value")
            .collect();
        assert!(
            reflection_deps.is_empty(),
            "{name} must stay binding-free; found reflection deps {reflection_deps:?} in {deps:?}"
        );
    }
}

fn workspace_members(manifest: &'static str) -> BTreeSet<&'static str> {
    let mut members = BTreeSet::new();
    let mut in_members = false;
    for line in manifest.lines() {
        let trimmed = strip_comment(line).trim();
        if trimmed.starts_with("members") && trimmed.contains('[') {
            in_members = true;
            continue;
        }
        if in_members && trimmed.starts_with(']') {
            break;
        }
        if in_members && let Some(member) = quoted_value(trimmed) {
            members.insert(member);
        }
    }
    members
}

fn dependency_names(manifest: &'static str) -> BTreeSet<&'static str> {
    let mut deps = BTreeSet::new();
    let mut in_dependencies = false;
    for line in manifest.lines() {
        let trimmed = strip_comment(line).trim();
        if trimmed == "[dependencies]" {
            in_dependencies = true;
            continue;
        }
        if in_dependencies && trimmed.starts_with('[') {
            break;
        }
        if !in_dependencies || trimmed.is_empty() {
            continue;
        }
        if let Some((name, _)) = trimmed.split_once('=') {
            deps.insert(
                name.trim()
                    .strip_suffix(".workspace")
                    .unwrap_or(name.trim()),
            );
        }
    }
    deps
}

fn quoted_value(line: &'static str) -> Option<&'static str> {
    let start = line.find('"')? + 1;
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn strip_comment(line: &'static str) -> &'static str {
    line.split_once('#').map_or(line, |(before, _)| before)
}

fn set<const N: usize>(items: [&'static str; N]) -> BTreeSet<&'static str> {
    items.into_iter().collect()
}

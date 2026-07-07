use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use facet::Facet;
use vix::exec::Tree;
use vix::machine::{Machine, MachineArg, RenderedValue};

const SOURCE: &str =
    include_str!("../../playgrounds/snark/src/bundled/vix/samples/cargo_manifest.vix");
const RODIN_SOURCE: &str = include_str!("../../rodin/rodin.vix");

const WORKSPACE_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/cargo_manifest_real/Cargo.toml"
);
const TAXON_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/cargo_manifest_real/phon/rust/taxon/Cargo.toml"
);
const TAXON_LIB: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/cargo_manifest_real/phon/rust/taxon/src/lib.rs"
);
const FACET_CORE_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/cargo_manifest_real/facet-core/Cargo.toml"
);
const FACET_CORE_LIB: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/cargo_manifest_real/facet-core/src/lib.rs"
);
const FACET_CORE_BUILD: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/cargo_manifest_real/facet-core/build.rs"
);
const FACET_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/cargo_manifest_real/facet/Cargo.toml"
);
const FACET_LIB: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/cargo_manifest_real/facet/src/lib.rs"
);
const FACET_BUILD: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/cargo_manifest_real/facet/build.rs"
);

const REAL_MEMBERS: [&str; 3] = ["taxon", "facet-core", "facet"];

#[test]
fn workspace_members_and_package_identity_come_from_real_manifest_copies() -> Result<(), String> {
    let metadata = cargo_metadata_oracle()?;
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, workspace_tree())?;

    let members = machine.demand_i64("workspace_members_text", vec![workspace])?;
    assert_eq!(
        rendered_string(&machine, "workspace_members_text", members)?,
        "phon/rust/taxon\nfacet-core\nfacet"
    );

    for (name, manifest, path) in [
        ("taxon", taxon_tree(), "phon/rust/taxon"),
        ("facet-core", facet_core_tree(), "facet-core"),
        ("facet", facet_tree(), "facet"),
    ] {
        let member = intern_tree(&mut machine, manifest)?;
        let path = intern_string(&mut machine, path)?;
        let package = machine.demand_i64("package_of", vec![member, workspace, path])?;
        let package = record(machine.render_result("package_of", package)?)?;
        let cargo = metadata
            .packages
            .iter()
            .find(|package| package.name == name)
            .ok_or_else(|| format!("cargo metadata did not include {name}"))?;
        assert_eq!(field_string(&package, "name")?, cargo.name);
        assert_eq!(field_string(&package, "version")?, cargo.version);
        assert_eq!(field_string(&package, "edition")?, cargo.edition);
    }

    Ok(())
}

#[test]
fn member_manifest_paths_are_derived_from_granted_root() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let root = intern_path(&mut machine, "/workspace")?;
    let member = intern_string(&mut machine, "facet-core")?;

    let value = machine.demand_i64("member_manifest_path", vec![root, member])?;
    assert_eq!(
        rendered_string(&machine, "member_manifest_path", value)?,
        "/workspace/facet-core/Cargo.toml"
    );
    Ok(())
}

#[test]
fn projected_member_manifests_are_read_from_granted_root() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(
        &mut machine,
        Tree::of(&[
            (
                "Cargo.toml",
                r#"[workspace]
members = ["crates/a"]
"#,
            ),
            (
                "crates/a/Cargo.toml",
                r#"[package]
name = "a"
version = "0.1.0"
edition = "2024"
"#,
            ),
        ]),
    )?;
    let root = intern_path(&mut machine, "")?;
    let member = intern_string(&mut machine, "crates/a")?;

    let value = machine.demand_i64(
        "workspace_projected_member_package_name",
        vec![workspace, root, member],
    )?;
    assert_eq!(
        rendered_string(&machine, "workspace_projected_member_package_name", value)?,
        "a"
    );
    let target = intern_string(&mut machine, "x86_64-apple-darwin")?;
    let selected = machine.demand_i64(
        "workspace_member_only_solve_selected_member_count",
        vec![workspace, root, target],
    )?;
    assert_eq!(selected, 1);
    Ok(())
}

#[test]
fn tiny_workspace_solve_diff_is_categorized_against_real_cargo_lock() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(
        &mut machine,
        Tree::of(&[
            (
                "Cargo.toml",
                r#"[workspace]
members = ["bytes"]
"#,
            ),
            (
                "bytes/Cargo.toml",
                r#"[package]
name = "bytes"
version = "1.12.0"
edition = "2024"
"#,
            ),
        ]),
    )?;
    let root = intern_path(&mut machine, "")?;
    let target = intern_string(&mut machine, "x86_64-apple-darwin")?;
    let selected = machine.demand_i64(
        "workspace_member_only_solve_selected_versions_text",
        vec![workspace, root, target],
    )?;
    let selected = rendered_string(
        &machine,
        "workspace_member_only_solve_selected_versions_text",
        selected,
    )?;
    let solve_rows = package_versions_from_solve_text(&selected)?;
    let lock_rows = cargo_lock_package_rows(&workspace_root().join("Cargo.lock"))?;
    let metadata_rows = cargo_metadata_real_workspace()?.package_rows();
    let diff = diff_package_versions_against_lock(&solve_rows, &lock_rows, &metadata_rows);

    assert!(
        solve_rows.contains(&PackageVersion::new("bytes", "1.12.0")),
        "{solve_rows:#?}"
    );
    assert!(
        solve_rows.contains(&PackageVersion::new("__workspace__", "0.0.0")),
        "{solve_rows:#?}"
    );
    assert_eq!(diff.solve_rows, 2, "{diff:#?}");
    assert_eq!(diff.lock_rows, lock_rows.len(), "{diff:#?}");
    assert_eq!(diff.matches, 1, "{diff:#?}");
    assert_eq!(diff.solve_only, 1, "{diff:#?}");
    assert_eq!(
        diff.solve_only_categories.get("workspace-pseudo-root"),
        Some(&1),
        "{diff:#?}"
    );
    assert_eq!(diff.lock_only + diff.matches, diff.lock_rows, "{diff:#?}");
    assert_eq!(
        diff.lock_only_categories.get("cargo-selected-not-in-solve"),
        Some(&(metadata_rows.len() - diff.matches)),
        "{diff:#?}"
    );

    write_tier_a_artifact(
        "tiny-solve-vs-lock-summary.tsv",
        &package_diff_summary_table(&diff),
    )?;
    write_tier_a_artifact(
        "tiny-solve-vs-lock-solve-rows.tsv",
        &package_rows_table(&solve_rows),
    )?;
    Ok(())
}

#[test]
fn tiny_workspace_prerelease_member_solve_selects_member() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(
        &mut machine,
        Tree::of(&[
            (
                "Cargo.toml",
                r#"[workspace]
members = ["facet"]
"#,
            ),
            (
                "facet/Cargo.toml",
                r#"[package]
name = "facet"
version = "0.50.0-rc.5"
edition = "2024"
"#,
            ),
        ]),
    )?;
    let root = intern_path(&mut machine, "")?;
    let target = intern_string(&mut machine, "x86_64-apple-darwin")?;
    let exact_candidates = machine.demand_i64(
        "workspace_member_only_first_member_exact_candidate_count",
        vec![workspace, root],
    )?;
    assert_eq!(exact_candidates, 1);
    let selected = machine.demand_i64(
        "workspace_member_only_solve_selected_member_count",
        vec![workspace, root, target],
    )?;
    assert_eq!(selected, 1);

    let selected_versions = machine.demand_i64(
        "workspace_member_only_solve_selected_versions_text",
        vec![workspace, root, target],
    )?;
    let selected_versions = rendered_string(
        &machine,
        "workspace_member_only_solve_selected_versions_text",
        selected_versions,
    )?;
    let solve_rows = package_versions_from_solve_text(&selected_versions)?;
    assert!(
        solve_rows.contains(&PackageVersion::new("facet", "0.50.0-rc.5")),
        "{solve_rows:#?}"
    );
    Ok(())
}

#[test]
fn workspace_member_globs_expand_from_root_manifest() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(
        &mut machine,
        Tree::of(&[
            (
                "Cargo.toml",
                r#"[workspace]
members = ["crates/*", "plain"]
"#,
            ),
            ("crates/a/Cargo.toml", "[package]\nname = \"a\"\n"),
            ("crates/b/Cargo.toml", "[package]\nname = \"b\"\n"),
            ("plain/Cargo.toml", "[package]\nname = \"plain\"\n"),
        ]),
    )?;

    let members = machine.demand_i64("workspace_members_text", vec![workspace])?;
    assert_eq!(
        rendered_string(&machine, "workspace_members_text", members)?,
        "crates/a\ncrates/b\nplain"
    );
    Ok(())
}

#[test]
fn dependency_declarations_extract_workspace_and_detailed_forms() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, workspace_tree())?;
    let taxon = intern_tree(&mut machine, taxon_tree())?;
    let facet_core = intern_tree(&mut machine, facet_core_tree())?;
    let facet = intern_tree(&mut machine, facet_tree())?;

    let dependency_names_kind = intern_string(&mut machine, "normal")?;
    let dependency_names = machine.demand_i64(
        "dependency_names_text",
        vec![facet_core, dependency_names_kind],
    )?;
    assert_eq!(
        rendered_string(&machine, "dependency_names_text", dependency_names)?,
        "bytes\nbytestring\ncamino\nchrono\ncompact_str\nconst-fnv1a-hash\niddqd\nimpls\nindexmap\njiff\nlock_api\nnum-complex\nordered-float\nruint\nrust_decimal\nsemver\nsmallvec\nsmartstring\nsmol_str\nstable_deref_trait\ntaxon\ntendril\ntime\nulid\nurl\nuuid\nyoke"
    );

    let blake3 = detailed_dep(&mut machine, taxon, workspace, "blake3", "normal")?;
    assert_eq!(field_string(&blake3, "version_req")?, "^1");
    assert_eq!(field_string(&blake3, "path")?, "");
    assert!(field_bool(&blake3, "workspace")?);
    assert!(!field_bool(&blake3, "default_features")?);

    let autocfg = detailed_dep(&mut machine, facet_core, workspace, "autocfg", "build")?;
    assert_eq!(field_string(&autocfg, "version_req")?, "^1.5.0");
    assert_eq!(field_string(&autocfg, "kind")?, "build");
    assert!(field_bool(&autocfg, "workspace")?);

    let bytes = detailed_dep(&mut machine, facet_core, workspace, "bytes", "normal")?;
    assert_eq!(field_string(&bytes, "version_req")?, "^1.11.0");
    assert!(field_bool(&bytes, "workspace")?);
    assert!(field_bool(&bytes, "optional")?);
    assert!(!field_bool(&bytes, "default_features")?);

    let time = detailed_dep(&mut machine, facet_core, workspace, "time", "normal")?;
    assert_eq!(field_string(&time, "version_req")?, "^0.3.44");
    assert!(field_bool(&time, "workspace")?);
    assert!(field_bool(&time, "optional")?);
    assert!(field_bool(&time, "default_features")?);

    let static_assertions = detailed_dep(
        &mut machine,
        facet,
        workspace,
        "static_assertions",
        "normal",
    )?;
    assert_eq!(field_string(&static_assertions, "version_req")?, "^1.1.0");
    assert!(field_bool(&static_assertions, "workspace")?);
    assert!(field_bool(&static_assertions, "optional")?);

    let tempfile = detailed_dep(&mut machine, facet, workspace, "tempfile", "dev")?;
    assert_eq!(field_string(&tempfile, "version_req")?, "^3.23.0");
    assert!(field_bool(&tempfile, "workspace")?);
    assert!(!field_bool(&tempfile, "optional")?);

    let facet_core_dep = detailed_dep(&mut machine, facet, workspace, "facet-core", "normal")?;
    assert_eq!(
        field_string(&facet_core_dep, "version_req")?,
        "=0.50.0-rc.5"
    );
    assert_eq!(field_string(&facet_core_dep, "path")?, "../facet-core");
    assert!(!field_bool(&facet_core_dep, "default_features")?);

    let macros_dep = detailed_dep(&mut machine, facet, workspace, "facet-macros", "normal")?;
    assert_eq!(field_string(&macros_dep, "version_req")?, "0.50.0-rc.5");
    assert_eq!(field_string(&macros_dep, "path")?, "../facet-macros");
    assert!(!field_bool(&macros_dep, "default_features")?);

    let const_hash_name = intern_string(&mut machine, "const-fnv1a-hash")?;
    let normal = intern_string(&mut machine, "normal")?;
    let const_hash = machine.demand_i64(
        "version_dependency_of",
        vec![facet_core, const_hash_name, normal],
    )?;
    let const_hash = record(machine.render_result("version_dependency_of", const_hash)?)?;
    assert_eq!(field_string(&const_hash, "version_req")?, "1");
    assert!(!field_bool(&const_hash, "workspace")?);

    Ok(())
}

#[test]
fn target_dependency_declarations_carry_parsed_cfg_data() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, workspace_tree())?;
    let manifest = intern_tree(
        &mut machine,
        Tree::of(&[(
            "Cargo.toml",
            r#"
[package]
name = "target-demo"
version = "0.1.0"
edition = "2024"

[target.'cfg(all(unix, target_arch = "x86_64"))'.dependencies]
libc = "0.2"
"#,
        )]),
    )?;

    let libc = detailed_target_dep(
        &mut machine,
        manifest,
        workspace,
        r#"cfg(all(unix, target_arch = "x86_64"))"#,
        "libc",
        "normal",
    )?;
    assert_eq!(field_string(&libc, "name")?, "libc");
    assert_eq!(field_string(&libc, "version_req")?, "0.2");
    assert_eq!(
        field_string(&libc, "target")?,
        r#"cfg(all(unix, target_arch = "x86_64"))"#
    );
    assert_eq!(field_doc_tag(&libc, "cfg")?, "all");
    Ok(())
}

#[test]
fn package_workspace_fields_are_inherited_generically() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, workspace_tree())?;
    let facet_core = intern_tree(&mut machine, facet_core_tree())?;

    for (function, expected) in [
        ("package_rust_version", "1.92"),
        ("package_license", "MIT OR Apache-2.0"),
        ("package_repository", "https://github.com/facet-rs/facet"),
    ] {
        let value = machine.demand_i64(function, vec![facet_core, workspace])?;
        assert_eq!(rendered_string(&machine, function, value)?, expected);
    }

    Ok(())
}

#[test]
fn target_shapes_match_cargo_metadata_for_real_members() -> Result<(), String> {
    let metadata = cargo_metadata_oracle()?;
    let cargo_shapes = metadata.target_shapes_for_real_members()?;

    let mut machine = manifest_machine()?;
    let mut vix_shapes = BTreeSet::new();
    for (package, manifest) in [
        ("taxon", taxon_tree()),
        ("facet-core", facet_core_tree()),
        ("facet", facet_tree()),
    ] {
        let manifest = intern_tree(&mut machine, manifest)?;
        let lib = machine.demand_i64("lib_target_shape", vec![manifest])?;
        let lib = record(machine.render_result("lib_target_shape", lib)?)?;
        vix_shapes.insert(target_shape_from_vix(package, &lib)?);
        if package == "facet-core" || package == "facet" {
            let build = machine.demand_i64("build_script_target_shape", vec![manifest])?;
            let build = record(machine.render_result("build_script_target_shape", build)?)?;
            vix_shapes.insert(target_shape_from_vix(package, &build)?);
        }
    }

    assert_eq!(vix_shapes, cargo_shapes);
    Ok(())
}

#[test]
fn profile_sections_and_package_overrides_are_ingested() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(
        &mut machine,
        Tree::of(&[(
            "Cargo.toml",
            r#"
[workspace]
members = []

[profile.dev]
opt-level = 1
debug = 1
debug-assertions = false
overflow-checks = false
panic = "abort"
lto = "thin"
codegen-units = 4
strip = "debuginfo"

[profile.dev.build-override]
opt-level = 2
debug = 0

[profile.dev.package.hot_dep]
opt-level = 3
debug-assertions = true
"#,
        )]),
    )?;
    let package = intern_string(&mut machine, "hot_dep")?;
    let unit_kind = intern_string(&mut machine, "lib")?;
    let profile_name = intern_string(&mut machine, "dev")?;

    let value = machine.demand_i64(
        "resolved_profile_for",
        vec![workspace, package, unit_kind, profile_name],
    )?;
    let profile = record(machine.render_result("resolved_profile_for", value)?)?;

    assert_eq!(field_string(&profile, "name")?, "dev");
    assert_eq!(field_string(&profile, "opt_level")?, "3");
    assert_eq!(field_string(&profile, "debuginfo")?, "1");
    assert_eq!(field_string(&profile, "debug_assertions")?, "true");
    assert_eq!(field_string(&profile, "overflow_checks")?, "false");
    assert_eq!(field_string(&profile, "panic")?, "abort");
    assert_eq!(field_string(&profile, "lto")?, "thin");
    assert_eq!(field_string(&profile, "codegen_units")?, "4");
    assert_eq!(field_string(&profile, "strip")?, "debuginfo");

    let unit_kind = intern_string(&mut machine, "build-script")?;
    let package = intern_string(&mut machine, "hot_dep")?;
    let profile_name = intern_string(&mut machine, "dev")?;
    let value = machine.demand_i64(
        "resolved_profile_for",
        vec![workspace, package, unit_kind, profile_name],
    )?;
    let build_profile = record(machine.render_result("resolved_profile_for", value)?)?;
    assert_eq!(field_string(&build_profile, "opt_level")?, "3");
    assert_eq!(field_string(&build_profile, "debuginfo")?, "0");

    let unit_kind = intern_string(&mut machine, "proc-macro")?;
    let package = intern_string(&mut machine, "plain_proc_macro")?;
    let profile_name = intern_string(&mut machine, "dev")?;
    let value = machine.demand_i64(
        "resolved_profile_for",
        vec![workspace, package, unit_kind, profile_name],
    )?;
    let proc_macro_profile = record(machine.render_result("resolved_profile_for", value)?)?;
    assert_eq!(field_string(&proc_macro_profile, "opt_level")?, "2");
    assert_eq!(field_string(&proc_macro_profile, "debuginfo")?, "0");

    Ok(())
}

#[test]
fn real_workspace_profile_package_overrides_match_live_manifest() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let workspace_manifest = std::fs::read_to_string(workspace_root().join("Cargo.toml"))
        .map_err(|err| err.to_string())?;
    let workspace = intern_tree(
        &mut machine,
        Tree::of(&[("Cargo.toml", workspace_manifest.as_str())]),
    )?;

    for package in ["aho-corasick", "blake3", "sha2"] {
        let package_arg = intern_string(&mut machine, package)?;
        let unit_kind = intern_string(&mut machine, "lib")?;
        let profile_name = intern_string(&mut machine, "dev")?;
        let value = machine.demand_i64(
            "resolved_profile_for",
            vec![workspace, package_arg, unit_kind, profile_name],
        )?;
        let profile = record(machine.render_result("resolved_profile_for", value)?)?;
        assert_eq!(
            field_string(&profile, "opt_level")?,
            "3",
            "{package} should inherit [profile.dev.package.{package}] opt-level"
        );
        assert_eq!(field_string(&profile, "debuginfo")?, "2");
    }

    Ok(())
}

#[test]
fn rodin_problem_shape_is_available_for_the_manifest_adapter() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let root_pkg = 7;
    let root_default_feature = 13;
    let value = machine.demand_i64("problem_of_member", vec![root_pkg, root_default_feature])?;
    let problem = record(machine.render_result("problem_of_member", value)?)?;
    assert_eq!(field_int(&problem, "root_pkg")?, root_pkg);
    assert_eq!(field_string(&problem, "root_req")?, "R * *\n");
    assert_eq!(
        field_int(&problem, "root_default_feature")?,
        root_default_feature
    );
    assert!(field_bool(&problem, "root_default_features")?);
    Ok(())
}

#[test]
fn direct_resolved_unit_adapter_gap_is_pinned() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let value = machine.demand_i64("resolved_unit_adaptation_gap", vec![])?;
    let gap = rendered_string(&machine, "resolved_unit_adaptation_gap", value)?;
    assert!(
        gap.contains("Path construction is join-only from a granted root")
            && gap.contains("sparse-index composition")
            && gap.contains("UnitTargetTable derivation"),
        "{gap}"
    );
    Ok(())
}

#[test]
fn real_workspace_metadata_baseline_is_counted() -> Result<(), String> {
    let metadata = cargo_metadata_real_workspace()?;
    let mut machine = manifest_machine()?;
    let workspace_manifest = std::fs::read_to_string(workspace_root().join("Cargo.toml"))
        .map_err(|err| err.to_string())?;
    let workspace = intern_tree(
        &mut machine,
        Tree::of(&[("Cargo.toml", &workspace_manifest)]),
    )?;
    let vix_member_count = machine.demand_i64("workspace_member_count", vec![workspace])?;
    let workspace_members: BTreeSet<_> = metadata.workspace_members.iter().collect();
    let mut total_oracle_deps = 0usize;
    let mut target_cfg_represented = 0usize;
    let mut before_workspace_allowlist_failures = 0usize;

    for package in metadata
        .packages
        .iter()
        .filter(|package| workspace_members.contains(&package.id))
    {
        let manifest_text = std::fs::read_to_string(&package.manifest_path).map_err(|err| {
            format!(
                "read manifest for {} at {}: {err}",
                package.name, package.manifest_path
            )
        })?;
        before_workspace_allowlist_failures += legacy_workspace_allowlist_failures(&manifest_text);
        total_oracle_deps += package.dependencies.len();
        target_cfg_represented += package
            .dependencies
            .iter()
            .filter(|dependency| dependency.target.is_some())
            .count();
    }

    assert_eq!(workspace_members.len(), 145);
    assert_eq!(vix_member_count, 145);
    assert_eq!(total_oracle_deps, 1127);
    assert_eq!(before_workspace_allowlist_failures, 762);
    assert_eq!(target_cfg_represented, 55);

    Ok(())
}

#[test]
fn real_workspace_member_only_index_builds_bounded_ring() -> Result<(), String> {
    let metadata = cargo_metadata_real_workspace()?;
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, real_workspace_manifest_tree(&metadata)?)?;
    let root = intern_path(&mut machine, "")?;
    let limit = 16;

    let package_count = machine.demand_i64(
        "workspace_member_only_index_package_count_limit",
        vec![workspace, root, limit],
    )?;
    let clause_count = machine.demand_i64(
        "workspace_member_only_index_clause_count_limit",
        vec![workspace, root, limit],
    )?;
    let workspace_members: BTreeSet<_> = metadata.workspace_members.iter().collect();

    assert_eq!(workspace_members.len(), 145);
    assert_eq!(package_count, limit + 1);
    assert_eq!(clause_count, limit * 2);
    Ok(())
}

#[test]
#[ignore = "tier-A measurement probe: real workspace member-only solve is semantically empty"]
fn real_workspace_member_index_solves_bounded_ring() -> Result<(), String> {
    real_workspace_member_only_solve_ring(16)
}

macro_rules! real_workspace_member_only_solve_ring_test {
    ($name:ident, $limit:expr) => {
        #[test]
        #[ignore = "tier-A measurement probe: real workspace member-only solve ring is semantically empty"]
        fn $name() -> Result<(), String> {
            real_workspace_member_only_solve_ring($limit)
        }
    };
}

real_workspace_member_only_solve_ring_test!(real_workspace_member_only_solve_ring_1, 1);
real_workspace_member_only_solve_ring_test!(real_workspace_member_only_solve_ring_2, 2);
real_workspace_member_only_solve_ring_test!(real_workspace_member_only_solve_ring_4, 4);
real_workspace_member_only_solve_ring_test!(real_workspace_member_only_solve_ring_8, 8);
real_workspace_member_only_solve_ring_test!(real_workspace_member_only_solve_ring_16, 16);

fn real_workspace_member_only_solve_ring(limit: i64) -> Result<(), String> {
    let metadata = cargo_metadata_real_workspace()?;
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, real_workspace_manifest_tree(&metadata)?)?;
    let root = intern_path(&mut machine, "")?;
    let target = intern_string(&mut machine, "x86_64-apple-darwin")?;

    let package_count = machine.demand_i64(
        "workspace_member_only_index_package_count_limit",
        vec![workspace, root, limit],
    )?;
    let selected_member_count = machine.demand_i64(
        "workspace_member_only_solve_selected_member_count_limit",
        vec![workspace, root, target, limit],
    )?;

    assert_eq!(package_count, limit + 1);
    assert_eq!(selected_member_count, limit);
    Ok(())
}

#[test]
#[ignore = "tier-A measurement probe: real workspace member-only ring 16 lock diff"]
fn real_workspace_member_only_solve_ring_16_lock_diff() -> Result<(), String> {
    let limit = 16;
    let metadata = cargo_metadata_real_workspace()?;
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, real_workspace_manifest_tree(&metadata)?)?;
    let root = intern_path(&mut machine, "")?;
    let target = intern_string(&mut machine, "x86_64-apple-darwin")?;

    let selected = machine.demand_i64(
        "workspace_member_only_solve_selected_versions_text_limit",
        vec![workspace, root, target, limit],
    )?;
    let selected = rendered_string(
        &machine,
        "workspace_member_only_solve_selected_versions_text_limit",
        selected,
    )?;
    let solve_rows = package_versions_from_solve_text(&selected)?;
    let lock_rows = cargo_lock_package_rows(&workspace_root().join("Cargo.lock"))?;
    let metadata_rows = metadata.package_rows();
    let diff = diff_package_versions_against_lock(&solve_rows, &lock_rows, &metadata_rows);

    assert_eq!(diff.solve_rows, 17, "{diff:#?}");
    assert_eq!(diff.matches, 16, "{diff:#?}");
    assert_eq!(diff.solve_only, 1, "{diff:#?}");
    assert_eq!(
        diff.solve_only_categories.get("workspace-pseudo-root"),
        Some(&1),
        "{diff:#?}"
    );
    assert_eq!(diff.lock_only + diff.matches, diff.lock_rows, "{diff:#?}");

    write_tier_a_artifact(
        "real-ring-16-solve-vs-lock-summary.tsv",
        &package_diff_summary_table(&diff),
    )?;
    write_tier_a_artifact(
        "real-ring-16-solve-vs-lock-solve-rows.tsv",
        &package_rows_table(&solve_rows),
    )?;
    Ok(())
}

#[test]
#[ignore = "tier-A measurement probe: full member-only index construction"]
fn real_workspace_member_only_index_builds_all_members() -> Result<(), String> {
    let metadata = cargo_metadata_real_workspace()?;
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, real_workspace_manifest_tree(&metadata)?)?;
    let root = intern_path(&mut machine, "")?;

    let package_count = machine.demand_i64(
        "workspace_member_only_index_package_count",
        vec![workspace, root],
    )?;
    let clause_count = machine.demand_i64(
        "workspace_member_only_index_clause_count",
        vec![workspace, root],
    )?;

    assert_eq!(package_count, 146);
    assert_eq!(clause_count, 290);
    Ok(())
}

#[test]
#[ignore = "tier-A measurement probe: direct-dep clause construction is currently over nextest's default timeout"]
fn real_workspace_member_index_builds_required_direct_dep_clauses() -> Result<(), String> {
    let metadata = cargo_metadata_real_workspace()?;
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, real_workspace_manifest_tree(&metadata)?)?;
    let root = intern_path(&mut machine, "")?;

    let clause_count =
        machine.demand_i64("workspace_member_index_clause_count", vec![workspace, root])?;
    let direct_clause_count = machine.demand_i64(
        "workspace_member_direct_dep_clause_count",
        vec![workspace, root],
    )?;

    assert!(clause_count > 290, "clause_count={clause_count}");
    assert_eq!(direct_clause_count, clause_count - 290);
    assert_eq!(direct_clause_count % 2, 0);
    Ok(())
}

#[test]
#[ignore = "tier-A measurement probe: bounded direct-dep solve currently hits molten handle -1"]
fn real_workspace_member_index_solves_bounded_direct_dep_ring() -> Result<(), String> {
    let metadata = cargo_metadata_real_workspace()?;
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, real_workspace_manifest_tree(&metadata)?)?;
    let root = intern_path(&mut machine, "")?;
    let target = intern_string(&mut machine, "x86_64-apple-darwin")?;
    let limit = 16;

    let selected_member_count = machine.demand_i64(
        "workspace_member_solve_selected_member_count_limit",
        vec![workspace, root, target, limit],
    )?;

    assert_eq!(selected_member_count, limit);
    Ok(())
}

#[test]
#[ignore = "tier-A measurement probe: full interpreted vix solve exceeds nextest's default timeout"]
fn real_workspace_member_index_solves_all_members() -> Result<(), String> {
    let metadata = cargo_metadata_real_workspace()?;
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, real_workspace_manifest_tree(&metadata)?)?;
    let root = intern_path(&mut machine, "")?;
    let target = intern_string(&mut machine, "x86_64-apple-darwin")?;

    let selected_member_count = machine.demand_i64(
        "workspace_member_only_solve_selected_member_count",
        vec![workspace, root, target],
    )?;

    assert_eq!(selected_member_count, 145);
    Ok(())
}

macro_rules! real_workspace_dependency_probe_test {
    ($name:ident, $shard:expr) => {
        #[test]
        fn $name() -> Result<(), String> {
            real_workspace_dependency_probe_shard($shard, 16)
        }
    };
}

real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_0, 0);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_1, 1);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_2, 2);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_3, 3);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_4, 4);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_5, 5);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_6, 6);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_7, 7);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_8, 8);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_9, 9);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_10, 10);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_11, 11);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_12, 12);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_13, 13);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_14, 14);
real_workspace_dependency_probe_test!(real_workspace_dependency_probe_shard_15, 15);

fn real_workspace_dependency_probe_shard(shard: usize, shards: usize) -> Result<(), String> {
    let metadata = cargo_metadata_real_workspace()?;
    let workspace_root = workspace_root();
    let workspace_manifest = std::fs::read_to_string(workspace_root.join("Cargo.toml"))
        .map_err(|err| err.to_string())?;
    let workspace = Tree::of(&[("Cargo.toml", workspace_manifest.as_str())]);
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, workspace)?;

    let workspace_members: BTreeSet<_> = metadata.workspace_members.iter().collect();
    let mut selected_oracle_deps = 0usize;
    let mut target_cfg_remainder = 0usize;
    let mut name_kind_mismatches = 0usize;
    let mut vix_errors = 0usize;
    let mut probed = 0usize;
    let mut examples = Vec::new();
    let mut dep_index = 0usize;

    for package in metadata
        .packages
        .iter()
        .filter(|package| workspace_members.contains(&package.id))
    {
        let manifest_path = Path::new(&package.manifest_path);
        let manifest_text = std::fs::read_to_string(manifest_path).map_err(|err| {
            format!(
                "read manifest for {} at {}: {err}",
                package.name, package.manifest_path
            )
        })?;
        let manifest = intern_tree(
            &mut machine,
            Tree::of(&[("Cargo.toml", manifest_text.as_str())]),
        )?;

        for dependency in &package.dependencies {
            let selected = dep_index % shards == shard;
            dep_index += 1;
            if !selected {
                continue;
            }
            selected_oracle_deps += 1;
            probed += 1;
            let kind = dependency.kind.as_deref().unwrap_or("normal");
            let key = dependency.key();
            let actual = match dependency.target.as_deref() {
                Some(target) => {
                    detailed_target_dep(&mut machine, manifest, workspace, target, key, kind)
                }
                None => detailed_dep(&mut machine, manifest, workspace, key, kind),
            };
            match actual {
                Ok(actual) => {
                    let actual_name = field_string(&actual, "name")?;
                    let actual_kind = field_string(&actual, "kind")?;
                    let actual_target = field_string(&actual, "target")?;
                    if actual_target != dependency.target.as_deref().unwrap_or("") {
                        target_cfg_remainder += 1;
                        push_example(
                            &mut examples,
                            format!(
                                "{}:{} expected target {:?}, got {actual_target:?}",
                                package.name, key, dependency.target
                            ),
                        );
                    }
                    if actual_name != key || actual_kind != kind {
                        name_kind_mismatches += 1;
                        push_example(
                            &mut examples,
                            format!(
                                "{}:{} expected {key}/{kind}, got {actual_name}/{actual_kind}",
                                package.name, key
                            ),
                        );
                    }
                }
                Err(err) => {
                    vix_errors += 1;
                    push_example(
                        &mut examples,
                        format!("{}:{}:{kind} -> {err}", package.name, key),
                    );
                }
            }
        }
    }

    let summary = RealWorkspaceProbeSummary {
        shard,
        shards,
        selected_oracle_deps,
        probed,
        target_cfg_remainder,
        name_kind_mismatches,
        vix_errors,
        examples,
    };

    assert!(summary.shard < summary.shards, "{summary:#?}");
    assert!(summary.selected_oracle_deps > 0, "{summary:#?}");
    assert!(summary.probed > 0, "{summary:#?}");
    assert_eq!(summary.selected_oracle_deps, summary.probed, "{summary:#?}");
    assert_eq!(summary.target_cfg_remainder, 0, "{summary:#?}");
    assert_eq!(summary.name_kind_mismatches, 0, "{summary:#?}");
    assert_eq!(summary.vix_errors, 0, "{summary:#?}");
    assert!(summary.examples.is_empty(), "{summary:#?}");

    Ok(())
}

fn manifest_machine() -> Result<Machine, String> {
    Machine::load(&format!("{RODIN_SOURCE}\n\n{SOURCE}"))
}

fn detailed_dep(
    machine: &mut Machine,
    manifest: i64,
    workspace: i64,
    name: &str,
    kind: &str,
) -> Result<BTreeMap<String, RenderedValue>, String> {
    let name = intern_string(machine, name)?;
    let kind = intern_string(machine, kind)?;
    let value = machine.demand_i64(
        "detailed_dependency_of",
        vec![manifest, workspace, name, kind],
    )?;
    record(machine.render_result("detailed_dependency_of", value)?)
}

fn detailed_target_dep(
    machine: &mut Machine,
    manifest: i64,
    workspace: i64,
    target: &str,
    name: &str,
    kind: &str,
) -> Result<BTreeMap<String, RenderedValue>, String> {
    let target = intern_string(machine, target)?;
    let name = intern_string(machine, name)?;
    let kind = intern_string(machine, kind)?;
    let value = machine.demand_i64(
        "detailed_target_dependency_of",
        vec![manifest, workspace, target, name, kind],
    )?;
    record(machine.render_result("detailed_target_dependency_of", value)?)
}

fn intern_tree(machine: &mut Machine, tree: Tree) -> Result<i64, String> {
    Ok(machine.intern_arg("Tree", MachineArg::Tree(tree))?.0)
}

fn intern_string(machine: &mut Machine, value: &str) -> Result<i64, String> {
    Ok(machine
        .intern_arg("String", MachineArg::String(value.to_owned()))?
        .0)
}

fn intern_path(machine: &mut Machine, value: &str) -> Result<i64, String> {
    Ok(machine
        .intern_arg("Path", MachineArg::Path(value.to_owned()))?
        .0)
}

fn rendered_string(machine: &Machine, name: &str, value: i64) -> Result<String, String> {
    match machine.render_result(name, value)? {
        RenderedValue::String { value } => Ok(value),
        other => Err(format!("{name} rendered as {other:?}, not String")),
    }
}

fn record(value: RenderedValue) -> Result<BTreeMap<String, RenderedValue>, String> {
    let RenderedValue::Record { fields, .. } = value else {
        return Err(format!("value rendered as {value:?}, not Record"));
    };
    Ok(fields
        .into_iter()
        .map(|field| (field.name, field.value))
        .collect())
}

fn field_string(fields: &BTreeMap<String, RenderedValue>, name: &str) -> Result<String, String> {
    match fields.get(name) {
        Some(RenderedValue::String { value }) => Ok(value.clone()),
        Some(RenderedValue::VersionSet { value }) => Ok(value.clone()),
        other => Err(format!("field {name} was {other:?}, not String")),
    }
}

fn target_shape_from_vix(
    package: &str,
    target: &BTreeMap<String, RenderedValue>,
) -> Result<TargetShape, String> {
    Ok(TargetShape {
        package: package.to_owned(),
        name: field_string(target, "name")?,
        kind: field_string(target, "kind")?,
        source_suffix: field_string(target, "source")?,
        crate_type: field_string(target, "crate_type")?,
    })
}

fn field_bool(fields: &BTreeMap<String, RenderedValue>, name: &str) -> Result<bool, String> {
    match fields.get(name) {
        Some(RenderedValue::Bool { value }) => Ok(*value),
        other => Err(format!("field {name} was {other:?}, not Bool")),
    }
}

fn field_int(fields: &BTreeMap<String, RenderedValue>, name: &str) -> Result<i64, String> {
    match fields.get(name) {
        Some(RenderedValue::Int { value }) => Ok(*value),
        other => Err(format!("field {name} was {other:?}, not Int")),
    }
}

fn field_doc_tag(fields: &BTreeMap<String, RenderedValue>, name: &str) -> Result<String, String> {
    match fields.get(name) {
        Some(RenderedValue::Doc {
            variant,
            value: Some(value),
        }) if variant == "Map" => match &**value {
            RenderedValue::Map { entries, .. } => entries
                .iter()
                .find_map(|entry| match (&entry.key, &entry.value) {
                    (
                        RenderedValue::String { value: key },
                        RenderedValue::Doc {
                            variant,
                            value: Some(value),
                        },
                    ) if key == "tag" && variant == "String" => match &**value {
                        RenderedValue::String { value } => Some(Ok(value.clone())),
                        other => Some(Err(format!("cfg tag rendered as {other:?}"))),
                    },
                    _ => None,
                })
                .unwrap_or_else(|| Err("cfg doc had no tag".to_string())),
            other => Err(format!("cfg doc map payload rendered as {other:?}")),
        },
        other => Err(format!("field {name} was {other:?}, not Doc::Map")),
    }
}

fn workspace_tree() -> Tree {
    Tree::of(&[("Cargo.toml", WORKSPACE_MANIFEST)])
}

fn real_workspace_manifest_tree(metadata: &CargoMetadata) -> Result<Tree, String> {
    let root = workspace_root();
    let workspace_members: BTreeSet<_> = metadata.workspace_members.iter().collect();
    let mut entries = BTreeMap::new();
    entries.insert(
        "Cargo.toml".to_owned(),
        std::fs::read_to_string(root.join("Cargo.toml")).map_err(|err| err.to_string())?,
    );
    for package in metadata
        .packages
        .iter()
        .filter(|package| workspace_members.contains(&package.id))
    {
        let manifest_path = Path::new(&package.manifest_path);
        let relative = manifest_path.strip_prefix(&root).map_err(|err| {
            format!(
                "{} was not under {}: {err}",
                package.manifest_path,
                root.display()
            )
        })?;
        let relative = relative
            .to_str()
            .ok_or_else(|| format!("manifest path was not utf-8: {}", package.manifest_path))?
            .to_owned();
        let contents = std::fs::read_to_string(manifest_path).map_err(|err| {
            format!(
                "read manifest for {} at {}: {err}",
                package.name, package.manifest_path
            )
        })?;
        entries.insert(relative, contents);
    }
    Ok(Tree {
        entries,
        blobs: BTreeMap::new(),
    })
}

fn taxon_tree() -> Tree {
    Tree::of(&[("Cargo.toml", TAXON_MANIFEST), ("src/lib.rs", TAXON_LIB)])
}

fn facet_core_tree() -> Tree {
    Tree::of(&[
        ("Cargo.toml", FACET_CORE_MANIFEST),
        ("src/lib.rs", FACET_CORE_LIB),
        ("build.rs", FACET_CORE_BUILD),
    ])
}

fn facet_tree() -> Tree {
    Tree::of(&[
        ("Cargo.toml", FACET_MANIFEST),
        ("src/lib.rs", FACET_LIB),
        ("build.rs", FACET_BUILD),
    ])
}

fn cargo_metadata_oracle() -> Result<CargoMetadata, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../playgrounds/snark/src/bundled/vix/samples/fixtures/cargo_manifest_real/Cargo.toml",
    );
    cargo_metadata_for_manifest(&manifest, true)
}

fn cargo_metadata_real_workspace() -> Result<CargoMetadata, String> {
    cargo_metadata_for_manifest(&workspace_root().join("Cargo.toml"), false)
}

fn cargo_metadata_for_manifest(manifest: &Path, no_deps: bool) -> Result<CargoMetadata, String> {
    let mut command = Command::new("cargo");
    command.args(["metadata", "--format-version", "1"]);
    if !no_deps {
        command.arg("--locked");
    }
    if no_deps {
        command.arg("--no-deps");
    }
    command.arg("--manifest-path").arg(manifest);
    let output = command.output().map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
    facet_json::from_str(&stdout).map_err(|err| err.to_string())
}

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("vix crate has workspace parent")
        .to_path_buf()
}

fn legacy_workspace_allowlist_failures(manifest: &str) -> usize {
    let mut section = "";
    let mut failures = 0;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed;
            continue;
        }
        let dependency_section = matches!(
            section,
            "[dependencies]" | "[build-dependencies]" | "[dev-dependencies]"
        ) || (section.starts_with("[target.")
            && section.contains(".dependencies]"));
        if !dependency_section || !trimmed.contains("workspace = true") {
            continue;
        }
        let key = trimmed
            .split_once('=')
            .map(|(key, _)| key.trim())
            .unwrap_or(trimmed)
            .strip_suffix(".workspace")
            .unwrap_or_else(|| {
                trimmed
                    .split_once('=')
                    .map(|(key, _)| key.trim())
                    .unwrap_or(trimmed)
            })
            .trim();
        if key != "blake3" && key != "autocfg" {
            failures += 1;
        }
    }
    failures
}

fn push_example(examples: &mut Vec<String>, example: String) {
    if examples.len() < 8 {
        examples.push(example);
    }
}

#[derive(Debug)]
struct RealWorkspaceProbeSummary {
    shard: usize,
    shards: usize,
    selected_oracle_deps: usize,
    probed: usize,
    target_cfg_remainder: usize,
    name_kind_mismatches: usize,
    vix_errors: usize,
    examples: Vec<String>,
}

#[derive(Debug, Facet)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    workspace_members: Vec<String>,
}

impl CargoMetadata {
    fn target_shapes_for_real_members(&self) -> Result<BTreeSet<TargetShape>, String> {
        self.packages
            .iter()
            .filter(|package| REAL_MEMBERS.contains(&package.name.as_str()))
            .flat_map(|package| {
                package.targets.iter().map(|target| {
                    Ok(TargetShape {
                        package: package.name.clone(),
                        name: target.name.replace('-', "_"),
                        kind: target.normalized_kind()?,
                        source_suffix: source_suffix(&target.src_path)?,
                        crate_type: target.crate_types.first().cloned().ok_or_else(|| {
                            format!("target {:?} had no crate_types", target.name)
                        })?,
                    })
                })
            })
            .collect()
    }

    fn package_rows(&self) -> BTreeSet<PackageVersion> {
        self.packages
            .iter()
            .map(|package| PackageVersion::new(&package.name, &package.version))
            .collect()
    }
}

#[derive(Debug, Facet)]
struct CargoPackage {
    id: String,
    name: String,
    version: String,
    edition: String,
    manifest_path: String,
    dependencies: Vec<CargoDependency>,
    targets: Vec<CargoTarget>,
}

#[derive(Debug, Facet)]
struct CargoLock {
    package: Vec<CargoLockPackage>,
}

#[derive(Debug, Facet)]
struct CargoLockPackage {
    name: String,
    version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PackageVersion {
    name: String,
    version: String,
}

impl PackageVersion {
    fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.to_owned(),
            version: version.to_owned(),
        }
    }
}

#[derive(Debug)]
struct PackageLockDiffSummary {
    solve_rows: usize,
    lock_rows: usize,
    matches: usize,
    solve_only: usize,
    lock_only: usize,
    version_skew_names: usize,
    solve_only_categories: BTreeMap<&'static str, usize>,
    lock_only_categories: BTreeMap<&'static str, usize>,
}

fn package_versions_from_solve_text(text: &str) -> Result<BTreeSet<PackageVersion>, String> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (name, version) = line
                .split_once(' ')
                .ok_or_else(|| format!("selected package row was not `name version`: {line:?}"))?;
            Ok(PackageVersion::new(name, version))
        })
        .collect()
}

fn cargo_lock_package_rows(path: &Path) -> Result<BTreeSet<PackageVersion>, String> {
    let text = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    let lock: CargoLock = facet_toml::from_str(&text).map_err(|err| err.to_string())?;
    Ok(lock
        .package
        .iter()
        .map(|package| PackageVersion::new(&package.name, &package.version))
        .collect())
}

fn diff_package_versions_against_lock(
    solve_rows: &BTreeSet<PackageVersion>,
    lock_rows: &BTreeSet<PackageVersion>,
    metadata_rows: &BTreeSet<PackageVersion>,
) -> PackageLockDiffSummary {
    let matches = solve_rows.intersection(lock_rows).count();
    let solve_only_rows = solve_rows.difference(lock_rows).collect::<Vec<_>>();
    let lock_only_rows = lock_rows.difference(solve_rows).collect::<Vec<_>>();
    let lock_names = lock_rows
        .iter()
        .map(|row| row.name.as_str())
        .collect::<BTreeSet<_>>();
    let solve_names = solve_rows
        .iter()
        .map(|row| row.name.as_str())
        .collect::<BTreeSet<_>>();
    let version_skew_names = solve_only_rows
        .iter()
        .filter(|row| lock_names.contains(row.name.as_str()))
        .map(|row| row.name.as_str())
        .collect::<BTreeSet<_>>()
        .len();

    let mut solve_only_categories = BTreeMap::new();
    for row in &solve_only_rows {
        bump(
            &mut solve_only_categories,
            if row.name == "__workspace__" {
                "workspace-pseudo-root"
            } else if metadata_rows.contains(row) {
                "cargo-selected-but-lock-missing"
            } else if lock_names.contains(row.name.as_str()) {
                "version-skew"
            } else {
                "solve-only-unknown-or-fixture"
            },
        );
    }

    let mut lock_only_categories = BTreeMap::new();
    for row in &lock_only_rows {
        bump(
            &mut lock_only_categories,
            if metadata_rows.contains(row) {
                "cargo-selected-not-in-solve"
            } else if solve_names.contains(row.name.as_str()) {
                "version-skew"
            } else {
                "lock-residue-not-selected-by-metadata"
            },
        );
    }

    PackageLockDiffSummary {
        solve_rows: solve_rows.len(),
        lock_rows: lock_rows.len(),
        matches,
        solve_only: solve_only_rows.len(),
        lock_only: lock_only_rows.len(),
        version_skew_names,
        solve_only_categories,
        lock_only_categories,
    }
}

fn package_diff_summary_table(diff: &PackageLockDiffSummary) -> String {
    let mut lines = vec![
        "metric\tcount".to_owned(),
        format!("solve_rows\t{}", diff.solve_rows),
        format!("lock_rows\t{}", diff.lock_rows),
        format!("matches\t{}", diff.matches),
        format!("solve_only\t{}", diff.solve_only),
        format!("lock_only\t{}", diff.lock_only),
        format!("version_skew_names\t{}", diff.version_skew_names),
    ];
    for (category, count) in &diff.solve_only_categories {
        lines.push(format!("solve_only:{category}\t{count}"));
    }
    for (category, count) in &diff.lock_only_categories {
        lines.push(format!("lock_only:{category}\t{count}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn package_rows_table(rows: &BTreeSet<PackageVersion>) -> String {
    let mut lines = vec!["name\tversion".to_owned()];
    lines.extend(
        rows.iter()
            .map(|row| format!("{}\t{}", row.name, row.version)),
    );
    lines.push(String::new());
    lines.join("\n")
}

fn bump(map: &mut BTreeMap<&'static str, usize>, key: &'static str) {
    *map.entry(key).or_default() += 1;
}

fn write_tier_a_artifact(relative: &str, contents: &str) -> Result<(), String> {
    let Ok(root) = std::env::var("TIER_A_OUT") else {
        return Ok(());
    };
    let path = Path::new(&root).join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    std::fs::write(path, contents).map_err(|err| err.to_string())
}

#[derive(Debug, Facet)]
struct CargoDependency {
    name: String,
    kind: Option<String>,
    rename: Option<String>,
    target: Option<String>,
}

impl CargoDependency {
    fn key(&self) -> &str {
        self.rename.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Debug, Facet)]
struct CargoTarget {
    name: String,
    src_path: String,
    crate_types: Vec<String>,
    kind: Vec<String>,
}

impl CargoTarget {
    fn normalized_kind(&self) -> Result<String, String> {
        let kind = self
            .kind
            .first()
            .ok_or_else(|| format!("target {:?} had no kind", self.name))?;
        Ok(match kind.as_str() {
            "custom-build" => "build-script".to_owned(),
            other => other.to_owned(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TargetShape {
    package: String,
    name: String,
    kind: String,
    source_suffix: String,
    crate_type: String,
}

fn source_suffix(path: &str) -> Result<String, String> {
    for suffix in ["src/lib.rs", "src/main.rs", "build.rs"] {
        if path.ends_with(suffix) {
            return Ok(suffix.to_owned());
        }
    }
    Err(format!("unexpected source path {path}"))
}

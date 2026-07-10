use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use facet::Facet;
use semver::{Version as SemverVersion, VersionReq};
use vix::exec::Tree;
use vix::machine::driver::Lane;
use vix::machine::{Machine, MachineArg, RenderedValue};

const SOURCE: &str =
    include_str!("../../playgrounds/snark/src/bundled/vix/samples/cargo_manifest.vix");
const RODIN_SOURCE: &str = include_str!("../../rodin/rodin.vix");
const TOKIO_1_52_3_SPARSE_ROW: &str = r#"{"name":"tokio","vers":"1.52.3","deps":[],"features":{"rt-multi-thread":["rt"]},"yanked":false}"#;

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

const REAL_MEMBERS: [&str; 3] = ["zztaxon", "zzfacet-core", "zzfacet"];

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
        ("zztaxon", taxon_tree(), "phon/rust/taxon"),
        ("zzfacet-core", facet_core_tree(), "facet-core"),
        ("zzfacet", facet_tree(), "facet"),
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
        "bytes\nbytestring\ncamino\nchrono\ncompact_str\nconst-fnv1a-hash\niddqd\nimpls\nindexmap\njiff\nlock_api\nnum-complex\nordered-float\nruint\nrust_decimal\nsemver\nsmallvec\nsmartstring\nsmol_str\nstable_deref_trait\ntendril\ntime\nulid\nurl\nuuid\nyoke\nzztaxon"
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

    let facet_core_dep = detailed_dep(&mut machine, facet, workspace, "zzfacet-core", "normal")?;
    assert_eq!(
        field_string(&facet_core_dep, "version_req")?,
        "=0.50.0-rc.5"
    );
    assert_eq!(field_string(&facet_core_dep, "path")?, "../facet-core");
    assert!(!field_bool(&facet_core_dep, "default_features")?);

    let macros_dep = detailed_dep(&mut machine, facet, workspace, "zzfacet-macros", "normal")?;
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
fn workspace_dependency_features_preserve_hyphenated_inherited_names() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(
        &mut machine,
        Tree::of(&[(
            "Cargo.toml",
            r#"[workspace]
[workspace.dependencies]
tokio = { version = "1", features = ["io-util", "rt", "rt-multi-thread"] }
"#,
        )]),
    )?;
    let manifest = intern_tree(
        &mut machine,
        Tree::of(&[(
            "Cargo.toml",
            r#"[package]
name = "peer-server"
version = "0.2.2"

[dependencies]
tokio = { workspace = true, features = ["rt-multi-thread", "net", "macros"] }
"#,
        )]),
    )?;

    let tokio = detailed_dep(&mut machine, manifest, workspace, "tokio", "normal")?;
    assert_eq!(
        field_strings(&tokio, "features")?,
        ["io-util", "rt", "rt-multi-thread", "macros", "net"]
    );
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
        ("zztaxon", taxon_tree()),
        ("zzfacet-core", facet_core_tree()),
        ("zzfacet", facet_tree()),
    ] {
        let manifest = intern_tree(&mut machine, manifest)?;
        let lib = machine.demand_i64("lib_target_shape", vec![manifest])?;
        let lib = record(machine.render_result("lib_target_shape", lib)?)?;
        vix_shapes.insert(target_shape_from_vix(package, &lib)?);
        if package == "zzfacet-core" || package == "zzfacet" {
            let build = machine.demand_i64("build_script_target_shape", vec![manifest])?;
            let build = record(machine.render_result("build_script_target_shape", build)?)?;
            vix_shapes.insert(target_shape_from_vix(package, &build)?);
        }
    }

    assert_eq!(vix_shapes, cargo_shapes);
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
        gap.contains("Ring-scale solution unit rows")
            && gap.contains("composed workspace+sparse solve")
            && gap.contains("feature-name UnitTargetTable emission"),
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

    assert_eq!(workspace_members.len(), 147);
    assert_eq!(vix_member_count, 147);
    assert_eq!(total_oracle_deps, 1135);
    assert_eq!(before_workspace_allowlist_failures, 765);
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

    assert_eq!(workspace_members.len(), 147);
    assert_eq!(package_count, limit + 1);
    assert_eq!(clause_count, 35);
    Ok(())
}

#[test]
#[ignore = "tier-A measurement probe: real workspace member-only solve bounded ring"]
fn real_workspace_member_index_solves_bounded_ring() -> Result<(), String> {
    real_workspace_member_only_solve_ring(16)
}

macro_rules! real_workspace_member_only_solve_ring_test {
    ($name:ident, $limit:expr) => {
        #[test]
        #[ignore = "tier-A measurement probe: real workspace member-only solve ring"]
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
real_workspace_member_only_solve_ring_test!(real_workspace_member_only_solve_ring_32, 32);
real_workspace_member_only_solve_ring_test!(real_workspace_member_only_solve_ring_64, 64);
real_workspace_member_only_solve_ring_test!(real_workspace_member_only_solve_ring_146, 146);

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

macro_rules! real_workspace_member_only_solve_ring_lock_diff_test {
    ($name:ident, $limit:expr) => {
        #[test]
        #[ignore = "tier-A measurement probe: real workspace member-only solve ring lock diff"]
        fn $name() -> Result<(), String> {
            real_workspace_member_only_solve_ring_lock_diff($limit)
        }
    };
}

real_workspace_member_only_solve_ring_lock_diff_test!(
    real_workspace_member_only_solve_ring_lock_diff_16,
    16
);
real_workspace_member_only_solve_ring_lock_diff_test!(
    real_workspace_member_only_solve_ring_lock_diff_32,
    32
);
real_workspace_member_only_solve_ring_lock_diff_test!(
    real_workspace_member_only_solve_ring_lock_diff_64,
    64
);
real_workspace_member_only_solve_ring_lock_diff_test!(
    real_workspace_member_only_solve_ring_lock_diff_146,
    146
);

fn real_workspace_member_only_solve_ring_lock_diff(limit: i64) -> Result<(), String> {
    real_workspace_member_only_solve_ring_lock_diff_on_lane(limit, Lane::Interp, None)
}

#[test]
#[ignore = "tier-A measurement probe: real workspace member-only ring 32 interpreter lane"]
fn real_workspace_member_only_solve_ring_lock_diff_32_interp_lane() -> Result<(), String> {
    real_workspace_member_only_solve_ring_lock_diff_on_lane(32, Lane::Interp, Some("interp"))
}

#[test]
#[ignore = "tier-A measurement probe: real workspace member-only ring 32 jit lane"]
fn real_workspace_member_only_solve_ring_lock_diff_32_jit_lane() -> Result<(), String> {
    real_workspace_member_only_solve_ring_lock_diff_on_lane(32, Lane::Jit, Some("jit"))
}

fn real_workspace_member_only_solve_ring_lock_diff_on_lane(
    limit: i64,
    lane: Lane,
    lane_artifact: Option<&str>,
) -> Result<(), String> {
    let mut timings = Vec::new();
    let metadata = timed_ring_step(
        &mut timings,
        "cargo_metadata_real_workspace",
        cargo_metadata_real_workspace,
    )?;
    let mut machine = timed_ring_step(&mut timings, "manifest_machine_with_lane", || {
        manifest_machine_with_lane(lane)
    })?;
    let workspace = timed_ring_step(&mut timings, "real_workspace_manifest_tree", || {
        let tree = real_workspace_manifest_tree(&metadata)?;
        intern_tree(&mut machine, tree)
    })?;
    let root = intern_path(&mut machine, "")?;
    let target = intern_string(&mut machine, "x86_64-apple-darwin")?;

    let selected = timed_ring_step(&mut timings, "solve_demand", || {
        machine.demand_i64(
            "workspace_member_only_solve_selected_versions_text_limit",
            vec![workspace, root, target, limit],
        )
    })?;
    let selected = timed_ring_step(&mut timings, "render_selected_versions", || {
        rendered_string(
            &machine,
            "workspace_member_only_solve_selected_versions_text_limit",
            selected,
        )
    })?;
    let solve_rows = timed_ring_step(&mut timings, "parse_solve_rows", || {
        package_versions_from_solve_text(&selected)
    })?;
    let lock_rows = timed_ring_step(&mut timings, "read_lock_rows", || {
        cargo_lock_package_rows(&workspace_root().join("Cargo.lock"))
    })?;
    let metadata_rows = timed_ring_step(&mut timings, "metadata_package_rows", || {
        Ok(metadata.package_rows())
    })?;
    let diff = timed_ring_step(&mut timings, "diff_package_versions", || {
        Ok(diff_package_versions_against_lock(
            &solve_rows,
            &lock_rows,
            &metadata_rows,
        ))
    })?;

    assert_eq!(diff.solve_rows, limit as usize + 1, "{diff:#?}");
    assert_eq!(diff.matches, limit as usize, "{diff:#?}");
    assert_eq!(diff.solve_only, 1, "{diff:#?}");
    assert_eq!(
        diff.solve_only_categories.get("workspace-pseudo-root"),
        Some(&1),
        "{diff:#?}"
    );
    assert_eq!(diff.lock_only + diff.matches, diff.lock_rows, "{diff:#?}");

    let artifact_prefix = match lane_artifact {
        Some(lane) => format!("real-ring-{limit}-{lane}"),
        None => format!("real-ring-{limit}"),
    };
    write_tier_a_artifact(
        &format!("{artifact_prefix}-solve-vs-lock-summary.tsv"),
        &package_diff_summary_table(&diff),
    )?;
    write_tier_a_artifact(
        &format!("{artifact_prefix}-solve-vs-lock-solve-rows.tsv"),
        &package_rows_table(&solve_rows),
    )?;
    write_tier_a_artifact(
        &format!("{artifact_prefix}-timings.tsv"),
        &ring_timings_table(&timings),
    )?;
    Ok(())
}

macro_rules! real_workspace_member_direct_sparse_solve_ring_lock_diff_test {
    ($name:ident, $limit:expr) => {
        #[test]
        #[ignore = "tier-A measurement probe: real workspace member + direct sparse dep solve ring lock diff"]
        fn $name() -> Result<(), String> {
            real_workspace_member_direct_sparse_solve_ring_lock_diff($limit)
        }
    };
}

real_workspace_member_direct_sparse_solve_ring_lock_diff_test!(
    real_workspace_member_direct_sparse_solve_ring_lock_diff_8,
    8
);
real_workspace_member_direct_sparse_solve_ring_lock_diff_test!(
    real_workspace_member_direct_sparse_solve_ring_lock_diff_16,
    16
);
real_workspace_member_direct_sparse_solve_ring_lock_diff_test!(
    real_workspace_member_direct_sparse_solve_ring_lock_diff_32,
    32
);

macro_rules! real_workspace_member_direct_sparse_native_reference_ring_test {
    ($name:ident, $limit:expr) => {
        #[test]
        #[ignore = "tier-A measurement probe: native Rust solve-only reference over exact Vix direct sparse index"]
        fn $name() -> Result<(), String> {
            let row = real_workspace_member_direct_sparse_native_reference_ring($limit)?;
            write_native_reference_summary(&[row])
        }
    };
}

real_workspace_member_direct_sparse_native_reference_ring_test!(
    real_workspace_member_direct_sparse_native_reference_ring_16,
    16
);
real_workspace_member_direct_sparse_native_reference_ring_test!(
    real_workspace_member_direct_sparse_native_reference_ring_32,
    32
);

#[test]
#[ignore = "tier-A measurement probe: real workspace member + direct sparse dep unit diff"]
fn real_workspace_member_direct_sparse_unit_diff_8() -> Result<(), String> {
    real_workspace_member_direct_sparse_unit_diff(8)
}

macro_rules! real_workspace_member_transitive_sparse_solve_ring_lock_diff_test {
    ($name:ident, $limit:expr) => {
        #[test]
        #[ignore = "tier-A measurement probe: real workspace member + first transitive sparse dep solve ring lock diff"]
        fn $name() -> Result<(), String> {
            real_workspace_member_transitive_sparse_solve_ring_lock_diff($limit)
        }
    };
}

real_workspace_member_transitive_sparse_solve_ring_lock_diff_test!(
    real_workspace_member_transitive_sparse_solve_ring_lock_diff_1,
    1
);
real_workspace_member_transitive_sparse_solve_ring_lock_diff_test!(
    real_workspace_member_transitive_sparse_solve_ring_lock_diff_2,
    2
);
real_workspace_member_transitive_sparse_solve_ring_lock_diff_test!(
    real_workspace_member_transitive_sparse_solve_ring_lock_diff_3,
    3
);
real_workspace_member_transitive_sparse_solve_ring_lock_diff_test!(
    real_workspace_member_transitive_sparse_solve_ring_lock_diff_4,
    4
);
real_workspace_member_transitive_sparse_solve_ring_lock_diff_test!(
    real_workspace_member_transitive_sparse_solve_ring_lock_diff_8,
    8
);

#[test]
#[ignore = "tier-A measurement repro: one pinned sparse row in cargo_manifest.vix"]
fn pinned_sparse_row_parses_in_cargo_manifest_module() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let row = intern_string(
        &mut machine,
        r#"{"name":"blake3","vers":"0.0.0","deps":[],"cksum":"9497a07b1d377f7cd343cd729b12147fdad56935c6e18834e0aa1b412a1bae57","features":{},"yanked":false,"pubtime":"2019-09-17T19:59:37Z"}"#,
    )?;
    let count = machine.demand_i64("workspace_sparse_row_count", vec![row])?;
    assert_eq!(count, 1);
    Ok(())
}

#[test]
fn typed_sparse_row_missing_required_field_reports_offending_row() -> Result<(), String> {
    assert_sparse_row_schema_error(
        r#"{"name":"blake3","deps":[],"features":{},"yanked":false}"#,
        "missing field `vers` for SparseIndexRow",
    )
}

#[test]
fn typed_sparse_row_wrong_field_type_reports_offending_row() -> Result<(), String> {
    assert_sparse_row_schema_error(
        r#"{"name":"blake3","vers":"0.0.0","deps":[],"features":[],"yanked":false}"#,
        "expected Map<String,Array<String>>, got []",
    )
}

#[test]
fn sparse_feature_closure_preserves_hyphenated_seed_feature() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let row = intern_string(&mut machine, TOKIO_1_52_3_SPARSE_ROW)?;
    let target = intern_string(&mut machine, "x86_64-apple-darwin")?;
    let features = machine.demand_i64(
        "sparse_row_rt_multi_thread_feature_debug",
        vec![row, target],
    )?;
    assert_eq!(
        rendered_string(
            &machine,
            "sparse_row_rt_multi_thread_feature_debug",
            features
        )?,
        "rt-multi-thread,rt"
    );
    Ok(())
}

#[test]
fn sparse_feature_closure_preserves_ring8_tokio_seed_features() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let row = intern_string(&mut machine, TOKIO_1_52_3_SPARSE_ROW)?;
    let target = intern_string(&mut machine, "x86_64-apple-darwin")?;
    let features = machine.demand_i64("sparse_row_tokio_ring8_feature_debug", vec![row, target])?;
    let features = rendered_string(&machine, "sparse_row_tokio_ring8_feature_debug", features)?;
    assert!(
        features
            .split(',')
            .any(|feature| feature == "rt-multi-thread"),
        "{features}"
    );
    assert!(
        !features.split(',').any(|feature| feature == "windows-sys"),
        "{features}"
    );
    Ok(())
}

fn real_workspace_member_direct_sparse_solve_ring_lock_diff(limit: i64) -> Result<(), String> {
    let metadata = cargo_metadata_real_workspace()?;
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, real_workspace_manifest_tree(&metadata)?)?;
    let root = intern_path(&mut machine, "")?;
    let target = intern_string(&mut machine, "x86_64-apple-darwin")?;
    let mut timings = Vec::new();
    let started = Instant::now();
    let sparse_jsonl_text =
        direct_sparse_snapshot_jsonl_for_ring(&mut machine, &metadata, workspace, limit)?;
    timings.push(("sparse_snapshot", started.elapsed()));
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-timings.tsv"),
        &ring_timings_table(&timings),
    )?;
    if let Some(tokio_row) = sparse_jsonl_text
        .lines()
        .find(|line| line.contains(r#""name":"tokio""#) && line.contains(r#""vers":"1.52.3""#))
    {
        write_tier_a_artifact(
            &format!("real-direct-ring-{limit}-tokio-row.json"),
            tokio_row,
        )?;
    }
    let sparse_jsonl = intern_string(&mut machine, &sparse_jsonl_text)?;
    let started = Instant::now();
    let sparse_row_count = machine.demand_i64("workspace_sparse_row_count", vec![sparse_jsonl])?;
    timings.push(("typed_sparse_row_count", started.elapsed()));
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-timings.tsv"),
        &ring_timings_table(&timings),
    )?;
    let started = Instant::now();
    let package_count = machine.demand_i64(
        "workspace_member_direct_sparse_index_package_count_limit",
        vec![workspace, root, sparse_jsonl, limit],
    )?;
    let clause_count = machine.demand_i64(
        "workspace_member_direct_sparse_index_clause_count_limit",
        vec![workspace, root, sparse_jsonl, limit],
    )?;
    let tokio = intern_string(&mut machine, "tokio")?;
    let tokio_req_text = machine.demand_i64(
        "workspace_member_direct_sparse_dep_req_text_limit",
        vec![workspace, root, sparse_jsonl, limit, tokio],
    )?;
    let tokio_req_text = rendered_string(
        &machine,
        "workspace_member_direct_sparse_dep_req_text_limit",
        tokio_req_text,
    )?;
    let tokio_candidate_text = machine.demand_i64(
        "workspace_member_direct_sparse_dep_candidate_text_limit",
        vec![workspace, root, sparse_jsonl, limit, tokio],
    )?;
    let tokio_candidate_text = rendered_string(
        &machine,
        "workspace_member_direct_sparse_dep_candidate_text_limit",
        tokio_candidate_text,
    )?;
    timings.push(("typed_sparse_index_and_debug", started.elapsed()));
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-timings.tsv"),
        &ring_timings_table(&timings),
    )?;
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-index-counts.tsv"),
        &format!(
            "metric\tcount\nsparse_rows\t{sparse_row_count}\npackages\t{package_count}\nclauses\t{clause_count}\n"
        ),
    )?;
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-tokio-narrowing.tsv"),
        &format!("metric\tvalue\ntokio_emitted_req\t{tokio_req_text}\n"),
    )?;
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-tokio-candidates.txt"),
        &tokio_candidate_text,
    )?;
    assert_all_req_lines(&tokio_req_text, "1", &format!("direct sparse ring {limit}"))?;

    let started = Instant::now();
    let selected = machine.demand_i64(
        "workspace_member_direct_sparse_solve_selected_versions_text_limit",
        vec![workspace, root, sparse_jsonl, target, limit],
    )?;
    let selected = rendered_string(
        &machine,
        "workspace_member_direct_sparse_solve_selected_versions_text_limit",
        selected,
    )?;
    let solve_rows = package_versions_from_solve_text(&selected)?;
    let lock_rows = cargo_lock_package_rows(&workspace_root().join("Cargo.lock"))?;
    let metadata_rows = metadata.package_rows();
    let diff = diff_package_versions_against_lock(&solve_rows, &lock_rows, &metadata_rows);
    timings.push(("solve_and_lock_diff", started.elapsed()));
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-timings.tsv"),
        &ring_timings_table(&timings),
    )?;

    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-solve-vs-lock-summary.tsv"),
        &package_diff_summary_table(&diff),
    )?;
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-solve-vs-lock-solve-rows.tsv"),
        &package_rows_table(&solve_rows),
    )?;
    assert!(
        diff.solve_rows > limit as usize,
        "direct sparse ring {limit} produced no member-root solve: {diff:#?}"
    );
    Ok(())
}

fn real_workspace_member_direct_sparse_unit_diff(limit: i64) -> Result<(), String> {
    let metadata = cargo_metadata_real_workspace()?;
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, real_workspace_manifest_tree(&metadata)?)?;
    let root = intern_path(&mut machine, "")?;
    let target = intern_string(&mut machine, "x86_64-apple-darwin")?;
    let sparse_jsonl_text =
        direct_sparse_snapshot_jsonl_for_ring(&mut machine, &metadata, workspace, limit)?;
    if let Some(tokio_row) = sparse_jsonl_text
        .lines()
        .find(|line| line.contains(r#""name":"tokio""#) && line.contains(r#""vers":"1.52.3""#))
    {
        write_tier_a_artifact(
            &format!("real-direct-ring-{limit}-unit-tokio-row.json"),
            tokio_row,
        )?;
    }
    let sparse_jsonl = intern_string(&mut machine, &sparse_jsonl_text)?;
    let tokio = intern_string(&mut machine, "tokio")?;
    let windows_sys = intern_string(&mut machine, "windows-sys")?;
    let tokio_enabled_features = machine.demand_i64(
        "workspace_member_direct_sparse_enabled_features_text_limit",
        vec![workspace, root, sparse_jsonl, target, limit, tokio],
    )?;
    let tokio_enabled_features = rendered_string(
        &machine,
        "workspace_member_direct_sparse_enabled_features_text_limit",
        tokio_enabled_features,
    )?;
    let tokio_windows_target = machine.demand_i64(
        "workspace_member_direct_sparse_feature_target_debug_limit",
        vec![
            workspace,
            root,
            sparse_jsonl,
            target,
            limit,
            tokio,
            windows_sys,
        ],
    )?;
    let tokio_windows_target = rendered_string(
        &machine,
        "workspace_member_direct_sparse_feature_target_debug_limit",
        tokio_windows_target,
    )?;

    let units = machine.demand_i64(
        "workspace_member_direct_sparse_solution_units_text_limit",
        vec![workspace, root, sparse_jsonl, target, limit],
    )?;
    let units = rendered_string(
        &machine,
        "workspace_member_direct_sparse_solution_units_text_limit",
        units,
    )?;
    let vix_units = ring_units_from_vix_text(&units)?;
    let selected_rows = vix_units
        .iter()
        .map(|unit| PackageVersion::new(&unit.package, &unit.version))
        .collect::<BTreeSet<_>>();
    let cargo_units = cargo_ring_unit_shapes(&metadata, &selected_rows)?;
    let diff = diff_ring_units(&vix_units, &cargo_units);

    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-unit-diff-summary.tsv"),
        &ring_unit_diff_summary_table(&diff),
    )?;
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-vix-units.tsv"),
        &ring_units_table(&vix_units),
    )?;
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-cargo-units.tsv"),
        &ring_units_table(&cargo_units),
    )?;
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-unit-divergence-categories.tsv"),
        &ring_unit_categories_table(&diff),
    )?;
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-tokio-enabled-features.txt"),
        &tokio_enabled_features,
    )?;
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-tokio-windows-sys-target.txt"),
        &tokio_windows_target,
    )?;

    assert!(!vix_units.is_empty(), "no Vix units derived");
    assert!(!cargo_units.is_empty(), "no Cargo units selected");
    assert_eq!(
        diff.unknown_divergences(),
        0,
        "uncategorized unit divergences remain: {diff:#?}"
    );
    Ok(())
}

fn real_workspace_member_direct_sparse_native_reference_ring(
    limit: i64,
) -> Result<NativeReferenceRingRow, String> {
    let metadata = cargo_metadata_real_workspace()?;
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, real_workspace_manifest_tree(&metadata)?)?;
    let root = intern_path(&mut machine, "")?;
    let sparse_jsonl_text =
        direct_sparse_snapshot_jsonl_for_ring(&mut machine, &metadata, workspace, limit)?;
    let sparse_jsonl = intern_string(&mut machine, &sparse_jsonl_text)?;
    let sparse_row_count = machine.demand_i64("workspace_sparse_row_count", vec![sparse_jsonl])?;
    let index_dump = machine.demand_i64(
        "workspace_member_direct_sparse_index_dump_limit",
        vec![workspace, root, sparse_jsonl, limit],
    )?;
    let index_dump = rendered_string(
        &machine,
        "workspace_member_direct_sparse_index_dump_limit",
        index_dump,
    )?;
    let native_index = NativeResolveIndex::from_dump(&index_dump)?;
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-native-reference-index.tsv"),
        &index_dump,
    )?;

    let repeats = native_reference_repeats();
    let native_started = Instant::now();
    let mut native_rows = BTreeSet::new();
    for _ in 0..repeats {
        native_rows = std::hint::black_box(native_index.solve_rows()?);
    }
    let native_total = native_started.elapsed();

    let lock_rows = cargo_lock_package_rows(&workspace_root().join("Cargo.lock"))?;
    let metadata_rows = metadata.package_rows();
    let diff = diff_package_versions_against_lock(&native_rows, &lock_rows, &metadata_rows);
    write_tier_a_artifact(
        &format!("real-direct-ring-{limit}-native-reference-solve-rows.tsv"),
        &package_rows_table(&native_rows),
    )?;

    Ok(NativeReferenceRingRow {
        ring: limit,
        sparse_rows: sparse_row_count as usize,
        packages: native_index.package_count(),
        clauses: native_index.clause_count(),
        solve_rows: diff.solve_rows,
        matches: diff.matches,
        version_skew_names: diff.version_skew_names,
        native_total,
        native_repeats: repeats,
    })
}

fn assert_all_req_lines(actual: &str, expected: &str, context: &str) -> Result<(), String> {
    let lines = actual
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return Err(format!("{context}: no emitted req lines"));
    }
    for line in lines {
        if line != expected {
            return Err(format!(
                "{context}: expected every emitted req to be {expected:?}, got {line:?} in {actual:?}"
            ));
        }
    }
    Ok(())
}

fn real_workspace_member_transitive_sparse_solve_ring_lock_diff(limit: i64) -> Result<(), String> {
    let metadata = cargo_metadata_real_workspace()?;
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, real_workspace_manifest_tree(&metadata)?)?;
    let root = intern_path(&mut machine, "")?;
    let target = intern_string(&mut machine, "x86_64-apple-darwin")?;
    let sparse_jsonl =
        transitive_sparse_snapshot_jsonl_for_ring(&mut machine, &metadata, workspace, limit)?;
    let sparse_jsonl = intern_string(&mut machine, &sparse_jsonl)?;
    let sparse_row_count = machine.demand_i64("workspace_sparse_row_count", vec![sparse_jsonl])?;
    let package_count = machine.demand_i64(
        "workspace_member_transitive_sparse_index_package_count_limit",
        vec![workspace, root, sparse_jsonl, limit],
    )?;
    let clause_count = machine.demand_i64(
        "workspace_member_transitive_sparse_index_clause_count_limit",
        vec![workspace, root, sparse_jsonl, limit],
    )?;

    write_tier_a_artifact(
        &format!("real-transitive-ring-{limit}-index-counts.tsv"),
        &format!(
            "metric\tcount\nsparse_rows\t{sparse_row_count}\npackages\t{package_count}\nclauses\t{clause_count}\n"
        ),
    )?;

    let selected = machine.demand_i64(
        "workspace_member_transitive_sparse_solve_selected_versions_text_limit",
        vec![workspace, root, sparse_jsonl, target, limit],
    )?;
    let selected = rendered_string(
        &machine,
        "workspace_member_transitive_sparse_solve_selected_versions_text_limit",
        selected,
    )?;
    let solve_rows = package_versions_from_solve_text(&selected)?;
    let lock_rows = cargo_lock_package_rows(&workspace_root().join("Cargo.lock"))?;
    let metadata_rows = metadata.package_rows();
    let diff = diff_package_versions_against_lock(&solve_rows, &lock_rows, &metadata_rows);

    write_tier_a_artifact(
        &format!("real-transitive-ring-{limit}-solve-vs-lock-summary.tsv"),
        &package_diff_summary_table(&diff),
    )?;
    write_tier_a_artifact(
        &format!("real-transitive-ring-{limit}-solve-vs-lock-solve-rows.tsv"),
        &package_rows_table(&solve_rows),
    )?;
    assert!(
        diff.solve_rows > limit as usize,
        "transitive sparse ring {limit} produced no member-root solve: {diff:#?}"
    );
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

    assert_eq!(package_count, 147);
    assert!(
        clause_count > 292,
        "default feature root clauses should extend the old 2-per-member baseline: {clause_count}"
    );
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

    assert!(
        clause_count > direct_clause_count,
        "clause_count={clause_count}"
    );
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

    assert_eq!(selected_member_count, 146);
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
    manifest_machine_with_lane(Lane::Interp)
}

fn manifest_machine_with_lane(lane: Lane) -> Result<Machine, String> {
    Machine::load_with_lane(&format!("{RODIN_SOURCE}\n\n{SOURCE}"), lane)
}

fn assert_sparse_row_schema_error(row: &str, expected_fragment: &str) -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let row_arg = intern_string(&mut machine, row)?;
    match machine.demand_i64("workspace_sparse_row_count", vec![row_arg]) {
        Ok(count) => Err(format!(
            "typed sparse row schema mismatch was silently accepted as count {count}"
        )),
        Err(err) => {
            for fragment in [
                "typed Json parse into SparseIndexRow failed",
                expected_fragment,
                "offending input:",
                row,
            ] {
                if !err.contains(fragment) {
                    return Err(format!(
                        "schema error did not include `{fragment}`\nerror: {err}"
                    ));
                }
            }
            Ok(())
        }
    }
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

fn field_strings(
    fields: &BTreeMap<String, RenderedValue>,
    name: &str,
) -> Result<Vec<String>, String> {
    match fields.get(name) {
        Some(RenderedValue::Array { items, .. }) => items
            .iter()
            .map(|item| match item {
                RenderedValue::String { value } => Ok(value.clone()),
                other => Err(format!("field {name} item was {other:?}, not String")),
            })
            .collect(),
        other => Err(format!("field {name} was {other:?}, not Array")),
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
    static METADATA: OnceLock<Result<CargoMetadata, String>> = OnceLock::new();
    METADATA
        .get_or_init(load_cargo_metadata_real_workspace)
        .clone()
}

fn load_cargo_metadata_real_workspace() -> Result<CargoMetadata, String> {
    if let Ok(path) = std::env::var("TIER_A_CARGO_METADATA") {
        let text = std::fs::read_to_string(&path)
            .map_err(|err| format!("read TIER_A_CARGO_METADATA at {path}: {err}"))?;
        return facet_json::from_str(&text).map_err(|err| err.to_string());
    }
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

fn direct_sparse_snapshot_jsonl_for_ring(
    machine: &mut Machine,
    metadata: &CargoMetadata,
    workspace: i64,
    limit: i64,
) -> Result<String, String> {
    let crate_names = direct_sparse_crate_names_for_ring(machine, metadata, workspace, limit)?;
    sparse_snapshot_jsonl_for_crates(
        crate_names,
        &format!("real-direct-ring-{limit}-sparse-input-crates.tsv"),
    )
}

fn transitive_sparse_snapshot_jsonl_for_ring(
    machine: &mut Machine,
    metadata: &CargoMetadata,
    workspace: i64,
    limit: i64,
) -> Result<String, String> {
    let direct_crates = direct_sparse_crate_names_for_ring(machine, metadata, workspace, limit)?;
    let mut crate_names = direct_crates.clone();
    for name in direct_crates {
        for dependency in sparse_snapshot_dependencies_for_crate(&name)? {
            crate_names.insert(dependency.package_name());
        }
    }
    sparse_snapshot_jsonl_for_crates(
        crate_names,
        &format!("real-transitive-ring-{limit}-sparse-input-crates.tsv"),
    )
}

fn direct_sparse_crate_names_for_ring(
    machine: &mut Machine,
    metadata: &CargoMetadata,
    workspace: i64,
    limit: i64,
) -> Result<BTreeSet<String>, String> {
    let members = machine.demand_i64("workspace_members_text", vec![workspace])?;
    let members = rendered_string(machine, "workspace_members_text", members)?;
    let selected_members = members
        .lines()
        .take(limit as usize)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let root = workspace_root();
    let workspace_members: BTreeSet<_> = metadata.workspace_members.iter().collect();
    let mut crate_names = BTreeSet::new();

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
        let member = relative
            .parent()
            .ok_or_else(|| format!("manifest path had no parent: {}", relative.display()))?
            .to_str()
            .ok_or_else(|| format!("manifest path was not utf-8: {}", relative.display()))?;
        if !selected_members.contains(member) {
            continue;
        }
        for dependency in &package.dependencies {
            crate_names.insert(dependency.name.clone());
        }
    }
    Ok(crate_names)
}

fn sparse_snapshot_jsonl_for_crates(
    crate_names: BTreeSet<String>,
    input_crates_artifact: &str,
) -> Result<String, String> {
    let sparse_root = tier_a_sparse_snapshot_root();
    let mut rows = String::new();
    let mut input_crates = BTreeSet::new();
    let only_crate = std::env::var("TIER_A_DIRECT_SPARSE_ONLY").ok();
    let max_lines = std::env::var("TIER_A_DIRECT_SPARSE_MAX_LINES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok());
    for name in crate_names {
        if only_crate.as_deref().is_some_and(|only| only != name) {
            continue;
        }
        let path = sparse_root
            .join("index")
            .join(sparse_index_path_for_crate(&name));
        if !path.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|err| format!("read sparse rows for {name} at {}: {err}", path.display()))?;
        if !rows.is_empty() && !rows.ends_with('\n') {
            rows.push('\n');
        }
        for line in text.lines().take(max_lines.unwrap_or(usize::MAX)) {
            let row: SparseIndexEntry = facet_json::from_str(line)
                .map_err(|err| format!("parse sparse row for {name}: {err}"))?;
            let row = SparseIndexRowForVix::from(row);
            let line = facet_json::to_string(&row)
                .map_err(|err| format!("serialize sparse row for {name}: {err}"))?;
            rows.push_str(&line);
            rows.push('\n');
        }
        if !rows.ends_with('\n') {
            rows.push('\n');
        }
        input_crates.insert(PackageVersion::new(&name, "sparse-index"));
    }

    write_tier_a_artifact(input_crates_artifact, &package_rows_table(&input_crates))?;
    Ok(rows)
}

fn sparse_snapshot_dependencies_for_crate(
    name: &str,
) -> Result<BTreeSet<SparseIndexDependency>, String> {
    let sparse_root = tier_a_sparse_snapshot_root();
    let path = sparse_root
        .join("index")
        .join(sparse_index_path_for_crate(name));
    if !path.exists() {
        return Ok(BTreeSet::new());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|err| format!("read sparse rows for {name} at {}: {err}", path.display()))?;
    let max_lines = std::env::var("TIER_A_DIRECT_SPARSE_MAX_LINES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok());
    let mut deps = BTreeSet::new();
    for line in text.lines().take(max_lines.unwrap_or(usize::MAX)) {
        let row: SparseIndexEntry = facet_json::from_str(line)
            .map_err(|err| format!("parse sparse row for {name}: {err}"))?;
        if row.yanked {
            continue;
        }
        deps.extend(row.deps);
    }
    Ok(deps)
}

fn tier_a_sparse_snapshot_root() -> std::path::PathBuf {
    std::env::var("TIER_A_SPARSE_OUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/tier-a-scale-measurement/sparse-index"))
}

fn sparse_index_path_for_crate(name: &str) -> std::path::PathBuf {
    let len = name.len();
    if len == 1 {
        ["1", name].iter().collect()
    } else if len == 2 {
        ["2", name].iter().collect()
    } else if len == 3 {
        ["3", &name[..1], name].iter().collect()
    } else {
        [&name[..2], &name[2..4], name].iter().collect()
    }
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

#[derive(Clone, Debug, Facet)]
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

#[derive(Clone, Debug, Facet)]
struct CargoPackage {
    id: String,
    name: String,
    version: String,
    edition: String,
    manifest_path: String,
    source: Option<String>,
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

#[derive(Clone)]
struct NativeResolveIndex {
    packages: Vec<usize>,
    names: Vec<String>,
    versions: Vec<NativeVersionRow>,
    clauses: Vec<NativeClause>,
    feature_clauses: usize,
    selected_guard_pkgs: BTreeSet<usize>,
}

#[derive(Clone)]
struct NativeVersionRow {
    pkg: usize,
    version: SemverVersion,
}

#[derive(Clone)]
struct NativeClause {
    parent_pkg: usize,
    parent_version: SemverVersion,
    consequent: NativeConsequent,
    kind: String,
}

#[derive(Clone)]
enum NativeConsequent {
    Activate { pkg: usize },
    Require { pkg: usize, req: Arc<VersionReq> },
}

#[derive(Clone, Default)]
struct NativeDomain {
    active: bool,
    reqs: Vec<Arc<VersionReq>>,
    selected: Option<SemverVersion>,
}

#[derive(Clone)]
struct NativeResolveState {
    domains: Vec<NativeDomain>,
}

struct NativeReferenceRingRow {
    ring: i64,
    sparse_rows: usize,
    packages: usize,
    clauses: usize,
    solve_rows: usize,
    matches: usize,
    version_skew_names: usize,
    native_total: Duration,
    native_repeats: usize,
}

impl NativeResolveIndex {
    fn from_dump(text: &str) -> Result<Self, String> {
        let mut index = Self {
            packages: Vec::new(),
            names: Vec::new(),
            versions: Vec::new(),
            clauses: Vec::new(),
            feature_clauses: 0,
            selected_guard_pkgs: BTreeSet::new(),
        };
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let parts = line.split('\t').collect::<Vec<_>>();
            match parts.as_slice() {
                ["p", name] => {
                    index.register_package_name(name);
                }
                ["v", name, version] => {
                    index.register_package(name, version)?;
                }
                [
                    "c",
                    parent_name,
                    parent_version,
                    tag,
                    dep_name,
                    req_text,
                    kind,
                ] => {
                    let parent_pkg = index.package_id(parent_name).ok_or_else(|| {
                        format!("native index clause referenced unknown parent {parent_name:?}")
                    })?;
                    let dep_pkg = index.package_id(dep_name).ok_or_else(|| {
                        format!("native index clause referenced unknown dep {dep_name:?}")
                    })?;
                    if *tag == "feature" {
                        index.feature_clauses += 1;
                        index.selected_guard_pkgs.insert(parent_pkg);
                        continue;
                    }
                    let consequent = match *tag {
                        "in_graph" => NativeConsequent::Activate { pkg: dep_pkg },
                        "version_set" => NativeConsequent::Require {
                            pkg: dep_pkg,
                            req: Arc::new(parse_req(req_text)?),
                        },
                        other => {
                            return Err(format!(
                                "native index has unsupported consequent tag {other:?}"
                            ));
                        }
                    };
                    index.add_selected_guard_clause(
                        parent_pkg,
                        parent_version,
                        consequent,
                        kind,
                    )?;
                }
                _ => return Err(format!("bad native index line {line:?}")),
            }
        }
        Ok(index)
    }

    fn register_package_name(&mut self, name: &str) -> usize {
        if let Some(pkg) = self.package_id(name) {
            return pkg;
        }
        let pkg = self.packages.len();
        self.packages.push(pkg);
        self.names.push(name.to_owned());
        pkg
    }

    fn register_package(&mut self, name: &str, version: &str) -> Result<usize, String> {
        let pkg = self.register_package_name(name);
        self.versions.push(NativeVersionRow {
            pkg,
            version: parse_version(version)?,
        });
        Ok(pkg)
    }

    fn add_selected_guard_clause(
        &mut self,
        parent_pkg: usize,
        parent_version: &str,
        consequent: NativeConsequent,
        kind: &str,
    ) -> Result<(), String> {
        self.selected_guard_pkgs.insert(parent_pkg);
        self.clauses.push(NativeClause {
            parent_pkg,
            parent_version: parse_version(parent_version)?,
            consequent,
            kind: kind.to_owned(),
        });
        Ok(())
    }

    fn package_id(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|candidate| candidate == name)
    }

    fn package_count(&self) -> usize {
        self.packages.len()
    }

    fn clause_count(&self) -> usize {
        self.clauses.len() + self.feature_clauses
    }

    fn solve_rows(&self) -> Result<BTreeSet<PackageVersion>, String> {
        let mut state = NativeResolveState {
            domains: vec![NativeDomain::default(); self.packages.len()],
        };
        let root = self
            .package_id("__workspace__")
            .ok_or_else(|| "native index did not include __workspace__".to_string())?;
        state.domains[root].active = true;
        let state = self.search(state)?;
        Ok(self
            .packages
            .iter()
            .filter_map(|&pkg| {
                state.domains[pkg].selected.as_ref().map(|version| {
                    PackageVersion::new(&self.names[pkg], &selected_version_text(version))
                })
            })
            .collect())
    }

    fn search(&self, mut state: NativeResolveState) -> Result<NativeResolveState, String> {
        self.propagate(&mut state)?;
        let Some(pkg) = self.next_undecided(&state) else {
            return Ok(state);
        };
        let mut candidates = self.candidates(pkg, &state.domains[pkg]);
        let mut last_error = None;
        while let Some(version) = candidates.pop() {
            let mut branch = state.clone();
            branch.domains[pkg].active = true;
            branch.domains[pkg].selected = Some(version);
            match self.search(branch) {
                Ok(solution) => return Ok(solution),
                Err(err) => last_error = Some(err),
            }
        }
        let cause = last_error
            .map(|err| format!("; last branch error: {err}"))
            .unwrap_or_default();
        Err(format!(
            "native solve exhausted candidates for {}{cause}",
            self.names[pkg]
        ))
    }

    fn propagate(&self, state: &mut NativeResolveState) -> Result<(), String> {
        loop {
            let mut changed = self.force_singletons(state)?;
            for clause in self.clauses.iter().filter(|clause| clause.kind != "dev") {
                if !self.selected_matches(state, clause.parent_pkg, &clause.parent_version) {
                    continue;
                }
                match &clause.consequent {
                    NativeConsequent::Activate { pkg } => {
                        let domain = &mut state.domains[*pkg];
                        if !domain.active {
                            domain.active = true;
                            changed = true;
                        }
                    }
                    NativeConsequent::Require { pkg, req } => {
                        let domain = &mut state.domains[*pkg];
                        if let Some(selected) = &domain.selected
                            && !native_req_matches(req, selected)
                        {
                            return Err(format!(
                                "native selected {} {} violates {}",
                                self.names[*pkg], selected, req
                            ));
                        }
                        domain.active = true;
                        if !domain.reqs.iter().any(|existing| existing == req) {
                            domain.reqs.push(req.clone());
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                return Ok(());
            }
        }
    }

    fn force_singletons(&self, state: &mut NativeResolveState) -> Result<bool, String> {
        let mut changed = false;
        for &pkg in &self.packages {
            let domain = &state.domains[pkg];
            if !domain.active || domain.selected.is_some() {
                continue;
            }
            let candidates = self.candidates(pkg, domain);
            if candidates.is_empty() {
                return Err(format!(
                    "native package {} has no candidates",
                    self.names[pkg]
                ));
            }
            if candidates.len() == 1 && !self.selected_guard_pkgs.contains(&pkg) {
                state.domains[pkg].selected = candidates.first().cloned();
                changed = true;
            }
        }
        Ok(changed)
    }

    fn selected_matches(
        &self,
        state: &NativeResolveState,
        pkg: usize,
        version: &SemverVersion,
    ) -> bool {
        state.domains[pkg]
            .selected
            .as_ref()
            .is_some_and(|selected| selected == version)
    }

    fn next_undecided(&self, state: &NativeResolveState) -> Option<usize> {
        self.packages.iter().copied().find(|&pkg| {
            let domain = &state.domains[pkg];
            domain.active && domain.selected.is_none()
        })
    }

    fn candidates(&self, pkg: usize, domain: &NativeDomain) -> Vec<SemverVersion> {
        let mut candidates = self
            .versions
            .iter()
            .filter(|row| row.pkg == pkg)
            .filter(|row| {
                domain
                    .reqs
                    .iter()
                    .all(|req| native_req_matches(req, &row.version))
            })
            .map(|row| row.version.clone())
            .collect::<Vec<_>>();
        candidates.sort();
        candidates
    }
}

fn parse_req(text: &str) -> Result<VersionReq, String> {
    VersionReq::parse(text).map_err(|err| format!("parse native VersionReq {text:?}: {err}"))
}

fn parse_version(text: &str) -> Result<SemverVersion, String> {
    SemverVersion::parse(text).map_err(|err| format!("parse native Version {text:?}: {err}"))
}

fn native_req_matches(req: &VersionReq, version: &SemverVersion) -> bool {
    req.to_string() == "*" || req.matches(version)
}

fn selected_version_text(version: &SemverVersion) -> String {
    version.to_string()
}

fn native_reference_repeats() -> usize {
    std::env::var("TIER_A_NATIVE_REFERENCE_REPEATS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&value| value > 0)
        .unwrap_or(20)
}

fn write_native_reference_summary(rows: &[NativeReferenceRingRow]) -> Result<(), String> {
    let mut lines = vec![
        "ring\tsparse_rows\tpackages\tclauses\tsolve_rows\tmatches\tversion_skew_names\tnative_solve_ms\tnative_repeats".to_owned(),
    ];
    lines.extend(rows.iter().map(|row| {
        let native_ms = row.native_total.as_secs_f64() * 1000.0 / row.native_repeats as f64;
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{native_ms:.6}\t{}",
            row.ring,
            row.sparse_rows,
            row.packages,
            row.clauses,
            row.solve_rows,
            row.matches,
            row.version_skew_names,
            row.native_repeats
        )
    }));
    lines.push(String::new());
    let table = lines.join("\n");
    eprintln!("{table}");
    write_tier_a_artifact("real-direct-native-reference-summary.tsv", &table)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RingUnitShape {
    package: String,
    version: String,
    target_name: String,
    target_kind: String,
    crate_type: String,
    source_suffix: String,
    mode: String,
    platform: String,
    features: Vec<String>,
    profile: RingUnitProfile,
    dependencies: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RingUnitProfile {
    name: String,
    opt_level: String,
    lto: String,
    codegen_backend: String,
    codegen_units: String,
    debuginfo: String,
    split_debuginfo: String,
    debug_assertions: String,
    overflow_checks: String,
    rpath: String,
    incremental: String,
    panic: String,
    strip: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RingUnitExactKey {
    package: String,
    version: String,
    target_name: String,
    target_kind: String,
    crate_type: String,
    source_suffix: String,
    mode: String,
    platform: String,
    features: Vec<String>,
    profile: RingUnitProfile,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RingUnitTargetKey {
    package: String,
    version: String,
    target_name: String,
    target_kind: String,
    crate_type: String,
    source_suffix: String,
    mode: String,
    platform: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RingUnitEdge {
    from: RingUnitTargetKey,
    extern_crate_name: String,
}

#[derive(Debug)]
struct RingUnitDiffSummary {
    vix_units: usize,
    cargo_units: usize,
    exact_unit_matches: usize,
    vix_only_units: usize,
    cargo_only_units: usize,
    target_key_matches: usize,
    vix_edges: usize,
    cargo_edges: usize,
    edge_matches: usize,
    vix_only_edges: usize,
    cargo_only_edges: usize,
    vix_only_categories: BTreeMap<&'static str, usize>,
    cargo_only_categories: BTreeMap<&'static str, usize>,
    vix_only_edge_categories: BTreeMap<&'static str, usize>,
    cargo_only_edge_categories: BTreeMap<&'static str, usize>,
}

impl RingUnitDiffSummary {
    fn unknown_divergences(&self) -> usize {
        self.vix_only_categories
            .get("unknown")
            .copied()
            .unwrap_or_default()
            + self
                .cargo_only_categories
                .get("unknown")
                .copied()
                .unwrap_or_default()
            + self
                .vix_only_edge_categories
                .get("unknown")
                .copied()
                .unwrap_or_default()
            + self
                .cargo_only_edge_categories
                .get("unknown")
                .copied()
                .unwrap_or_default()
    }
}

impl From<&RingUnitShape> for RingUnitExactKey {
    fn from(unit: &RingUnitShape) -> Self {
        Self {
            package: unit.package.clone(),
            version: unit.version.clone(),
            target_name: unit.target_name.clone(),
            target_kind: unit.target_kind.clone(),
            crate_type: unit.crate_type.clone(),
            source_suffix: unit.source_suffix.clone(),
            mode: unit.mode.clone(),
            platform: unit.platform.clone(),
            features: unit.features.clone(),
            profile: unit.profile.clone(),
        }
    }
}

impl From<&RingUnitShape> for RingUnitTargetKey {
    fn from(unit: &RingUnitShape) -> Self {
        Self {
            package: unit.package.clone(),
            version: unit.version.clone(),
            target_name: unit.target_name.clone(),
            target_kind: unit.target_kind.clone(),
            crate_type: unit.crate_type.clone(),
            source_suffix: unit.source_suffix.clone(),
            mode: unit.mode.clone(),
            platform: unit.platform.clone(),
        }
    }
}

fn ring_units_from_vix_text(text: &str) -> Result<Vec<RingUnitShape>, String> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(ring_unit_from_tsv_line)
        .collect()
}

fn ring_unit_from_tsv_line(line: &str) -> Result<RingUnitShape, String> {
    let columns = line.split('\t').collect::<Vec<_>>();
    if columns.len() != 23 {
        return Err(format!(
            "expected 23 unit columns, got {} in {line:?}",
            columns.len()
        ));
    }
    Ok(RingUnitShape {
        package: columns[0].to_owned(),
        version: columns[1].to_owned(),
        target_name: columns[2].to_owned(),
        target_kind: columns[3].to_owned(),
        crate_type: columns[4].to_owned(),
        source_suffix: columns[5].to_owned(),
        mode: columns[6].to_owned(),
        platform: columns[7].to_owned(),
        features: comma_list(columns[8]),
        profile: RingUnitProfile {
            name: columns[9].to_owned(),
            opt_level: columns[10].to_owned(),
            lto: columns[11].to_owned(),
            codegen_backend: columns[12].to_owned(),
            codegen_units: columns[13].to_owned(),
            debuginfo: columns[14].to_owned(),
            split_debuginfo: columns[15].to_owned(),
            debug_assertions: columns[16].to_owned(),
            overflow_checks: columns[17].to_owned(),
            rpath: columns[18].to_owned(),
            incremental: columns[19].to_owned(),
            panic: columns[20].to_owned(),
            strip: columns[21].to_owned(),
        },
        dependencies: comma_list(columns[22]),
    })
}

fn comma_list(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        let mut values = value
            .split(',')
            .filter(|item| !item.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        values.sort();
        values.dedup();
        values
    }
}

fn cargo_ring_unit_shapes(
    metadata: &CargoMetadata,
    selected_rows: &BTreeSet<PackageVersion>,
) -> Result<Vec<RingUnitShape>, String> {
    // Do not build a full-workspace unit graph and filter it here: Cargo has
    // already applied workspace-wide feature unification by then. Ring probes
    // must root Cargo with exactly the selected workspace members.
    let graph = cargo_unit_graph_for_ring(metadata, selected_rows)?;
    let package_by_id = metadata
        .packages
        .iter()
        .map(|package| {
            (
                package.id.clone(),
                PackageVersion::new(&package.name, &package.version),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut shapes = Vec::new();
    for unit in &graph.units {
        let Some(package) = package_by_id.get(&unit.pkg_id) else {
            continue;
        };
        if !selected_rows.contains(package) {
            continue;
        }
        let dependencies = unit
            .dependencies
            .iter()
            .filter_map(|dependency| dependency.extern_crate_name.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        shapes.push(RingUnitShape {
            package: package.name.clone(),
            version: package.version.clone(),
            target_name: unit.target.name.replace('-', "_"),
            target_kind: unit.target.normalized_unit_kind()?,
            crate_type: unit
                .target
                .crate_types
                .first()
                .cloned()
                .ok_or_else(|| format!("unit target {} had no crate types", unit.target.name))?,
            source_suffix: cargo_unit_source_suffix(&unit.target.src_path)?,
            mode: unit.mode.clone(),
            platform: unit.platform.clone().unwrap_or_default(),
            features: sorted_strings(unit.features.clone()),
            profile: RingUnitProfile::from(&unit.profile),
            dependencies,
        });
    }
    shapes.sort();
    shapes.dedup();
    Ok(shapes)
}

fn cargo_unit_graph_for_ring(
    metadata: &CargoMetadata,
    selected_rows: &BTreeSet<PackageVersion>,
) -> Result<CargoUnitGraphForDiff, String> {
    let text = if let Ok(path) = std::env::var("TIER_A_RING_UNIT_GRAPH") {
        std::fs::read_to_string(&path)
            .map_err(|err| format!("read TIER_A_RING_UNIT_GRAPH at {path}: {err}"))?
    } else {
        cargo_unit_graph_ring_stdout(metadata, selected_rows)?
    };
    facet_json::from_str(&text).map_err(|err| err.to_string())
}

fn cargo_unit_graph_ring_stdout(
    metadata: &CargoMetadata,
    selected_rows: &BTreeSet<PackageVersion>,
) -> Result<String, String> {
    let root_packages = cargo_ring_root_packages(metadata, selected_rows);
    if root_packages.is_empty() {
        return Err("ring unit graph oracle had no workspace root packages".to_owned());
    }
    let mut command = Command::new("cargo");
    command
        .arg("+nightly")
        .arg("build")
        .arg("--unit-graph")
        .arg("-Z")
        .arg("unstable-options")
        .arg("--locked");
    for package in root_packages {
        command.arg("-p").arg(package);
    }
    let output = command
        .current_dir(workspace_root())
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "cargo ring unit graph oracle failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|err| err.to_string())
}

fn cargo_ring_root_packages(
    metadata: &CargoMetadata,
    selected_rows: &BTreeSet<PackageVersion>,
) -> Vec<String> {
    let workspace_members = metadata
        .workspace_members
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    metadata
        .packages
        .iter()
        .filter(|package| package.source.is_none())
        .filter(|package| workspace_members.contains(package.id.as_str()))
        .filter(|package| {
            selected_rows.contains(&PackageVersion::new(&package.name, &package.version))
        })
        .map(|package| package.name.clone())
        .collect()
}

fn sorted_strings(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

fn cargo_unit_source_suffix(source: &str) -> Result<String, String> {
    for marker in ["/src/bin/", "/src/lib.rs", "/src/main.rs", "/build.rs"] {
        if let Some(index) = source.rfind(marker) {
            let suffix = &source[index + 1..];
            return Ok(suffix.to_owned());
        }
    }
    Err(format!("unexpected Cargo unit source path `{source}`"))
}

fn diff_ring_units(vix: &[RingUnitShape], cargo: &[RingUnitShape]) -> RingUnitDiffSummary {
    let vix_exact = ring_unit_exact_keys(vix);
    let cargo_exact = ring_unit_exact_keys(cargo);
    let vix_targets = ring_unit_target_keys(vix);
    let cargo_targets = ring_unit_target_keys(cargo);
    let vix_edges = ring_unit_edges(vix);
    let cargo_edges = ring_unit_edges(cargo);
    let mut vix_only_categories = BTreeMap::new();
    let mut cargo_only_categories = BTreeMap::new();
    let mut vix_only_edge_categories = BTreeMap::new();
    let mut cargo_only_edge_categories = BTreeMap::new();

    for key in vix_exact.difference(&cargo_exact) {
        bump(
            &mut vix_only_categories,
            categorize_unit_key(UnitDiffSide::VixOnly, key, cargo, &cargo_targets),
        );
    }
    for key in cargo_exact.difference(&vix_exact) {
        bump(
            &mut cargo_only_categories,
            categorize_unit_key(UnitDiffSide::CargoOnly, key, vix, &vix_targets),
        );
    }
    for edge in vix_edges.difference(&cargo_edges) {
        bump(
            &mut vix_only_edge_categories,
            categorize_edge(edge, &cargo_targets, "vix-edge-not-in-cargo"),
        );
    }
    for edge in cargo_edges.difference(&vix_edges) {
        bump(
            &mut cargo_only_edge_categories,
            categorize_edge(
                edge,
                &vix_targets,
                "cargo-dependency-edge-outside-vix-selected-closure",
            ),
        );
    }

    RingUnitDiffSummary {
        vix_units: vix_exact.len(),
        cargo_units: cargo_exact.len(),
        exact_unit_matches: vix_exact.intersection(&cargo_exact).count(),
        vix_only_units: vix_exact.difference(&cargo_exact).count(),
        cargo_only_units: cargo_exact.difference(&vix_exact).count(),
        target_key_matches: vix_targets.intersection(&cargo_targets).count(),
        vix_edges: vix_edges.len(),
        cargo_edges: cargo_edges.len(),
        edge_matches: vix_edges.intersection(&cargo_edges).count(),
        vix_only_edges: vix_edges.difference(&cargo_edges).count(),
        cargo_only_edges: cargo_edges.difference(&vix_edges).count(),
        vix_only_categories,
        cargo_only_categories,
        vix_only_edge_categories,
        cargo_only_edge_categories,
    }
}

fn ring_unit_exact_keys(units: &[RingUnitShape]) -> BTreeSet<RingUnitExactKey> {
    units.iter().map(RingUnitExactKey::from).collect()
}

fn ring_unit_target_keys(units: &[RingUnitShape]) -> BTreeSet<RingUnitTargetKey> {
    units.iter().map(RingUnitTargetKey::from).collect()
}

fn ring_unit_edges(units: &[RingUnitShape]) -> BTreeSet<RingUnitEdge> {
    units
        .iter()
        .flat_map(|unit| {
            let from = RingUnitTargetKey::from(unit);
            unit.dependencies
                .iter()
                .cloned()
                .map(move |extern_crate_name| RingUnitEdge {
                    from: from.clone(),
                    extern_crate_name,
                })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnitDiffSide {
    VixOnly,
    CargoOnly,
}

fn categorize_unit_key(
    side: UnitDiffSide,
    key: &RingUnitExactKey,
    other_units: &[RingUnitShape],
    other_targets: &BTreeSet<RingUnitTargetKey>,
) -> &'static str {
    let target = RingUnitTargetKey {
        package: key.package.clone(),
        version: key.version.clone(),
        target_name: key.target_name.clone(),
        target_kind: key.target_kind.clone(),
        crate_type: key.crate_type.clone(),
        source_suffix: key.source_suffix.clone(),
        mode: key.mode.clone(),
        platform: key.platform.clone(),
    };
    if !other_targets.contains(&target) {
        return "target-kind-or-source-gap";
    }
    let same_target = other_units
        .iter()
        .filter(|unit| RingUnitTargetKey::from(*unit) == target)
        .collect::<Vec<_>>();
    let same_profile = same_target.iter().any(|unit| unit.profile == key.profile);
    let same_features = same_target.iter().any(|unit| unit.features == key.features);
    if same_profile && !same_features {
        let current_features = key.features.iter().collect::<BTreeSet<_>>();
        let same_profile_targets = same_target
            .iter()
            .filter(|unit| unit.profile == key.profile)
            .collect::<Vec<_>>();
        let current_is_subset = same_profile_targets.iter().any(|unit| {
            let other_features = unit.features.iter().collect::<BTreeSet<_>>();
            current_features.is_subset(&other_features)
        });
        let current_is_superset = same_profile_targets.iter().any(|unit| {
            let other_features = unit.features.iter().collect::<BTreeSet<_>>();
            current_features.is_superset(&other_features)
        });
        match (side, current_is_subset, current_is_superset) {
            (UnitDiffSide::VixOnly, true, false) => {
                return "vix-feature-subset-vs-cargo-transitive-path-closure";
            }
            (UnitDiffSide::CargoOnly, false, true) => {
                return "cargo-feature-superset-from-transitive-path-closure";
            }
            _ => {}
        }
    }
    match (same_profile, same_features) {
        (true, true) => "duplicate-or-edge-only-unit",
        (true, false) => "feature-set-gap",
        (false, true) => "profile-field-gap",
        (false, false) => "feature-and-profile-gap",
    }
}

fn categorize_edge(
    edge: &RingUnitEdge,
    other_targets: &BTreeSet<RingUnitTargetKey>,
    matched_target_category: &'static str,
) -> &'static str {
    if other_targets.contains(&edge.from) {
        matched_target_category
    } else {
        "edge-source-unit-gap"
    }
}

fn ring_unit_diff_summary_table(diff: &RingUnitDiffSummary) -> String {
    let mut lines = vec![
        "metric\tcount".to_owned(),
        format!("vix_units\t{}", diff.vix_units),
        format!("cargo_units\t{}", diff.cargo_units),
        format!("exact_unit_matches\t{}", diff.exact_unit_matches),
        format!("vix_only_units\t{}", diff.vix_only_units),
        format!("cargo_only_units\t{}", diff.cargo_only_units),
        format!("target_key_matches\t{}", diff.target_key_matches),
        format!("vix_edges\t{}", diff.vix_edges),
        format!("cargo_edges\t{}", diff.cargo_edges),
        format!("edge_matches\t{}", diff.edge_matches),
        format!("vix_only_edges\t{}", diff.vix_only_edges),
        format!("cargo_only_edges\t{}", diff.cargo_only_edges),
    ];
    for (category, count) in &diff.vix_only_categories {
        lines.push(format!("vix_only:{category}\t{count}"));
    }
    for (category, count) in &diff.cargo_only_categories {
        lines.push(format!("cargo_only:{category}\t{count}"));
    }
    for (category, count) in &diff.vix_only_edge_categories {
        lines.push(format!("vix_only_edge:{category}\t{count}"));
    }
    for (category, count) in &diff.cargo_only_edge_categories {
        lines.push(format!("cargo_only_edge:{category}\t{count}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn ring_unit_categories_table(diff: &RingUnitDiffSummary) -> String {
    let mut lines = vec!["side\tcategory\tcount".to_owned()];
    for (category, count) in &diff.vix_only_categories {
        lines.push(format!("vix_only\t{category}\t{count}"));
    }
    for (category, count) in &diff.cargo_only_categories {
        lines.push(format!("cargo_only\t{category}\t{count}"));
    }
    for (category, count) in &diff.vix_only_edge_categories {
        lines.push(format!("vix_only_edge\t{category}\t{count}"));
    }
    for (category, count) in &diff.cargo_only_edge_categories {
        lines.push(format!("cargo_only_edge\t{category}\t{count}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn ring_units_table(units: &[RingUnitShape]) -> String {
    let mut lines = vec![
        "package\tversion\ttarget_name\ttarget_kind\tcrate_type\tsource_suffix\tmode\tplatform\tfeatures\tprofile_name\topt_level\tlto\tcodegen_backend\tcodegen_units\tdebuginfo\tsplit_debuginfo\tdebug_assertions\toverflow_checks\trpath\tincremental\tpanic\tstrip\tdependencies".to_owned(),
    ];
    lines.extend(units.iter().map(|unit| {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            unit.package,
            unit.version,
            unit.target_name,
            unit.target_kind,
            unit.crate_type,
            unit.source_suffix,
            unit.mode,
            unit.platform,
            unit.features.join(","),
            unit.profile.name,
            unit.profile.opt_level,
            unit.profile.lto,
            unit.profile.codegen_backend,
            unit.profile.codegen_units,
            unit.profile.debuginfo,
            unit.profile.split_debuginfo,
            unit.profile.debug_assertions,
            unit.profile.overflow_checks,
            unit.profile.rpath,
            unit.profile.incremental,
            unit.profile.panic,
            unit.profile.strip,
            unit.dependencies.join(","),
        )
    }));
    lines.push(String::new());
    lines.join("\n")
}

fn timed_ring_step<T>(
    timings: &mut Vec<(&'static str, Duration)>,
    label: &'static str,
    step: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let start = Instant::now();
    let result = step();
    timings.push((label, start.elapsed()));
    result
}

fn ring_timings_table(timings: &[(&'static str, Duration)]) -> String {
    let mut lines = vec!["step\twall_ms".to_owned()];
    lines.extend(
        timings
            .iter()
            .map(|(step, duration)| format!("{step}\t{:.3}", duration.as_secs_f64() * 1000.0)),
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
struct CargoUnitGraphForDiff {
    units: Vec<CargoUnitForDiff>,
}

#[derive(Debug, Facet)]
struct CargoUnitForDiff {
    pkg_id: String,
    target: CargoUnitTargetForDiff,
    mode: String,
    platform: Option<String>,
    features: Vec<String>,
    profile: CargoProfileForDiff,
    dependencies: Vec<CargoUnitDependencyForDiff>,
}

#[derive(Debug, Facet)]
struct CargoUnitTargetForDiff {
    name: String,
    src_path: String,
    crate_types: Vec<String>,
    kind: Vec<String>,
}

impl CargoUnitTargetForDiff {
    fn normalized_unit_kind(&self) -> Result<String, String> {
        let kind = self
            .kind
            .first()
            .ok_or_else(|| format!("unit target {:?} had no kind", self.name))?;
        Ok(match kind.as_str() {
            "custom-build" => "build-script".to_owned(),
            other => other.to_owned(),
        })
    }
}

#[derive(Debug, Facet)]
struct CargoUnitDependencyForDiff {
    extern_crate_name: Option<String>,
}

#[derive(Debug, Facet)]
struct CargoProfileForDiff {
    name: String,
    opt_level: String,
    lto: String,
    codegen_backend: Option<String>,
    codegen_units: Option<i64>,
    debuginfo: i64,
    split_debuginfo: Option<String>,
    debug_assertions: bool,
    overflow_checks: bool,
    rpath: bool,
    incremental: bool,
    panic: String,
    strip: CargoProfileStripForDiff,
}

#[derive(Debug, Facet)]
struct CargoProfileStripForDiff {
    deferred: String,
}

impl From<&CargoProfileForDiff> for RingUnitProfile {
    fn from(profile: &CargoProfileForDiff) -> Self {
        Self {
            name: profile.name.clone(),
            opt_level: profile.opt_level.clone(),
            lto: profile.lto.clone(),
            codegen_backend: profile
                .codegen_backend
                .clone()
                .unwrap_or_else(|| "null".to_owned()),
            codegen_units: profile
                .codegen_units
                .map(|value| value.to_string())
                .unwrap_or_else(|| "null".to_owned()),
            debuginfo: profile.debuginfo.to_string(),
            split_debuginfo: profile
                .split_debuginfo
                .clone()
                .unwrap_or_else(|| "null".to_owned()),
            debug_assertions: profile.debug_assertions.to_string(),
            overflow_checks: profile.overflow_checks.to_string(),
            rpath: profile.rpath.to_string(),
            incremental: profile.incremental.to_string(),
            panic: profile.panic.clone(),
            strip: profile.strip.deferred.clone(),
        }
    }
}

#[derive(Clone, Debug, Facet)]
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

#[derive(Clone, Debug, Facet)]
struct SparseIndexEntry {
    name: String,
    vers: String,
    deps: Vec<SparseIndexDependency>,
    features: BTreeMap<String, Vec<String>>,
    features2: Option<BTreeMap<String, Vec<String>>>,
    yanked: bool,
}

#[derive(Clone, Debug, Facet, PartialEq, Eq, PartialOrd, Ord)]
struct SparseIndexDependency {
    name: String,
    package: Option<String>,
    req: String,
    features: Vec<String>,
    kind: String,
    target: Option<String>,
    optional: bool,
    default_features: bool,
}

impl SparseIndexDependency {
    fn package_name(&self) -> String {
        self.package.clone().unwrap_or_else(|| self.name.clone())
    }
}

#[derive(Clone, Debug, Facet)]
struct SparseIndexRowForVix {
    name: String,
    vers: String,
    deps: Vec<SparseIndexDependencyForVix>,
    features: BTreeMap<String, Vec<String>>,
    yanked: bool,
}

impl From<SparseIndexEntry> for SparseIndexRowForVix {
    fn from(row: SparseIndexEntry) -> Self {
        Self {
            name: row.name,
            vers: row.vers,
            deps: row.deps.into_iter().map(Into::into).collect(),
            features: merged_sparse_features(row.features, row.features2),
            yanked: row.yanked,
        }
    }
}

fn merged_sparse_features(
    mut features: BTreeMap<String, Vec<String>>,
    features2: Option<BTreeMap<String, Vec<String>>>,
) -> BTreeMap<String, Vec<String>> {
    if let Some(features2) = features2 {
        for (name, enables) in features2 {
            features.entry(name).or_default().extend(enables);
        }
    }
    for enables in features.values_mut() {
        enables.sort();
        enables.dedup();
    }
    features
}

#[derive(Clone, Debug, Facet)]
struct SparseIndexDependencyForVix {
    name: String,
    package: String,
    req: String,
    kind: String,
    target: String,
    optional: bool,
    default_features: bool,
    features: Vec<String>,
}

impl From<SparseIndexDependency> for SparseIndexDependencyForVix {
    fn from(dep: SparseIndexDependency) -> Self {
        Self {
            name: dep.name,
            package: dep.package.unwrap_or_default(),
            req: dep.req,
            kind: dep.kind,
            target: dep.target.unwrap_or_default(),
            optional: dep.optional,
            default_features: dep.default_features,
            features: dep.features,
        }
    }
}

#[derive(Clone, Debug, Facet)]
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

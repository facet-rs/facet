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
fn dependency_declarations_extract_workspace_and_detailed_forms() -> Result<(), String> {
    let mut machine = manifest_machine()?;
    let workspace = intern_tree(&mut machine, workspace_tree())?;
    let taxon = intern_tree(&mut machine, taxon_tree())?;
    let facet_core = intern_tree(&mut machine, facet_core_tree())?;
    let facet = intern_tree(&mut machine, facet_tree())?;

    let blake3 = detailed_dep(&mut machine, taxon, workspace, "blake3", "normal")?;
    assert_eq!(field_string(&blake3, "version_req")?, "^1");
    assert_eq!(field_string(&blake3, "path")?, "");
    assert!(field_bool(&blake3, "workspace")?);
    assert!(!field_bool(&blake3, "default_features")?);

    let autocfg = detailed_dep(&mut machine, facet_core, workspace, "autocfg", "build")?;
    assert_eq!(field_string(&autocfg, "version_req")?, "^1.5.0");
    assert_eq!(field_string(&autocfg, "kind")?, "build");
    assert!(field_bool(&autocfg, "workspace")?);

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
        gap.contains("ResolvedUnit requires Path fields")
            && gap.contains("no string-to-Path constructor"),
        "{gap}"
    );
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

fn intern_tree(machine: &mut Machine, tree: Tree) -> Result<i64, String> {
    Ok(machine.intern_arg("Tree", MachineArg::Tree(tree))?.0)
}

fn intern_string(machine: &mut Machine, value: &str) -> Result<i64, String> {
    Ok(machine
        .intern_arg("String", MachineArg::String(value.to_owned()))?
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

fn workspace_tree() -> Tree {
    Tree::of(&[("Cargo.toml", WORKSPACE_MANIFEST)])
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
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
    facet_json::from_str(&stdout).map_err(|err| err.to_string())
}

#[derive(Debug, Facet)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
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
}

#[derive(Debug, Facet)]
struct CargoPackage {
    name: String,
    version: String,
    edition: String,
    targets: Vec<CargoTarget>,
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

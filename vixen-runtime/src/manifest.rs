//! The machine manifest: what THIS machine offers, as declared typed data —
//! and the binding check that lets a program fail before anything runs
//! (`vix-core/docs/content/spec/vixen/machine.md`).
//!
//! The manifest replaces the runner's capability *conjuring*: until now the
//! demand root minted a capability value for whatever type a test named
//! (`publish_capability`, unconditionally), so nothing could fail. Now the
//! manifest is the single source of the machine's word — the host `Target`
//! plus one offer per capability type, each carrying the tool's program and
//! its facts as ordinary typed fields — and root capability parameters bind
//! against it by declared type before any island of the test is submitted.
//!
//! The requirement side is never spelled beside the code that implies it: it
//! is extracted from use through the capability package's command grammar
//! ([`vixen_primitives::capability_package`]) and normalized to [`Target`]
//! values — the manifest never learns a tool's dialect.
//!
//! r[impl vixen.machine.manifest]

use std::collections::BTreeMap;

use vix::vir::{Island, Module, Node, Op, PartitionedTest, Type};
use vixen_primitives::capability_package::{
    PlanElement, Target, TargetCapture, capability_package,
};

/// The triple this build of the runner is running on — the manifest's default
/// `host` fact. cfg-derived because no taxon-backed `Target` machinery exists
/// yet (see [`Target`]'s honest-stand-in note); the mapping covers the
/// platforms this workspace builds for.
#[must_use]
pub fn host_target() -> Target {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };
    let os = if cfg!(target_os = "linux") {
        "unknown-linux-gnu"
    } else if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(target_os = "windows") {
        "pc-windows-msvc"
    } else {
        "unknown-unknown"
    };
    Target::new(format!("{arch}-{os}"))
}

/// One capability offer: the machine's word for one capability type. The
/// tool's reference is a program path (host-trusting exactly as the exec
/// backend is, for 0.1); the rest are the capability's *facts* as typed
/// fields — machine-ness is never a set of booleans
/// (`vixen.machine.facts-are-fields`).
///
/// r[impl vixen.machine.facts-are-fields]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityOffer {
    /// The capability type's nominal name (`Sh`, `Rustc`, …).
    pub ty: String,
    /// The tool closure's reference: for 0.1 a program path/name.
    pub program: String,
    /// The toolchain version fact, when the tool has one.
    pub toolchain: Option<String>,
    /// The targets this toolchain can produce. Empty for a target-neutral
    /// tool, whose plans never impose a target requirement anyway.
    pub targets: Vec<Target>,
}

impl CapabilityOffer {
    /// Render the offer's facts for a diagnostic — the "machine offers" side.
    #[must_use]
    pub fn rendered(&self) -> String {
        let mut facts = Vec::new();
        if let Some(toolchain) = &self.toolchain {
            facts.push(format!("toolchain {toolchain}"));
        }
        if !self.targets.is_empty() {
            let targets = self
                .targets
                .iter()
                .map(Target::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            facts.push(format!("targets [{targets}]"));
        }
        if facts.is_empty() {
            self.ty.clone()
        } else {
            format!("{} {{ {} }}", self.ty, facts.join(", "))
        }
    }
}

/// A machine's capability set as a declared, typed value: the host `Target`
/// plus one offer per capability type. Nothing is discovered ambiently and
/// nothing is probed to mint identity — the embedder states this value (in
/// tests as a plain Rust value, or loaded as config via [`Self::from_toml`]),
/// and it is the single thing root capability parameters bind against.
///
/// r[impl vixen.machine.manifest]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineManifest {
    pub host: Target,
    pub capabilities: Vec<CapabilityOffer>,
}

impl MachineManifest {
    /// The manifest the ratchet harness runs under by default: the offers the
    /// runner used to conjure, now stated as data. The programs (`echo`,
    /// `sh`) are byte-identical to the parameter names the retired conjuring
    /// path used, so every existing capability value identity is preserved.
    #[must_use]
    pub fn ratchet_default() -> Self {
        let neutral = |ty: &str, program: &str| CapabilityOffer {
            ty: ty.to_owned(),
            program: program.to_owned(),
            toolchain: None,
            targets: Vec::new(),
        };
        Self {
            host: host_target(),
            capabilities: vec![
                neutral("Echo", "echo"),
                neutral("Sh", "sh"),
                neutral("ProgressiveSh", "sh"),
            ],
        }
    }

    /// The machine's offer for one capability type, by nominal name.
    #[must_use]
    pub fn offer(&self, ty: &str) -> Option<&CapabilityOffer> {
        self.capabilities.iter().find(|offer| offer.ty == ty)
    }

    /// Load a manifest from its TOML config spelling — the embedder-facing
    /// form of "the embedder loads the manifest as config". The document
    /// shape:
    ///
    /// ```toml
    /// host = "x86_64-unknown-linux-gnu"
    ///
    /// [[capability]]
    /// ty = "Rustc"
    /// program = "/toolchains/1.89.0/bin/rustc"
    /// toolchain = "1.89.0"
    /// targets = ["x86_64-unknown-linux-gnu"]
    /// ```
    pub fn from_toml(source: &str) -> Result<Self, String> {
        #[derive(facet::Facet)]
        struct OfferDoc {
            ty: String,
            program: String,
            toolchain: Option<String>,
            targets: Option<Vec<String>>,
        }
        #[derive(facet::Facet)]
        struct ManifestDoc {
            host: String,
            capability: Vec<OfferDoc>,
        }
        let doc: ManifestDoc = facet_toml::from_str(source).map_err(|error| error.to_string())?;
        Ok(Self {
            host: Target::new(doc.host),
            capabilities: doc
                .capability
                .into_iter()
                .map(|offer| CapabilityOffer {
                    ty: offer.ty,
                    program: offer.program,
                    toolchain: offer.toolchain,
                    targets: offer
                        .targets
                        .unwrap_or_default()
                        .into_iter()
                        .map(Target::new)
                        .collect(),
                })
                .collect(),
        })
    }

    /// Bind one test's requirement set against this manifest. Every
    /// unsatisfiable requirement — the type absent, or a required `Target`
    /// the offered value's facts lack — is returned as a typed refusal naming
    /// both sides. The caller raises them BEFORE submitting any island, so no
    /// process spawns and no demand parks. A [`TargetRequirement::Computed`]
    /// requirement is not checkable at bind time and imposes nothing here —
    /// the static report already degraded it honestly.
    ///
    /// r[impl vixen.machine.binding-fails-before-effects]
    #[must_use]
    pub fn bind(&self, requirements: &TestRequirements) -> Vec<CapabilityRefusal> {
        let mut refusals = Vec::new();
        for capability in &requirements.capabilities {
            let refuse = |required_target: Option<&Target>, offered: Option<&CapabilityOffer>| {
                CapabilityRefusal {
                    test: requirements.test.clone(),
                    parameter: capability.parameter.clone(),
                    required_type: capability.ty.clone(),
                    required_target: required_target.map(|target| target.as_str().to_owned()),
                    offered: offered.map(CapabilityOffer::rendered),
                }
            };
            let Some(offer) = self.offer(&capability.ty) else {
                refusals.push(refuse(None, None));
                continue;
            };
            for requirement in &capability.targets {
                let required = match requirement {
                    TargetRequirement::Literal(target) => target,
                    TargetRequirement::Computed => continue,
                };
                if !offer.targets.contains(required) {
                    refusals.push(refuse(Some(required), Some(offer)));
                }
            }
        }
        refusals
    }
}

/// The environment variable through which an invoker DECLARES the machine
/// manifest file: an explicit path, read once at the embedder entrypoint.
/// This is a declaration, not discovery — no path is probed, no directory
/// walked, no fallback location tried (`vixen.machine.manifest`: a machine's
/// word is stated, never conjured from its surroundings).
pub const MANIFEST_ENV: &str = "VIX_MACHINE_MANIFEST";

/// A typed manifest-loading failure. A declared file that cannot be read or
/// parsed is THIS error at the entrypoint — never a silent fall-back to the
/// harness default, which would run the program under a machine word the
/// invoker explicitly replaced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestLoadError {
    /// The declared path could not be read.
    Unreadable { path: String, detail: String },
    /// The declared file read, but is not a valid manifest document.
    Malformed { path: String, detail: String },
}

impl core::fmt::Display for ManifestLoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unreadable { path, detail } => write!(
                f,
                "error[manifest]: declared machine manifest `{path}` cannot be read: {detail}"
            ),
            Self::Malformed { path, detail } => write!(
                f,
                "error[manifest]: declared machine manifest `{path}` is not a manifest document: {detail}"
            ),
        }
    }
}

/// Load the manifest an explicit path declares: read the file, parse it as
/// the [`MachineManifest::from_toml`] document. Both failure sides are loud
/// and typed, naming the declared path.
pub fn load_manifest(path: &str) -> Result<MachineManifest, ManifestLoadError> {
    let source = std::fs::read_to_string(path).map_err(|error| ManifestLoadError::Unreadable {
        path: path.to_owned(),
        detail: error.to_string(),
    })?;
    MachineManifest::from_toml(&source).map_err(|detail| ManifestLoadError::Malformed {
        path: path.to_owned(),
        detail,
    })
}

/// Resolve this process's machine word: the manifest file
/// [`MANIFEST_ENV`] explicitly declares, or
/// [`MachineManifest::ratchet_default`] when nothing is declared. A declared
/// file that is missing, unreadable, malformed — or a declared path that is
/// not even UTF-8 — is a loud typed error; the default serves only the
/// UNDECLARED case.
pub fn declared_manifest() -> Result<MachineManifest, ManifestLoadError> {
    match std::env::var_os(MANIFEST_ENV) {
        Some(path) => {
            let path = path.to_str().ok_or_else(|| ManifestLoadError::Unreadable {
                path: path.to_string_lossy().into_owned(),
                detail: "the declared path is not valid UTF-8".to_owned(),
            })?;
            load_manifest(path)
        }
        None => Ok(MachineManifest::ratchet_default()),
    }
}

/// One requirement a test's plans impose on a capability type, in the shared
/// vocabulary — `Target` values, never tool strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetRequirement {
    /// A literal role capture: statically known, checked at bind time.
    Literal(Target),
    /// A capture whose value is computed at run time. The static report
    /// degrades honestly to "target decided at run time"; bind time cannot
    /// check it.
    Computed,
}

/// One capability parameter's requirement row: presence (the declared type)
/// plus every target requirement extracted from the test's plans.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityRequirement {
    /// The declaring parameter's name (diagnostic vocabulary only — binding
    /// is by type).
    pub parameter: String,
    /// The capability type's nominal name.
    pub ty: String,
    pub targets: Vec<TargetRequirement>,
}

/// One test's requirement set, readable without executing anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestRequirements {
    pub test: String,
    pub capabilities: Vec<CapabilityRequirement>,
}

/// A typed pre-effect refusal: the vixen half of
/// `machine.primitive.capabilities-by-identity`'s admissibility sentence,
/// naming both sides. `offered: None` means the type is absent from the
/// manifest entirely.
///
/// r[impl vixen.machine.binding-fails-before-effects]
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct CapabilityRefusal {
    pub test: String,
    pub parameter: String,
    /// What the program requires: the capability type's nominal name…
    pub required_type: String,
    /// …and the required target, when the refusal is finer than presence.
    pub required_target: Option<String>,
    /// What the machine offers for that type, facts rendered; `None` when the
    /// manifest has no offer of the type.
    pub offered: Option<String>,
}

impl core::fmt::Display for CapabilityRefusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "error[capability]: `{}` demands {}",
            self.test, self.required_type
        )?;
        if let Some(target) = &self.required_target {
            write!(f, " producing {target}")?;
        }
        match &self.offered {
            Some(offered) => write!(f, "\n  machine offers: {offered}")?,
            None => write!(f, "\n  machine offers no {}", self.required_type)?,
        }
        write!(f, "\n  no effect was started")
    }
}

/// The nominal name of a capability parameter's declared record type.
#[must_use]
pub fn capability_type_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Record(record) => Some(&record.name),
        _ => None,
    }
}

/// The static requirement report for every test of a compiled module: the
/// root's capability parameter types plus every literal role capture its
/// plans contain, with computed captures degraded honestly. Nothing is
/// executed, demanded, or interned to produce this.
///
/// r[impl vixen.machine.requirements-are-static]
pub fn static_requirements(
    module: &Module,
) -> Result<Vec<TestRequirements>, vix::diagnostic::Diagnostics> {
    module
        .tests
        .iter()
        .map(|test| Ok(test_requirements(&module.try_partition_test(test)?)))
        .collect()
}

/// One partitioned test's requirement set, extracted from use: presence from
/// the declared capability parameters, target requirements from every exec
/// plan's role captures under the owning package's command grammar
/// (`vixen.machine.requirements-from-use`). Literal captures are read
/// directly off the partitioned VIR — this is partition-time knowledge, ahead
/// of bind time and of any execution.
///
/// r[impl vixen.machine.requirements-from-use]
#[must_use]
pub fn test_requirements(partitioned: &PartitionedTest) -> TestRequirements {
    // Presence rows, one per declared capability parameter, in declaration
    // order; target requirements accumulate per capability TYPE (binding is
    // nominal).
    let mut rows: Vec<CapabilityRequirement> = Vec::new();
    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    for capability in &partitioned.capabilities {
        let Some(ty) = capability_type_name(&capability.ty) else {
            continue;
        };
        by_type.entry(ty.to_owned()).or_insert_with(|| {
            rows.push(CapabilityRequirement {
                parameter: capability.name.clone(),
                ty: ty.to_owned(),
                targets: Vec::new(),
            });
            rows.len() - 1
        });
    }
    // Every exec plan in the test, wherever it was partitioned to.
    let mut islands: Vec<&Island> = Vec::new();
    islands.extend(partitioned.values.iter().map(|value| &value.island));
    islands.extend(partitioned.wire_islands.iter().map(|value| &value.island));
    islands.extend(partitioned.islands.iter());
    islands.extend(partitioned.generator.iter());
    for island in islands {
        collect_exec_requirements(&island.nodes, &mut rows, &mut by_type);
        for callee in &island.callees {
            collect_exec_requirements(&callee.nodes, &mut rows, &mut by_type);
        }
    }
    TestRequirements {
        test: partitioned.name.clone(),
        capabilities: rows,
    }
}

/// Scan one node vector for exec invocations and fold each plan's target
/// captures into the requirement rows. The capability type is read from the
/// request record's capability field type; the plan elements are classified
/// literal (`Op::String`) or computed, and the owning package's grammar does
/// the extraction — this function knows no dialect.
fn collect_exec_requirements(
    nodes: &[Node],
    rows: &mut Vec<CapabilityRequirement>,
    by_type: &mut BTreeMap<String, usize>,
) {
    let exec = vix::runtime::exec_primitive_id();
    let node_by_id = |id: vix::vir::NodeId| nodes.iter().find(|node| node.id == id);
    for node in nodes {
        let Op::InvokePrimitive { primitive } = &node.op else {
            continue;
        };
        if *primitive != exec {
            continue;
        }
        // An ABSENT request node is not an invariant break: this scan runs per
        // island, and partitioning decides which island holds which node, so a
        // request the current vector cannot see is one another vector carries.
        // Skipping is right here. A request node that IS present with the wrong
        // arity is a different claim — see the assert below.
        let Some(request) = node.inputs.first().copied().and_then(node_by_id) else {
            continue;
        };
        // `{capability, argv, mounts}` — the mounts are inputs to the process,
        // not part of its plan, so the requirement scan reads past them.
        //
        // A shape mismatch here is an INVARIANT BREAK, not a case to skip: the
        // node is already a confirmed exec-primitive invocation, so its request
        // is the one `lower_exec` built. Skipping quietly is how this scan
        // reported "no requirements" for programs that had them when the mounts
        // field landed — a refusal that never happens. Fail where the cause is.
        assert!(
            request.inputs.len() == 3,
            "an exec request has {{capability, argv, mounts}}; found {} inputs. \
             A request that grows a field must teach this scan what it means.",
            request.inputs.len()
        );
        let [capability_id, argv_id, _mounts_id] = request.inputs.as_slice() else {
            unreachable!("arity asserted above");
        };
        let Some(ty) =
            node_by_id(*capability_id).and_then(|capability| capability_type_name(&capability.ty))
        else {
            continue;
        };
        let Some(package) = capability_package(ty) else {
            continue;
        };
        let plan: Vec<PlanElement> = match node_by_id(*argv_id) {
            Some(argv) if matches!(argv.op, Op::Array) => argv
                .inputs
                .iter()
                .map(|element| match node_by_id(*element).map(|node| &node.op) {
                    Some(Op::String(text)) => PlanElement::Literal(text.clone()),
                    _ => PlanElement::Computed,
                })
                .collect(),
            // A plan whose argv is not structurally visible here is wholly
            // computed — degrade, never guess.
            _ => vec![PlanElement::Computed],
        };
        let captures = package.target_captures(&plan);
        if captures.is_empty() {
            continue;
        }
        let row = *by_type.entry(ty.to_owned()).or_insert_with(|| {
            rows.push(CapabilityRequirement {
                parameter: ty.to_owned(),
                ty: ty.to_owned(),
                targets: Vec::new(),
            });
            rows.len() - 1
        });
        for capture in captures {
            let requirement = match capture {
                TargetCapture::Literal(target) => TargetRequirement::Literal(target),
                TargetCapture::Computed => TargetRequirement::Computed,
            };
            if !rows[row].targets.contains(&requirement) {
                rows[row].targets.push(requirement);
            }
        }
    }
}

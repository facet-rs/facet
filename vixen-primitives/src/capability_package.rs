//! Capability packages as data: per-tool command-grammar knowledge that the
//! machine is banned from holding (`machine.capability.no-argv-dialect`) and
//! this crate is the 0.1 home for (`vixen.capability.packages-ship-in-vixen-primitives`).
//!
//! What a package contributes here is the requirement-bearing slice of its
//! command grammar: which roles of an invocation carry a *target* and how the
//! tool's dialect normalizes into the shared vocabulary — `Target` values,
//! never tool strings (`vixen.machine.requirements-from-use`). The manifest
//! and the binding check compare `Target`s; they never learn a dialect.
//!
//! r[impl vixen.capability.package-is-data]

use vix::runtime::ExecOutputProtocol;

/// A target triple as a first-class typed value.
///
/// HONEST STAND-IN: `machine.primitive.target-value` wants `Target` as a vix
/// value with schema, literal syntax (`t"..."`), and taxon-derived OS/arch.
/// None of that machinery exists on this branch, so this is the smallest
/// representation that keeps the checks typed: a newtype over the canonical
/// triple spelling. Every comparison in the manifest/binding path goes through
/// this type — upgrading it to the taxon-backed value changes its innards,
/// not the call sites.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Target(String);

impl Target {
    #[must_use]
    pub fn new(triple: impl Into<String>) -> Self {
        Self(triple.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for Target {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One argv element of an exec plan as static analysis sees it: a literal the
/// program spelled, or a value computed at run time. Literal captures are
/// therefore checkable before anything executes; computed captures degrade
/// honestly (`vixen.machine.requirements-are-static`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanElement {
    Literal(String),
    Computed,
}

/// One extracted target capture: the requirement an invocation imposes through
/// the package's grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetCapture {
    /// The role's value is spelled literally — the requirement is static.
    Literal(Target),
    /// The role is present but its value is computed at run time: statically
    /// this is "target decided at run time", never silently no requirement.
    Computed,
}

/// How a package's command grammar carries the target requirement — the
/// universal layer is the ROLE, the per-tool spelling is this data
/// (`vixen.machine.requirements-from-use`, the generality table).
#[derive(Clone, Copy, Debug)]
pub enum TargetDiscipline {
    /// No target role exists in this tool's grammar: a target-neutral
    /// invocation imposes no target requirement and runs wherever the tool
    /// exists. This is correct semantics, not a missing case.
    Neutral,
    /// rustc/clang-shaped: a target-role flag whose following argv element is
    /// the tool's target dialect. `normalize` maps that dialect into the
    /// shared vocabulary.
    ArgvFlag {
        flag: &'static str,
        normalize: fn(&str) -> Target,
    },
    /// go-shaped: the target rides declared environment roles. This package
    /// family's command grammar spells environment assignments as leading
    /// `NAME=VALUE` argv elements (the `env(1)` convention); the named roles
    /// combine into one `Target` through `normalize`.
    EnvRoles {
        os_role: &'static str,
        arch_role: &'static str,
        normalize: fn(os: &str, arch: &str) -> Target,
    },
    /// mingw-gcc/`cl.exe`-shaped: the target is in no invocation at all — the
    /// binary/environment IS the target. The requirement is a fact demanded
    /// of the capability itself: the plan implicitly requires the machine's
    /// host, checked against the offered capability's own target facts.
    HostFact,
}

/// One capability package's registered slice: its nominal type, its output
/// protocol (a command-package contract, `machine.primitive.command-package`),
/// and the requirement-bearing part of its command grammar.
#[derive(Clone, Copy, Debug)]
pub struct CapabilityPackage {
    pub name: &'static str,
    pub protocol: ExecOutputProtocol,
    pub target_discipline: TargetDiscipline,
}

/// rustc's target dialect is already the shared triple vocabulary: the
/// normalization is the identity injection into `Target`.
fn rustc_target(dialect: &str) -> Target {
    Target::new(dialect)
}

/// The go-shaped dialect: `(GOOS, GOARCH)` words map into a triple. The map is
/// closed over the words this test package admits; an out-of-map word composes
/// a triple-shaped value so the comparison stays in `Target` vocabulary — a
/// production package would reject it in command validation instead.
fn go_target(os: &str, arch: &str) -> Target {
    let arch = match arch {
        "amd64" => "x86_64",
        "arm64" => "aarch64",
        "386" => "i686",
        other => other,
    };
    let os = match os {
        "linux" => "unknown-linux-gnu",
        "windows" => "pc-windows-gnu",
        "darwin" => "apple-darwin",
        other => return Target::new(format!("{arch}-unknown-{other}")),
    };
    Target::new(format!("{arch}-{os}"))
}

/// The registered packages. The first three are the ratchet corpus's v1
/// packages (target-neutral shells); the last three are the machine-manifest
/// design's generality stress tests (`vixen.machine.requirements-from-use`) —
/// one per ugly end of the spelling table. 0.1 has no package distribution,
/// so a test package registers exactly like a production one
/// (`vixen.capability.packages-ship-in-vixen-primitives`).
pub const CAPABILITY_PACKAGES: &[CapabilityPackage] = &[
    CapabilityPackage {
        name: "Echo",
        protocol: ExecOutputProtocol::ExitOnly,
        target_discipline: TargetDiscipline::Neutral,
    },
    CapabilityPackage {
        name: "Sh",
        protocol: ExecOutputProtocol::ExitOnly,
        target_discipline: TargetDiscipline::Neutral,
    },
    CapabilityPackage {
        name: "ProgressiveSh",
        protocol: ExecOutputProtocol::ProgressiveLinesV1,
        target_discipline: TargetDiscipline::Neutral,
    },
    CapabilityPackage {
        name: "Rustc",
        protocol: ExecOutputProtocol::ExitOnly,
        target_discipline: TargetDiscipline::ArgvFlag {
            flag: "--target",
            normalize: rustc_target,
        },
    },
    CapabilityPackage {
        name: "Go",
        protocol: ExecOutputProtocol::ExitOnly,
        target_discipline: TargetDiscipline::EnvRoles {
            os_role: "GOOS",
            arch_role: "GOARCH",
            normalize: go_target,
        },
    },
    CapabilityPackage {
        name: "MingwGcc",
        protocol: ExecOutputProtocol::ExitOnly,
        target_discipline: TargetDiscipline::HostFact,
    },
];

/// Look one package up by its nominal capability type name.
#[must_use]
pub fn capability_package(name: &str) -> Option<&'static CapabilityPackage> {
    CAPABILITY_PACKAGES
        .iter()
        .find(|package| package.name == name)
}

/// Whether an argv element is an environment assignment under the env-role
/// grammar: `NAME=VALUE` with a nonempty `[A-Za-z_][A-Za-z0-9_]*` name.
fn parse_assignment(element: &str) -> Option<(&str, &str)> {
    let (name, value) = element.split_once('=')?;
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        return None;
    }
    Some((name, value))
}

impl CapabilityPackage {
    /// Split the materialized plan into the environment assignments this
    /// package's grammar declares and the remaining argv. Identity is
    /// untouched: the demand preimage hashes the full normalized plan; this
    /// split happens host-side, at value redemption, exactly like the program
    /// name itself.
    #[must_use]
    pub fn split_invocation(&self, argv: Vec<String>) -> (Vec<(String, String)>, Vec<String>) {
        match self.target_discipline {
            TargetDiscipline::EnvRoles { .. } => {
                let mut env = Vec::new();
                let mut rest = Vec::new();
                let mut in_leading = true;
                for element in argv {
                    if in_leading && let Some((name, value)) = parse_assignment(&element) {
                        env.push((name.to_owned(), value.to_owned()));
                        continue;
                    }
                    in_leading = false;
                    rest.push(element);
                }
                (env, rest)
            }
            _ => (Vec::new(), argv),
        }
    }

    /// Extract the target captures a plan imposes under this package's
    /// grammar. `plan` is the argv as static analysis sees it; the same
    /// extraction applies to a fully materialized argv by presenting every
    /// element as a literal.
    #[must_use]
    pub fn target_captures(&self, plan: &[PlanElement]) -> Vec<TargetCapture> {
        match self.target_discipline {
            TargetDiscipline::Neutral | TargetDiscipline::HostFact => Vec::new(),
            TargetDiscipline::ArgvFlag { flag, normalize } => {
                let mut captures = Vec::new();
                let mut elements = plan.iter().peekable();
                while let Some(element) = elements.next() {
                    if !matches!(element, PlanElement::Literal(text) if text == flag) {
                        continue;
                    }
                    match elements.peek() {
                        Some(PlanElement::Literal(value)) => {
                            captures.push(TargetCapture::Literal(normalize(value)));
                        }
                        // The role is present but its value is computed: the
                        // requirement exists, its target is decided at run
                        // time.
                        Some(PlanElement::Computed) => captures.push(TargetCapture::Computed),
                        // A trailing flag with no value is the tool's own
                        // command-validation failure, not a capture.
                        None => {}
                    }
                }
                captures
            }
            TargetDiscipline::EnvRoles {
                os_role,
                arch_role,
                normalize,
            } => {
                // Scan the leading assignment region. A computed element in
                // that region could itself be an assignment to a role, so it
                // poisons static knowledge: the capture degrades to Computed
                // rather than silently vanishing.
                let mut os = None;
                let mut arch = None;
                let mut saw_computed = false;
                for element in plan {
                    match element {
                        PlanElement::Literal(text) => {
                            let Some((name, value)) = parse_assignment(text) else {
                                break;
                            };
                            if name == os_role {
                                os = Some(value.to_owned());
                            } else if name == arch_role {
                                arch = Some(value.to_owned());
                            }
                        }
                        PlanElement::Computed => {
                            saw_computed = true;
                            break;
                        }
                    }
                }
                match (os, arch) {
                    (Some(os), Some(arch)) => vec![TargetCapture::Literal(normalize(&os, &arch))],
                    // A partial assignment (one role, the other defaulting to
                    // the tool's host detection) or a computed leading element
                    // is a target decided at run time.
                    (None, None) if !saw_computed => Vec::new(),
                    _ => vec![TargetCapture::Computed],
                }
            }
        }
    }

    /// Whether this package's plans implicitly require the machine's host as
    /// their target — the fact-shaped end of the table: no capture exists, the
    /// capability's own target facts are the claim.
    #[must_use]
    pub fn requires_host_fact(&self) -> bool {
        matches!(self.target_discipline, TargetDiscipline::HostFact)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The injected capability-type list and the package registry are two
    /// spellings of one set: a type a program can name must have a package
    /// (grammar, protocol), and a registered package must be nameable.
    #[test]
    fn every_declared_capability_type_has_exactly_one_package() {
        let types: Vec<&str> = crate::CAPABILITY_TYPES.iter().map(|decl| decl.name).collect();
        let packages: Vec<&str> = CAPABILITY_PACKAGES.iter().map(|package| package.name).collect();
        assert_eq!(types, packages);
    }
}

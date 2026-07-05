//! Native real-process exec backend for open vix.
//!
//! This backend deliberately trusts the host. It does not sandbox, interpose a
//! VFS, or observe arbitrary process reads. Declared command roles define the
//! staged input ceiling and the tier-2 read-set: input roles pin content bytes;
//! search-dir roles pin directory membership only.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};

use crate::exec::{ExecCache, ExecEvent, ExecPlan, ObservedWorld, Outcome, Role, Tool, Tree};
use crate::machine::{
    MachineExecBackend, MachineExecRequest, MachinePathDemand, MachinePendingRun,
};

const ENV_ALLOWLIST: &[&str] = &["PATH", "HOME", "TMPDIR", "TEMP", "TMP", "SystemRoot"];

pub struct RealProcessBackend {
    cache: Mutex<ExecCache>,
}

impl RealProcessBackend {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(ExecCache::new()),
        }
    }
}

impl Default for RealProcessBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MachineExecBackend for RealProcessBackend {
    fn spawn(&self, request: MachineExecRequest) -> Result<Arc<dyn MachinePendingRun>, String> {
        let tool = RealProcessTool {
            command: request.command.clone(),
            output: request.output.clone(),
        };
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "real-process exec cache poisoned".to_string())?;
        let outcome = cache.exec(&request.plan, request.capability, &request.mounts, &tool)?;
        let event = cache
            .events
            .last()
            .cloned()
            .ok_or_else(|| "real-process cache did not record an event".to_string())?;
        Ok(Arc::new(RealProcessRun { outcome, event }))
    }
}

struct RealProcessRun {
    outcome: Outcome,
    event: ExecEvent,
}

impl MachinePendingRun for RealProcessRun {
    fn demand_path(&self, path: &str) -> Result<MachinePathDemand, String> {
        if self.outcome.outputs.entries.contains_key(path) {
            let contents = self
                .outcome
                .outputs
                .entries
                .get(path)
                .expect("entry checked")
                .clone();
            return Ok(MachinePathDemand::File(contents));
        }
        if self.outcome.outputs.blobs.contains_key(path)
            || has_child(&self.outcome.outputs.entries, path)
            || has_child(&self.outcome.outputs.blobs, path)
        {
            return Ok(MachinePathDemand::FinishRequired {
                path: path.to_string(),
            });
        }
        Ok(MachinePathDemand::Missing {
            path: path.to_string(),
        })
    }

    fn flush(&self) -> Result<(Tree, ExecEvent), String> {
        Ok((self.outcome.outputs.clone(), self.event.clone()))
    }
}

fn has_child<T>(entries: &BTreeMap<String, T>, path: &str) -> bool {
    let prefix = format!("{path}/");
    entries.keys().any(|entry| entry.starts_with(&prefix))
}

struct RealProcessTool {
    command: String,
    output: String,
}

impl Tool for RealProcessTool {
    fn run(&self, plan: &ExecPlan, world: &mut ObservedWorld<'_>) -> Result<Tree, String> {
        let plan = plan.normalized();
        let temp = tempfile::Builder::new()
            .prefix("vix-real-process-")
            .tempdir()
            .map_err(|err| format!("real-process tempdir: {err}"))?;
        let root = temp.path();
        fs::create_dir_all(root.join(".vix-cas"))
            .map_err(|err| format!("real-process cas dir: {err}"))?;

        stage_declared_inputs(&plan, world, root)?;
        prepare_output_dirs(&plan, root)?;

        let argv = plan
            .argv
            .iter()
            .map(|(arg, role)| map_arg(arg, *role, root))
            .collect::<Result<Vec<_>, _>>()?;
        let output = Command::new(&self.command)
            .args(argv)
            .current_dir(root)
            .env_clear()
            .envs(scrubbed_env(root))
            .output()
            .map_err(|err| format!("real-process {} spawn failed: {err}", self.command))?;
        if !output.status.success() {
            return Err(format!(
                "real-process {} exited with {}: {}",
                self.command,
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        harvest_outputs(&plan, root).or_else(|err| {
            if self.output.is_empty() {
                Err(err)
            } else {
                let mut tree = Tree::default();
                let physical = physical_path(&self.output, root)?;
                let bytes = fs::read(&physical).map_err(|read_err| {
                    format!("{err}; fallback output read failed: {read_err}")
                })?;
                tree.insert_bytes(self.output.clone(), bytes);
                Ok(tree)
            }
        })
    }
}

fn stage_declared_inputs(
    plan: &ExecPlan,
    world: &mut ObservedWorld<'_>,
    root: &Path,
) -> Result<(), String> {
    for (arg, role) in &plan.argv {
        match role {
            Role::Input => {
                let bytes = world.read_bytes(arg).ok_or_else(|| {
                    format!("real-process input `{arg}` is outside mounted trees")
                })?;
                stage_file(root, arg, &bytes)?;
            }
            Role::SearchDir => {
                let physical = physical_path(arg, root)?;
                fs::create_dir_all(&physical)
                    .map_err(|err| format!("real-process create search dir `{arg}`: {err}"))?;
                if let Some(names) = world.list(arg) {
                    for name in names {
                        let logical = listed_logical_path(arg, &name, world);
                        if let Some(bytes) = world.peek_bytes(&logical) {
                            stage_file(root, &logical, &bytes)?;
                        }
                    }
                }
            }
            Role::Output | Role::Flag => {}
        }
    }
    Ok(())
}

fn listed_logical_path(dir: &str, name: &str, world: &ObservedWorld<'_>) -> String {
    let direct = format!("{}/{}", dir.trim_end_matches('/'), name);
    if world.peek_bytes(&direct).is_some() {
        return direct;
    }
    if let Some(root) = mount_root(dir) {
        let rooted = format!("{root}/{name}");
        if world.peek_bytes(&rooted).is_some() {
            return rooted;
        }
    }
    direct
}

fn mount_root(path: &str) -> Option<String> {
    let mut parts = path.trim_start_matches('/').split('/');
    let first = parts.next()?;
    let second = parts.next()?;
    if first == "m" {
        Some(format!("/{first}/{second}"))
    } else {
        None
    }
}

fn prepare_output_dirs(plan: &ExecPlan, root: &Path) -> Result<(), String> {
    for (arg, role) in &plan.argv {
        if *role == Role::Output {
            let path = physical_path(arg, root)?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("real-process create output dir `{arg}`: {err}"))?;
            }
        }
    }
    Ok(())
}

fn harvest_outputs(plan: &ExecPlan, root: &Path) -> Result<Tree, String> {
    let mut tree = Tree::default();
    for (arg, role) in &plan.argv {
        if *role == Role::Output {
            let path = physical_path(arg, root)?;
            let bytes = fs::read(&path)
                .map_err(|err| format!("real-process output `{arg}` was not produced: {err}"))?;
            tree.insert_bytes(arg.clone(), bytes);
        }
    }
    if tree.entries.is_empty() && tree.blobs.is_empty() {
        return Err("real-process plan declared no outputs".to_string());
    }
    Ok(tree)
}

fn stage_file(root: &Path, logical: &str, bytes: &[u8]) -> Result<(), String> {
    let physical = physical_path(logical, root)?;
    if let Some(parent) = physical.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("real-process create staged parent `{logical}`: {err}"))?;
    }

    let hash = Sha256::digest(bytes);
    let cas_path = root.join(".vix-cas").join(hex::encode(hash));
    if !cas_path.exists() {
        fs::write(&cas_path, bytes)
            .map_err(|err| format!("real-process write cas `{logical}`: {err}"))?;
    }
    fs::hard_link(&cas_path, &physical)
        .or_else(|_| fs::copy(&cas_path, &physical).map(|_| ()))
        .map_err(|err| format!("real-process stage `{logical}`: {err}"))
}

fn map_arg(arg: &str, role: Role, root: &Path) -> Result<OsString, String> {
    match role {
        Role::Input | Role::Output | Role::SearchDir => {
            Ok(physical_path(arg, root)?.into_os_string())
        }
        Role::Flag => Ok(OsString::from(arg)),
    }
}

fn physical_path(logical: &str, root: &Path) -> Result<PathBuf, String> {
    let relative = logical.trim_start_matches('/');
    let path = Path::new(relative);
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return Err(format!(
            "real-process refuses non-relative staged path `{logical}`"
        ));
    }
    Ok(root.join(path))
}

fn scrubbed_env(root: &Path) -> Vec<(OsString, OsString)> {
    let mut env: Vec<(OsString, OsString)> = ENV_ALLOWLIST
        .iter()
        .filter_map(|key| std::env::var_os(key).map(|value| (OsString::from(key), value)))
        .collect();
    let tmp = root.join("tmp");
    let _ = fs::create_dir_all(&tmp);
    let tmp = tmp.into_os_string();
    env.retain(|(key, _)| !matches!(key.to_str(), Some("TMPDIR" | "TEMP" | "TMP")));
    env.push((OsString::from("TMPDIR"), tmp.clone()));
    env.push((OsString::from("TEMP"), tmp.clone()));
    env.push((OsString::from("TMP"), tmp));
    env
}

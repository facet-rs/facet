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
use std::thread::JoinHandle;

use sha2::{Digest, Sha256};

use crate::exec::{
    ExecCache, ExecEvent, ExecPlan, MountedWorld, ObservedWorld, Outcome, Role, Tool, Tree,
    role_embedded_paths, role_input_paths, role_output_paths, role_search_dir_paths,
};
use crate::machine::{
    MachineExecBackend, MachineExecRequest, MachinePathDemand, MachinePendingRun,
};

const ENV_ALLOWLIST: &[&str] = &["PATH", "HOME", "TMPDIR", "TEMP", "TMP", "SystemRoot"];

pub struct RealProcessBackend {
    cache: Arc<Mutex<ExecCache>>,
}

impl RealProcessBackend {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(ExecCache::new())),
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
        let cache = Arc::clone(&self.cache);
        let handle = std::thread::spawn(move || run_real_process_request(cache, request));
        Ok(Arc::new(RealProcessRun {
            state: Mutex::new(Some(RealProcessRunState::Starting(handle))),
        }))
    }
}

struct RealProcessRun {
    state: Mutex<Option<RealProcessRunState>>,
}

enum RealProcessRunState {
    Starting(JoinHandle<Result<(Outcome, ExecEvent), String>>),
    Done(Outcome, ExecEvent),
}

impl MachinePendingRun for RealProcessRun {
    fn demand_path(&self, path: &str) -> Result<MachinePathDemand, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "real-process run state poisoned".to_string())?;
        if let Some(RealProcessRunState::Done(outcome, _)) = &*state {
            if outcome.outputs.entries.contains_key(path) {
                let contents = outcome
                    .outputs
                    .entries
                    .get(path)
                    .expect("entry checked")
                    .clone();
                return Ok(MachinePathDemand::File(contents));
            }
            if outcome.outputs.blobs.contains_key(path)
                || has_child(&outcome.outputs.entries, path)
                || has_child(&outcome.outputs.blobs, path)
            {
                return Ok(MachinePathDemand::FinishRequired {
                    path: path.to_string(),
                });
            }
            return Ok(MachinePathDemand::Missing {
                path: path.to_string(),
            });
        }
        Ok(MachinePathDemand::FinishRequired {
            path: path.to_string(),
        })
    }

    fn flush(&self) -> Result<(Outcome, ExecEvent), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "real-process run state poisoned".to_string())?;
        match state
            .take()
            .ok_or_else(|| "real-process run state missing".to_string())?
        {
            RealProcessRunState::Done(outcome, event) => {
                *state = Some(RealProcessRunState::Done(outcome.clone(), event.clone()));
                Ok((outcome, event))
            }
            RealProcessRunState::Starting(handle) => {
                let (outcome, event) = handle
                    .join()
                    .map_err(|_| "real-process runner thread panicked".to_string())??;
                *state = Some(RealProcessRunState::Done(outcome.clone(), event.clone()));
                Ok((outcome, event))
            }
        }
    }
}

fn run_real_process_request(
    cache: Arc<Mutex<ExecCache>>,
    request: MachineExecRequest,
) -> Result<(Outcome, ExecEvent), String> {
    if let Some((outcome, event)) = {
        let mut cache = cache
            .lock()
            .map_err(|_| "real-process exec cache poisoned".to_string())?;
        cache
            .lookup(&request.plan, request.capability, &request.mounts)
            .map(|outcome| {
                let event = cache
                    .events
                    .last()
                    .cloned()
                    .expect("lookup pushed an event");
                (outcome, event)
            })
    } {
        return Ok((outcome, event));
    }

    let tool = RealProcessTool {
        command: request.command,
        output: request.output,
    };
    let world = MountedWorld::new(&request.mounts);
    let mut observed = ObservedWorld::new(&world);
    let outputs = tool.run(&request.plan, &mut observed)?;
    let outcome = Outcome {
        outputs,
        read_set: observed.into_read_set(),
    };
    let mut cache = cache
        .lock()
        .map_err(|_| "real-process exec cache poisoned".to_string())?;
    cache.record_ran(
        &request.plan,
        request.capability,
        &request.mounts,
        outcome.clone(),
    );
    let event = cache
        .events
        .last()
        .cloned()
        .ok_or_else(|| "real-process cache did not record an event".to_string())?;
    Ok((outcome, event))
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
            Role::Input | Role::InputFlag => {
                for input in role_input_paths(arg, *role) {
                    let bytes = world.read_bytes(&input).ok_or_else(|| {
                        format!("real-process input `{input}` is outside mounted trees")
                    })?;
                    stage_file(root, &input, &bytes)?;
                }
            }
            Role::SearchDir | Role::SearchDirFlag => {
                for dir in role_search_dir_paths(arg, *role) {
                    let physical = physical_path(&dir, root)?;
                    fs::create_dir_all(&physical)
                        .map_err(|err| format!("real-process create search dir `{dir}`: {err}"))?;
                    if let Some(names) = world.list(&dir) {
                        for name in names {
                            let logical = listed_logical_path(&dir, &name, world);
                            if let Some(bytes) = world.peek_bytes(&logical) {
                                stage_file(root, &logical, &bytes)?;
                            }
                        }
                    }
                }
            }
            Role::Output | Role::OutputFlag | Role::Flag => {}
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
        for output in role_output_paths(arg, *role) {
            let path = physical_path(&output, root)?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("real-process create output dir `{output}`: {err}"))?;
            }
        }
    }
    Ok(())
}

fn harvest_outputs(plan: &ExecPlan, root: &Path) -> Result<Tree, String> {
    let mut tree = Tree::default();
    for (arg, role) in &plan.argv {
        for output in role_output_paths(arg, *role) {
            let path = physical_path(&output, root)?;
            let bytes = fs::read(&path)
                .map_err(|err| format!("real-process output `{output}` was not produced: {err}"))?;
            tree.insert_bytes(output, bytes);
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
        Role::InputFlag | Role::OutputFlag | Role::SearchDirFlag => {
            let mut mapped = arg.to_string();
            let mut paths = role_embedded_paths(arg, role);
            paths.sort_by_key(|path| std::cmp::Reverse(path.len()));
            for path in paths {
                let physical = physical_path(&path, root)?;
                mapped = mapped.replace(&path, physical.to_string_lossy().as_ref());
            }
            Ok(OsString::from(mapped))
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

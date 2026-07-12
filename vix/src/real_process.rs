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

use crate::exec::{
    ExecCache, ExecEvent, ExecPlan, MountedWorld, ObservedWorld, Outcome, Role, Tool, Tree,
    role_embedded_paths, role_env_paths, role_input_paths, role_output_dir_paths,
    role_output_paths, role_search_dir_paths, role_stdout_paths,
};
use crate::machine::{
    MachineExecBackend, MachineExecRequest, MachinePathDemand, MachinePendingRun,
};

const ENV_ALLOWLIST: &[&str] = &["PATH", "HOME", "TMPDIR", "TEMP", "TMP", "SystemRoot"];
const CC_TOOLCHAIN_REQUIREMENT: &str = "--requires-toolchain=cc";

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
    let capability = real_process_capability(&request.command, request.capability, &request.plan)?;
    if let Some((outcome, event)) = {
        let mut cache = cache
            .lock()
            .map_err(|_| "real-process exec cache poisoned".to_string())?;
        cache
            .lookup(&request.plan, capability, &request.mounts)
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
        tree_events: Vec::new(),
    };
    let mut cache = cache
        .lock()
        .map_err(|_| "real-process exec cache poisoned".to_string())?;
    cache.record_ran(&request.plan, capability, &request.mounts, outcome.clone());
    let event = cache
        .events
        .last()
        .cloned()
        .ok_or_else(|| "real-process cache did not record an event".to_string())?;
    Ok((outcome, event))
}

fn real_process_capability(command: &str, capability: u64, plan: &ExecPlan) -> Result<u64, String> {
    let capability = if command == "rustc" {
        rustc_real_process_capability(capability)?
    } else {
        capability
    };

    if command == "build_script" && build_script_requires_cc(plan) {
        return build_script_cc_capability(capability);
    }

    Ok(capability)
}

fn rustc_real_process_capability(capability: u64) -> Result<u64, String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|err| format!("real-process rustc -vV failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "real-process rustc -vV exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Proc-macro crates produce dylibs that the consumer rustc loads into its
    // own process. Cargo's unit graph keeps these as host units; recording
    // `rustc -vV` in the native backend key keeps producer/consumer artifacts
    // tied to the exact compiler binary behind the `rustc` command.
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-real-process-rustc-capability");
    hasher.update(&capability.to_le_bytes());
    hasher.update(&output.stdout);
    let hash = hasher.finalize();
    Ok(u64::from_le_bytes(
        hash.as_bytes()[..8].try_into().expect("blake3 prefix"),
    ))
}

fn build_script_cc_capability(capability: u64) -> Result<u64, String> {
    let identity = probe_cc_toolchain()?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-real-process-build-script-cc-capability");
    hasher.update(&capability.to_le_bytes());
    identity.update_hash(&mut hasher);
    let hash = hasher.finalize();
    let effective = u64::from_le_bytes(hash.as_bytes()[..8].try_into().expect("blake3 prefix"));
    write_cc_toolchain_receipt(&identity, effective)?;
    Ok(effective)
}

#[derive(Clone)]
struct CcToolchainIdentity {
    path: PathBuf,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl CcToolchainIdentity {
    fn update_hash(&self, hasher: &mut blake3::Hasher) {
        let path = self.path.to_string_lossy();
        hasher.update(path.as_bytes());
        hasher.update(&self.stdout);
        hasher.update(&self.stderr);
    }

    fn identity_hash(&self) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"vix-real-process-cc-toolchain-identity");
        self.update_hash(&mut hasher);
        hasher.finalize()
    }
}

fn probe_cc_toolchain() -> Result<CcToolchainIdentity, String> {
    let path = resolve_program_on_path("cc")?;
    let output = Command::new(&path)
        .arg("--version")
        .output()
        .map_err(|err| format!("real-process cc --version failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "real-process cc --version exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(CcToolchainIdentity {
        path,
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

fn resolve_program_on_path(program: &str) -> Result<PathBuf, String> {
    let path = std::env::var_os("PATH")
        .ok_or_else(|| format!("real-process cannot resolve `{program}` without PATH"))?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!("real-process could not find `{program}` on PATH"))
}

fn write_cc_toolchain_receipt(
    identity: &CcToolchainIdentity,
    effective_capability: u64,
) -> Result<(), String> {
    let Ok(out_root) = std::env::var("TIER_A_OUT") else {
        return Ok(());
    };
    let out_root = Path::new(&out_root);
    fs::create_dir_all(out_root)
        .map_err(|err| format!("real-process create capability receipt dir: {err}"))?;
    let receipt = format!(
        "capability\tcc\ncapability_u64\t{effective_capability}\nidentity_hash\t{}\npath\t{}\nversion_stdout\n{}\nversion_stderr\n{}\n",
        identity.identity_hash().to_hex(),
        identity.path.display(),
        String::from_utf8_lossy(&identity.stdout),
        String::from_utf8_lossy(&identity.stderr),
    );
    fs::write(out_root.join("real-process-cc-capability.txt"), receipt)
        .map_err(|err| format!("real-process write cc capability receipt: {err}"))
}

fn build_script_requires_cc(plan: &ExecPlan) -> bool {
    plan.argv
        .iter()
        .any(|(arg, role)| *role == Role::Flag && arg == CC_TOOLCHAIN_REQUIREMENT)
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

        let mut env = command_env(&plan, root)?;
        if self.command == "build_script" {
            configure_build_script_toolchain_env(&plan, root, &mut env)?;
        }
        let (program, argv, current_dir) = if self.command == "build_script" {
            let mut executable = None;
            for (arg, role) in &plan.argv {
                if *role == Role::Executable {
                    executable = Some(physical_path(arg, root)?);
                    break;
                }
            }
            let executable = executable
                .ok_or_else(|| "build_script command missing --executable".to_string())?;
            make_executable(&executable)?;
            let argv = process_argv(&plan, root)?;
            let current_dir = build_script_current_dir(&plan, root)?;
            (executable.into_os_string(), argv, current_dir)
        } else {
            (
                OsString::from(&self.command),
                map_argv(&self.command, &plan, world, root)?,
                root.to_path_buf(),
            )
        };

        let output = Command::new(&program)
            .args(&argv)
            .current_dir(&current_dir)
            .env_clear()
            .envs(env.iter().cloned())
            .output()
            .map_err(|err| format!("real-process {} spawn failed: {err}", self.command))?;
        if !output.status.success() {
            dump_failed_process(&self.command, &program, &argv, &current_dir, &env)?;
            dump_failed_inputs(&self.command, &plan, root)?;
            return Err(format!(
                "real-process {} exited with {}: {}",
                self.command,
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        harvest_outputs(&plan, root, &output.stdout).or_else(|err| {
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
            Role::Executable | Role::Input | Role::InputFlag => {
                for input in role_input_paths(arg, *role) {
                    stage_declared_input(&input, world, root)?;
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
            Role::Env => {
                for path in role_env_paths(arg, *role) {
                    stage_env_path(root, &path, world)?;
                }
            }
            Role::Output | Role::OutputFlag | Role::OutputDir | Role::Stdout | Role::Flag => {}
        }
    }
    Ok(())
}

fn stage_env_path(root: &Path, path: &str, world: &mut ObservedWorld<'_>) -> Result<(), String> {
    if let Some(bytes) = world.read_bytes(path) {
        stage_file(root, path, &bytes)?;
        return Ok(());
    }
    stage_env_dir(root, path, world)
}

fn stage_env_dir(root: &Path, path: &str, world: &mut ObservedWorld<'_>) -> Result<(), String> {
    let Some(names) = world.list(path) else {
        return Ok(());
    };
    for name in names {
        let logical = listed_logical_path(path, &name, world);
        if let Some(bytes) = world.read_bytes(&logical) {
            stage_file(root, &logical, &bytes)?;
        } else {
            stage_env_dir(root, &logical, world)?;
        }
    }
    Ok(())
}

fn stage_declared_input(
    input: &str,
    world: &mut ObservedWorld<'_>,
    root: &Path,
) -> Result<(), String> {
    if let Some(bytes) = world.read_bytes(input) {
        stage_file(root, input, &bytes)?;
        if let Some(mount) = mount_root(input) {
            stage_env_dir(root, &mount, world)?;
        }
        return Ok(());
    }
    if let Some(names) = world.list(input) {
        for name in names {
            let logical = format!("{}/{}", input.trim_end_matches('/'), name);
            let bytes = world.read_bytes(&logical).ok_or_else(|| {
                format!("real-process input `{logical}` vanished while staging `{input}`")
            })?;
            stage_file(root, &logical, &bytes)?;
        }
        return Ok(());
    }
    Err(format!(
        "real-process input `{input}` is outside mounted trees"
    ))
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
        for output in role_output_dir_paths(arg, *role) {
            let path = physical_path(&output, root)?;
            fs::create_dir_all(&path)
                .map_err(|err| format!("real-process create output dir `{output}`: {err}"))?;
        }
    }
    Ok(())
}

fn harvest_outputs(plan: &ExecPlan, root: &Path, stdout: &[u8]) -> Result<Tree, String> {
    let mut tree = Tree::default();
    for (arg, role) in &plan.argv {
        for output in role_output_paths(arg, *role) {
            let path = physical_path(&output, root)?;
            let bytes = fs::read(&path)
                .map_err(|err| format!("real-process output `{output}` was not produced: {err}"))?;
            tree.insert_bytes(output, bytes);
        }
        for stdout_path in role_stdout_paths(arg, *role) {
            tree.insert_bytes(stdout_path, stdout.to_vec());
        }
        for output_dir in role_output_dir_paths(arg, *role) {
            harvest_output_dir(root, &output_dir, &mut tree)?;
        }
    }
    if tree.entries.is_empty() && tree.blobs.is_empty() {
        return Err("real-process plan declared no outputs".to_string());
    }
    Ok(tree)
}

fn dump_failed_inputs(command: &str, plan: &ExecPlan, root: &Path) -> Result<(), String> {
    let Ok(out_root) = std::env::var("TIER_A_OUT") else {
        return Ok(());
    };
    let out_root = Path::new(&out_root);
    dump_failed_root(command, root, out_root)?;
    let dump_root = out_root.join("real-process-failed-inputs");
    fs::create_dir_all(&dump_root)
        .map_err(|err| format!("real-process create failed-input dump dir: {err}"))?;
    for (arg, role) in &plan.argv {
        for input in role_input_paths(arg, *role) {
            let physical = physical_path(&input, root)?;
            if !physical.is_file() {
                continue;
            }
            let bytes = fs::read(&physical).map_err(|err| {
                format!(
                    "real-process read failed-input `{}`: {err}",
                    physical.display()
                )
            })?;
            let name = format!(
                "{}__{}",
                command,
                input.trim_start_matches('/').replace('/', "__")
            );
            fs::write(dump_root.join(name), bytes)
                .map_err(|err| format!("real-process write failed-input dump: {err}"))?;
        }
    }
    Ok(())
}

fn dump_failed_process(
    command: &str,
    program: &OsString,
    argv: &[OsString],
    current_dir: &Path,
    env: &[(OsString, OsString)],
) -> Result<(), String> {
    let Ok(out_root) = std::env::var("TIER_A_OUT") else {
        return Ok(());
    };
    let mut out = String::new();
    out.push_str(&format!("command\t{command}\n"));
    out.push_str(&format!("program\t{}\n", program.to_string_lossy()));
    out.push_str(&format!("current_dir\t{}\n", current_dir.display()));
    out.push_str("argv\n");
    for arg in argv {
        out.push_str(&format!("{}\n", arg.to_string_lossy()));
    }
    out.push_str("env\n");
    for (key, value) in env {
        out.push_str(&format!(
            "{}={}\n",
            key.to_string_lossy(),
            value.to_string_lossy()
        ));
    }
    fs::write(
        Path::new(&out_root).join("real-process-failed-command.txt"),
        out,
    )
    .map_err(|err| format!("real-process write failed command dump: {err}"))
}

fn dump_failed_root(command: &str, root: &Path, out_root: &Path) -> Result<(), String> {
    let dump_root = out_root.join("real-process-failed-root");
    fs::create_dir_all(&dump_root)
        .map_err(|err| format!("real-process create failed-root dump dir: {err}"))?;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)
            .map_err(|err| format!("real-process read failed-root `{}`: {err}", dir.display()))?
        {
            let entry = entry.map_err(|err| format!("real-process failed-root entry: {err}"))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|err| format!("real-process failed-root strip prefix: {err}"))?;
            let relative = relative
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "__");
            let bytes = fs::read(&path).map_err(|err| {
                format!("real-process read failed-root `{}`: {err}", path.display())
            })?;
            fs::write(dump_root.join(format!("{command}__{relative}")), bytes)
                .map_err(|err| format!("real-process write failed-root dump: {err}"))?;
        }
    }
    Ok(())
}

fn harvest_output_dir(root: &Path, logical_dir: &str, tree: &mut Tree) -> Result<(), String> {
    let physical = physical_path(logical_dir, root)?;
    if !physical.exists() {
        return Ok(());
    }
    let mut stack = vec![physical];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)
            .map_err(|err| format!("real-process read output dir `{}`: {err}", dir.display()))?
        {
            let entry = entry.map_err(|err| {
                format!(
                    "real-process read output dir entry `{}`: {err}",
                    dir.display()
                )
            })?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|err| format!("real-process output path escaped root: {err}"))?
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = fs::read(&path).map_err(|err| {
                    format!(
                        "real-process output `{}` was not readable: {err}",
                        path.display()
                    )
                })?;
                tree.insert_bytes(relative, bytes);
            }
        }
    }
    Ok(())
}

fn stage_file(root: &Path, logical: &str, bytes: &[u8]) -> Result<(), String> {
    let physical = physical_path(logical, root)?;
    if let Some(parent) = physical.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("real-process create staged parent `{logical}`: {err}"))?;
    }

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-real-process-cas");
    hasher.update(
        &i64::try_from(bytes.len())
            .expect("staged file length fits i64")
            .to_le_bytes(),
    );
    hasher.update(bytes);
    let cas_path = root
        .join(".vix-cas")
        .join(hex::encode(hasher.finalize().as_bytes()));
    if !cas_path.exists() {
        fs::write(&cas_path, bytes)
            .map_err(|err| format!("real-process write cas `{logical}`: {err}"))?;
    }
    if physical.exists() {
        let current = fs::read(&physical)
            .map_err(|err| format!("real-process read existing staged `{logical}`: {err}"))?;
        if current == bytes {
            return Ok(());
        }
        fs::remove_file(&physical)
            .map_err(|err| format!("real-process replace staged `{logical}`: {err}"))?;
    }
    fs::copy(&cas_path, &physical)
        .map(|_| ())
        .map_err(|err| format!("real-process stage `{logical}`: {err}"))
}

fn process_argv(plan: &ExecPlan, root: &Path) -> Result<Vec<OsString>, String> {
    plan.argv
        .iter()
        .filter(|(arg, role)| !is_control_arg(arg, *role))
        .map(|(arg, role)| map_arg(arg, *role, root))
        .collect()
}

fn is_control_arg(arg: &str, role: Role) -> bool {
    matches!(
        role,
        Role::Executable | Role::OutputDir | Role::Stdout | Role::Env
    ) || matches!(arg, "--executable" | "--stdout" | "--out-dir" | "--env")
        || arg == CC_TOOLCHAIN_REQUIREMENT
}

fn map_arg(arg: &str, role: Role, root: &Path) -> Result<OsString, String> {
    match role {
        Role::Executable
        | Role::Input
        | Role::Output
        | Role::OutputDir
        | Role::Stdout
        | Role::SearchDir => Ok(physical_path(arg, root)?.into_os_string()),
        Role::InputFlag | Role::OutputFlag | Role::Env | Role::SearchDirFlag => {
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

fn command_env(plan: &ExecPlan, root: &Path) -> Result<Vec<(OsString, OsString)>, String> {
    let mut env = scrubbed_env(root);
    for (arg, role) in &plan.argv {
        if *role != Role::Env {
            continue;
        }
        let (key, value) = arg
            .split_once('=')
            .ok_or_else(|| format!("env role expected KEY=VALUE, got `{arg}`"))?;
        let value = if env_path_value_should_be_physical(key) || value.starts_with('/') {
            physical_path(value, root)?.into_os_string()
        } else {
            OsString::from(value)
        };
        env.retain(|(existing, _)| existing.to_str() != Some(key));
        env.push((OsString::from(key), value));
    }
    Ok(env)
}

fn env_path_value_should_be_physical(key: &str) -> bool {
    matches!(key, "CARGO_MANIFEST_DIR" | "OUT_DIR")
}

fn build_script_current_dir(plan: &ExecPlan, root: &Path) -> Result<PathBuf, String> {
    for (arg, role) in &plan.argv {
        if *role != Role::Env {
            continue;
        }
        let Some((key, value)) = arg.split_once('=') else {
            continue;
        };
        if key == "CARGO_MANIFEST_DIR" {
            return physical_path(value, root);
        }
    }
    Ok(root.to_path_buf())
}

fn configure_build_script_toolchain_env(
    plan: &ExecPlan,
    root: &Path,
    env: &mut Vec<(OsString, OsString)>,
) -> Result<(), String> {
    if build_script_requires_cc(plan) {
        let identity = probe_cc_toolchain()?;
        set_env(env, "CC", identity.path.into_os_string());
        return Ok(());
    }

    let trap_dir = root.join(".vix-undeclared-toolchain").join("bin");
    fs::create_dir_all(&trap_dir)
        .map_err(|err| format!("real-process create undeclared toolchain trap: {err}"))?;
    for program in ["cc", "gcc", "clang", "c++", "g++", "clang++"] {
        let trap = trap_dir.join(program);
        fs::write(
            &trap,
            "#!/bin/sh\necho 'vix real-process: undeclared C-toolchain capability' >&2\nexit 127\n",
        )
        .map_err(|err| format!("real-process write undeclared toolchain trap: {err}"))?;
        make_executable(&trap)?;
    }

    set_env(env, "CC", trap_dir.join("cc").into_os_string());
    set_env(env, "CXX", trap_dir.join("c++").into_os_string());
    prepend_env_path(env, &trap_dir);
    Ok(())
}

fn set_env(env: &mut Vec<(OsString, OsString)>, key: &str, value: OsString) {
    env.retain(|(existing, _)| existing.to_str() != Some(key));
    env.push((OsString::from(key), value));
}

fn prepend_env_path(env: &mut Vec<(OsString, OsString)>, dir: &Path) {
    let mut value = OsString::from(dir);
    if let Some((_, existing)) = env.iter().find(|(key, _)| key.to_str() == Some("PATH"))
        && !existing.is_empty()
    {
        value.push(":");
        value.push(existing);
    }
    set_env(env, "PATH", value);
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

fn map_argv(
    command: &str,
    plan: &ExecPlan,
    world: &ObservedWorld<'_>,
    root: &Path,
) -> Result<Vec<OsString>, String> {
    let mut argv = Vec::new();
    for (arg, role) in &plan.argv {
        if is_control_arg(arg, *role) {
            continue;
        }
        if command == "ar"
            && *role == Role::Input
            && world.peek_bytes(arg).is_none()
            && let Some(names) = world.peek_list(arg)
        {
            for name in names {
                let logical = format!("{}/{}", arg.trim_end_matches('/'), name);
                argv.push(physical_path(&logical, root)?.into_os_string());
            }
            continue;
        }
        argv.push(map_arg(arg, *role, root)?);
    }
    Ok(argv)
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

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|err| {
            format!(
                "real-process executable metadata `{}`: {err}",
                path.display()
            )
        })?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o700);
    fs::set_permissions(path, permissions)
        .map_err(|err| format!("real-process chmod executable `{}`: {err}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

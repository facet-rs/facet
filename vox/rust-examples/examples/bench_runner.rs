use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{JoinHandle, sleep};
use std::time::Duration;

use sysinfo::{Pid, ProcessesToUpdate, System};

use eyre::{Context as _, Result};

#[derive(Debug, Clone)]
struct Config {
    addr: String,
    subject_mode: String,
    subject_cmd: PathBuf,
    bench_client_cmd: Option<PathBuf>,
    bench_client_args: Vec<OsString>,
    samply: bool,
}

fn workspace_root() -> Result<PathBuf> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| eyre::eyre!("rust-examples crate must live under the workspace root"))?
        .to_path_buf())
}

fn default_bench_client_cmd() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("failed to determine current executable path")?;
    let dir = exe
        .parent()
        .ok_or_else(|| eyre::eyre!("current executable has no parent directory"))?;
    let bin_name = if cfg!(windows) {
        "bench_client.exe"
    } else {
        "bench_client"
    };
    Ok(dir.join(bin_name))
}

fn default_subject_cmd() -> Result<PathBuf> {
    Ok(workspace_root()?
        .join("swift")
        .join("subject")
        .join("subject-swift.sh"))
}

fn parse_config() -> Result<Config> {
    let mut subject_cmd = default_subject_cmd()?;
    let mut subject_mode = "server".to_string();
    let mut bench_client_cmd = None;
    let mut bench_client_args = Vec::<OsString>::new();
    let mut addr = "local:///tmp/bench.vox".to_string();
    let mut samply = false;

    let mut positionals = Vec::<String>::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--" => {
                bench_client_args.extend(args.map(OsString::from));
                break;
            }
            "--subject-cmd" => {
                let value = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --subject-cmd"))?;
                subject_cmd = PathBuf::from(value);
            }
            "--subject-mode" => {
                subject_mode = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --subject-mode"))?;
            }
            "--bench-client-cmd" => {
                let value = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for --bench-client-cmd"))?;
                bench_client_cmd = Some(PathBuf::from(value));
            }
            "--count" | "--payload-size" | "--payload-sizes" | "--in-flight" | "--in-flights"
            | "--addr" => {
                let value = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing value for {arg}"))?;
                if arg == "--addr" {
                    addr = value.clone();
                }
                bench_client_args.push(OsString::from(arg));
                bench_client_args.push(OsString::from(value));
            }
            "--json" => {
                bench_client_args.push(OsString::from(arg));
            }
            "--samply" => {
                samply = true;
            }
            _ if arg.starts_with("--") => {
                return Err(eyre::eyre!("unknown flag: {arg}"));
            }
            _ => {
                positionals.push(arg.clone());
                bench_client_args.push(OsString::from(arg));
            }
        }
    }

    if let Some(pos_addr) = positionals.get(1) {
        addr = pos_addr.clone();
    }

    let workspace_root = workspace_root()?;
    if subject_cmd.is_relative() {
        subject_cmd = workspace_root.join(subject_cmd);
    }
    if let Some(path) = bench_client_cmd.as_mut()
        && path.is_relative()
    {
        *path = workspace_root.join(path.clone());
    }

    Ok(Config {
        addr,
        subject_mode,
        subject_cmd,
        bench_client_cmd,
        bench_client_args,
        samply,
    })
}

fn current_profile() -> &'static str {
    let exe = std::env::current_exe().ok();
    if exe.as_ref().is_some_and(|path| {
        path.components()
            .any(|component| component.as_os_str() == "release")
    }) {
        "release"
    } else {
        "debug"
    }
}

fn build_bench_client(workspace_root: &Path) -> Result<PathBuf> {
    let mut cargo = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    cargo.current_dir(workspace_root);
    cargo.args([
        "build",
        "--quiet",
        "-p",
        "rust-examples",
        "--example",
        "bench_client",
    ]);
    if current_profile() == "release" {
        cargo.arg("--release");
    }
    let status = cargo
        .status()
        .context("failed to build bench_client example")?;
    if !status.success() {
        return Err(eyre::eyre!(
            "cargo build for bench_client failed with {status}"
        ));
    }
    default_bench_client_cmd()
}

fn resolve_bench_client_cmd(
    workspace_root: &Path,
    configured: &Option<PathBuf>,
) -> Result<PathBuf> {
    if let Some(path) = configured {
        return Ok(path.clone());
    }

    let exe = default_bench_client_cmd()?;
    if exe.exists() {
        return Ok(exe);
    }

    build_bench_client(workspace_root)
}

fn local_socket_path(addr: &str) -> Option<PathBuf> {
    addr.strip_prefix("local://").map(PathBuf::from)
}

fn read_rss_kib(pid: u32, sys: &mut System) -> Option<u64> {
    let pid = Pid::from(pid as usize);
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), false);
    Some(sys.process(pid)?.memory() / 1024)
}

#[derive(Debug, Clone, Copy)]
struct PeakMemory {
    peak_rss_kib: u64,
}

struct MemorySampler {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<PeakMemory>>,
}

impl MemorySampler {
    fn start(pid: u32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_thread = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            let mut sys = System::new();
            let mut peak_rss_kib = 0u64;
            while !stop_for_thread.load(Ordering::Relaxed) {
                if let Some(rss_kib) = read_rss_kib(pid, &mut sys) {
                    peak_rss_kib = peak_rss_kib.max(rss_kib);
                }
                sleep(Duration::from_millis(100));
            }
            if let Some(rss_kib) = read_rss_kib(pid, &mut sys) {
                peak_rss_kib = peak_rss_kib.max(rss_kib);
            }
            PeakMemory { peak_rss_kib }
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }

    fn finish(&mut self) -> PeakMemory {
        self.stop.store(true, Ordering::Relaxed);
        self.handle
            .take()
            .and_then(|handle| handle.join().ok())
            .unwrap_or(PeakMemory { peak_rss_kib: 0 })
    }
}

impl Drop for MemorySampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn remove_stale_socket(addr: &str) -> Result<()> {
    let path = if let Some(path) = local_socket_path(addr) {
        path
    } else {
        return Ok(());
    };
    let mut lock_name = path.as_os_str().to_owned();
    lock_name.push(".lock");
    let lock_path = PathBuf::from(lock_name);
    for candidate in [&path, &lock_path] {
        if candidate.exists() {
            std::fs::remove_file(candidate)
                .with_context(|| format!("failed to remove stale file {}", candidate.display()))?;
        }
    }
    Ok(())
}

fn spawn_child(mut command: Command, label: &str) -> Result<Child> {
    command.stdin(Stdio::null());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());
    command
        .spawn()
        .with_context(|| format!("failed to spawn {label}"))
}

fn wait_for_socket_or_exit(child: &mut Child, path: &Path) -> Result<()> {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if path.exists() {
            return Ok(());
        }

        if let Some(status) = child.try_wait().context("failed to poll bench_client")? {
            return Err(exit_error("bench_client", status));
        }

        if std::time::Instant::now() >= deadline {
            return Err(eyre::eyre!(
                "timed out waiting for {} to appear",
                path.display()
            ));
        }

        sleep(Duration::from_millis(25));
    }
}

fn exit_error(label: &str, status: ExitStatus) -> eyre::Report {
    eyre::eyre!("{label} exited with status {status}")
}

fn wait_for_exit(child: &mut Child, label: &str, timeout: Duration) -> Result<Option<ExitStatus>> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("failed to poll {label}"))?
        {
            return Ok(Some(status));
        }
        if std::time::Instant::now() >= deadline {
            return Ok(None);
        }
        sleep(Duration::from_millis(25));
    }
}

struct ChildGuard {
    child: Option<Child>,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn child_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("child is present")
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn run() -> Result<()> {
    let cfg = parse_config()?;
    let workspace_root = workspace_root()?;
    let bench_client_cmd = resolve_bench_client_cmd(&workspace_root, &cfg.bench_client_cmd)?;

    eprintln!(
        "starting benchmark runner: addr={}, subject_cmd={}, bench_client_cmd={}",
        cfg.addr,
        cfg.subject_cmd.display(),
        bench_client_cmd.display()
    );

    remove_stale_socket(&cfg.addr)?;

    let mut bench_cmd = Command::new(&bench_client_cmd);
    bench_cmd
        .current_dir(&workspace_root)
        .args(&cfg.bench_client_args);
    let bench_client = spawn_child(bench_cmd, "bench_client")?;
    let mut bench_client = ChildGuard::new(bench_client);

    if let Some(socket_path) = local_socket_path(&cfg.addr) {
        wait_for_socket_or_exit(bench_client.child_mut(), &socket_path)?;
    } else {
        sleep(Duration::from_millis(100));
    }

    let mut subject_cmd = if cfg.samply {
        let mut cmd = Command::new("samply");
        cmd.args(["record", "--"]).arg(&cfg.subject_cmd);
        cmd
    } else {
        Command::new(&cfg.subject_cmd)
    };
    subject_cmd
        .current_dir(&workspace_root)
        .env("SUBJECT_MODE", &cfg.subject_mode)
        .env("PEER_ADDR", &cfg.addr);
    let subject = spawn_child(subject_cmd, "subject")?;
    let mut subject = ChildGuard::new(subject);
    let mut subject_memory = MemorySampler::start(subject.child_mut().id());

    // Fail fast if the subject exits before the bench client is done.
    loop {
        if let Some(status) = bench_client
            .child_mut()
            .try_wait()
            .context("failed to poll bench_client")?
        {
            if !status.success() {
                return Err(exit_error("bench_client", status));
            }
            break;
        }
        if let Some(status) = subject
            .child_mut()
            .try_wait()
            .context("failed to poll subject")?
        {
            return Err(exit_error("subject", status));
        }
        sleep(Duration::from_millis(25));
    }

    if cfg.samply {
        eprintln!("waiting for samply to exit (take your time)...");
        let status = subject
            .child_mut()
            .wait()
            .context("failed to wait for samply")?;
        if !status.success() {
            return Err(exit_error("samply", status));
        }
    } else {
        match wait_for_exit(subject.child_mut(), "subject", Duration::from_secs(1))? {
            Some(status) if status.success() => {}
            Some(status) => return Err(exit_error("subject", status)),
            None => {
                eprintln!("subject did not exit promptly; terminating it");
                let _ = subject.child_mut().kill();
                let _ = subject.child_mut().wait();
            }
        }
    }
    let peak_memory = subject_memory.finish();
    let peak_rss_kib = peak_memory.peak_rss_kib;
    eprintln!(
        "subject peak_rss_kib={} peak_rss_mib={:.2}",
        peak_rss_kib,
        peak_rss_kib as f64 / 1024.0,
    );

    if let Some(path) = local_socket_path(&cfg.addr) {
        let _ = std::fs::remove_file(path);
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

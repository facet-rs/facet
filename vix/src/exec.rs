//! The exec model: TWO-TIER CACHE KEYS at the language/runtime seam.
//!
//! The seam, precisely:
//!   - the RUNTIME answers questions: "what happened when I ran this" (the
//!     observed read-set, from the VFS) and "what is the world right now"
//!     (point queries: read a path, list a dir). It also enforces the sandbox
//!     ceiling and moves bytes. All of that is proprietary machinery.
//!   - the LANGUAGE owns correctness: what a key IS, when a cached result may
//!     be reused, what an observation means. `verify()` — "does this read-set
//!     still hold" — lives HERE so that false-positive-never cannot be
//!     implemented wrong by a runtime. A buggy runtime causes false negatives
//!     (rebuild too much, safe), never false positives.
//!   - what CROSSES the seam is pure facet data: ExecPlan, Mount, ReadSet,
//!     Outcome — plus two small traits (Snapshot, Tool).
//!
//! The two tiers:
//!   - tier 1: closure × plan × capability fingerprint × COARSE mount
//!     fingerprints. Computable before running; exact over everything
//!     declared, so it never false-positives — it just over-invalidates
//!     ("this folder on that machine changed SOMEWHERE").
//!   - tier 2: on a tier-1 miss, prior outcomes for the same plan × capability
//!     are candidates; each carries the read-set OBSERVED when it ran —
//!     content reads, directory listings, and NEGATIVE lookups. If the
//!     current world verifies the read-set, the outcome is reused without
//!     running. Nix is tier-1 only; tier 2 is the anti-Nix pillar.
//!
//! Command grammars map onto this as the ceiling/predicted/observed triple:
//! grammar-extracted ROLES declare the mount ceiling (what you MAY touch) and
//! predict the read-set (what you'll LIKELY touch); the VFS observes what you
//! DID touch, which becomes the pin that refines the key. The `Role` enum
//! here is the grammar's output shape; the toy role extraction in the oracle
//! stands in for the real snark command grammars.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};

// ---------------------------------------------------------------------------
// Data that crosses the seam (all facet, all open).
// ---------------------------------------------------------------------------

/// A content-addressed tree: path -> contents. (The oracle's stand-in for the
/// CAS. Text entries keep the fake lane readable; blob entries carry real
/// process artifacts without lossy UTF-8 conversion.
#[derive(facet::Facet, Debug, Clone, Default, PartialEq, Eq)]
pub struct Tree {
    pub entries: BTreeMap<String, String>,
    pub blobs: BTreeMap<String, Vec<u8>>,
}

impl Tree {
    pub fn of(entries: &[(&str, &str)]) -> Tree {
        Tree {
            entries: entries
                .iter()
                .map(|(p, c)| (p.to_string(), c.to_string()))
                .collect(),
            blobs: BTreeMap::new(),
        }
    }

    pub fn of_blobs(entries: &[(&str, &[u8])]) -> Tree {
        Tree {
            entries: BTreeMap::new(),
            blobs: entries
                .iter()
                .map(|(p, c)| (p.to_string(), c.to_vec()))
                .collect(),
        }
    }

    pub fn insert_bytes(&mut self, path: impl Into<String>, contents: Vec<u8>) {
        let path = path.into();
        match String::from_utf8(contents) {
            Ok(text) => {
                self.entries.insert(path, text);
            }
            Err(err) => {
                self.blobs.insert(path, err.into_bytes());
            }
        }
    }

    pub fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.entries
            .get(path)
            .map(|contents| contents.as_bytes().to_vec())
            .or_else(|| self.blobs.get(path).cloned())
    }

    pub fn display_entries(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .entries
            .iter()
            .map(|(path, contents)| (path.clone(), contents.clone()))
            .collect();
        out.extend(
            self.blobs
                .iter()
                .map(|(path, contents)| (path.clone(), format!("<blob:{} bytes>", contents.len()))),
        );
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// The TIER-1 fingerprint: everything, coarsely. Any change anywhere in
    /// the tree changes it — that's the point (and the imprecision tier 2
    /// exists to refine).
    pub fn fingerprint(&self) -> u64 {
        let mut h = DefaultHasher::new();
        for (path, contents) in &self.entries {
            0u8.hash(&mut h);
            path.hash(&mut h);
            contents.hash(&mut h);
        }
        for (path, contents) in &self.blobs {
            1u8.hash(&mut h);
            path.hash(&mut h);
            contents.hash(&mut h);
        }
        h.finish()
    }
}

/// A tree mounted at a path — the sandbox ceiling is exactly the mount set.
#[derive(facet::Facet, Debug, Clone)]
pub struct Mount {
    pub at: String,
    pub tree: Tree,
}

/// What a command-grammar role says an argv element IS. Grammar-extracted in
/// the real system; the seam only cares about this shape.
#[derive(facet::Facet, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Role {
    /// A path the tool will read.
    Input,
    /// A path the tool will write.
    Output,
    /// A directory the tool will PROBE (search paths — the negative-lookup
    /// factory).
    SearchDir,
    /// Behavior-changing but not path-shaped.
    Flag,
}

/// A resolved command: argv with roles. This is what a `cc! { … }` block
/// becomes after splices evaluate and the command grammar assigns roles.
#[derive(facet::Facet, Debug, Clone)]
pub struct ExecPlan {
    pub argv: Vec<(String, Role)>,
}

impl ExecPlan {
    pub fn hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        for (arg, role) in &self.argv {
            arg.hash(&mut h);
            (*role as u8).hash(&mut h);
        }
        h.finish()
    }

    /// The NORMALIZED plan: canonical argv ordering, so equivalent
    /// invocations share cache identity — `cc -c x.c -O2` == `cc -O2 -c x.c`.
    ///
    /// Toy stand-in for the snark command grammars (synthesis §3⅞): the real
    /// grammar marks which argument classes commute; here, standalone
    /// behavior flags (-O*, -W*, -D*) commute and sort among their own
    /// positions, while everything position-sensitive (inputs, -o/-I pairs,
    /// search order) stays put.
    pub fn normalized(&self) -> ExecPlan {
        let commutes = |i: usize| -> bool {
            let (arg, role) = &self.argv[i];
            if *role != Role::Flag {
                return false;
            }
            // A flag that OWNS the next argument (-o out, -I dir) or is
            // owned by the previous one must not move.
            if arg == "-o" || arg == "-I" {
                return false;
            }
            if i > 0 {
                let prev = &self.argv[i - 1].0;
                if prev == "-o" || prev == "-I" {
                    return false;
                }
            }
            arg.starts_with("-O") || arg.starts_with("-W") || arg.starts_with("-D")
        };
        let slots: Vec<usize> = (0..self.argv.len()).filter(|&i| commutes(i)).collect();
        let mut movable: Vec<(String, Role)> =
            slots.iter().map(|&i| self.argv[i].clone()).collect();
        movable.sort_by(|a, b| a.0.cmp(&b.0));
        let mut argv = self.argv.clone();
        for (&slot, item) in slots.iter().zip(movable) {
            argv[slot] = item;
        }
        ExecPlan { argv }
    }

    /// Identity hash: the normalized plan's hash. Cache keys are
    /// semantics-shaped, not byte-shaped.
    pub fn identity_hash(&self) -> u64 {
        self.normalized().hash()
    }
}

/// The command grammar's important-first description of an invocation:
/// level 0 = verb + object, deeper levels = modifiers, last = full argv.
///
/// Toy stand-in for the snark command grammars (synthesis §3⅞), which will
/// derive this from salience queries over the parsed invocation.
pub fn describe(command: &str, plan: &ExecPlan) -> Vec<String> {
    let base = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();
    let output = plan
        .argv
        .iter()
        .find(|(_, r)| *r == Role::Output)
        .map(|(a, _)| base(a));
    let inputs: Vec<String> = plan
        .argv
        .iter()
        .filter(|(_, r)| *r == Role::Input)
        .map(|(a, _)| base(a))
        .collect();
    let flags: Vec<&str> = plan
        .argv
        .iter()
        .filter(|(a, r)| *r == Role::Flag && a.starts_with('-'))
        .map(|(a, _)| a.as_str())
        .collect();
    let search: Vec<String> = plan
        .argv
        .iter()
        .filter(|(_, r)| *r == Role::SearchDir)
        .map(|(a, _)| a.clone())
        .collect();

    let level0 = match command {
        "cc" if flags.contains(&"-c") => format!(
            "compile {} → {}",
            inputs.join(", "),
            output.clone().unwrap_or_else(|| "?".into())
        ),
        "cc" => format!(
            "link {} → {}",
            inputs.join(" + "),
            output.clone().unwrap_or_else(|| "?".into())
        ),
        "ar" => format!(
            "archive {} object{} → {}",
            inputs.len(),
            if inputs.len() == 1 { "" } else { "s" },
            output.clone().unwrap_or_else(|| "?".into())
        ),
        _ => format!(
            "{command} {} → {}",
            inputs.join(", "),
            output.clone().unwrap_or_else(|| "?".into())
        ),
    };

    let mut level1 = Vec::new();
    if flags.contains(&"-O2") {
        level1.push("optimized (-O2)".to_string());
    }
    if flags.contains(&"-Wall") {
        level1.push("all warnings".to_string());
    }
    let defines: Vec<&str> = flags
        .iter()
        .filter(|f| f.starts_with("-D"))
        .copied()
        .collect();
    if !defines.is_empty() {
        level1.push(format!("defines {}", defines.join(" ")));
    }
    if !search.is_empty() {
        level1.push(format!("headers from {}", search.join(", ")));
    }

    let full = plan
        .argv
        .iter()
        .map(|(a, _)| a.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    let mut out = vec![level0];
    if !level1.is_empty() {
        out.push(level1.join(", "));
    }
    out.push(format!("{command} {full}"));
    out
}

/// One observed interaction with the world. Deletions are observable because
/// ABSENCE is observable: a negative lookup pins "this path did not exist".
#[derive(facet::Facet, Debug, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum ReadObservation {
    /// The path existed with this content hash.
    Content(u64),
    /// The path did NOT exist (a probe missed). The entry every mtime/depfile
    /// system forgets — and the reason `make` misses a new header shadowing
    /// an old one earlier in the search path.
    Absent,
    /// A directory was enumerated; the hash covers the sorted name list, so
    /// additions AND deletions diverge it.
    Listing(u64),
}

/// Exactly what a run touched. BTreeMap: canonical order, hashable, diffable.
#[derive(facet::Facet, Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadSet {
    pub entries: BTreeMap<String, ReadObservation>,
}

/// A finished run: outputs plus the observed read-set.
#[derive(facet::Facet, Debug, Clone)]
pub struct Outcome {
    pub outputs: Tree,
    pub read_set: ReadSet,
}

// ---------------------------------------------------------------------------
// The seam traits.
// ---------------------------------------------------------------------------

/// Point queries about the world — what the RUNTIME answers. The language
/// never sees a filesystem; it sees this.
pub trait Snapshot {
    fn read(&self, path: &str) -> Option<String>;
    fn read_bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.read(path).map(String::into_bytes)
    }
    fn list(&self, dir: &str) -> Option<Vec<String>>;
}

/// A tool the executor can run (the oracle's stand-in for spawning inside the
/// sandbox). It only sees the world through the observing snapshot — that IS
/// the sandbox: reads outside the mounts answer None, and everything answers
/// into the read-set.
pub trait Tool {
    fn run(&self, plan: &ExecPlan, world: &mut ObservedWorld<'_>) -> Result<Tree, String>;
}

fn content_hash(s: &str) -> u64 {
    content_hash_bytes(s.as_bytes())
}

fn content_hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

fn listing_hash(names: &[String]) -> u64 {
    let mut h = DefaultHasher::new();
    for n in names {
        n.hash(&mut h);
    }
    h.finish()
}

/// The mounted world: point queries answered from the mount set, nothing
/// else — the CEILING. (Language-side here because the oracle's world is made
/// of Tree values; the real one is the proprietary VFS behind this trait.)
pub struct MountedWorld<'a> {
    mounts: &'a [Mount],
}

impl<'a> MountedWorld<'a> {
    pub fn new(mounts: &'a [Mount]) -> Self {
        MountedWorld { mounts }
    }
}

impl Snapshot for MountedWorld<'_> {
    fn read(&self, path: &str) -> Option<String> {
        for m in self.mounts {
            if let Some(rest) = path.strip_prefix(&m.at)
                && let Some(contents) = m.tree.entries.get(rest.trim_start_matches('/'))
            {
                return Some(contents.clone());
            }
        }
        None
    }

    fn read_bytes(&self, path: &str) -> Option<Vec<u8>> {
        for m in self.mounts {
            if let Some(rest) = path.strip_prefix(&m.at) {
                let key = rest.trim_start_matches('/');
                if let Some(contents) = m.tree.entries.get(key) {
                    return Some(contents.as_bytes().to_vec());
                }
                if let Some(contents) = m.tree.blobs.get(key) {
                    return Some(contents.clone());
                }
            }
        }
        None
    }

    fn list(&self, dir: &str) -> Option<Vec<String>> {
        for m in self.mounts {
            if let Some(rest) = dir.strip_prefix(&m.at) {
                let prefix = rest.trim_start_matches('/');
                let names: BTreeSet<String> = m
                    .tree
                    .entries
                    .keys()
                    .chain(m.tree.blobs.keys())
                    .filter(|k| {
                        prefix.is_empty()
                            || k.strip_prefix(prefix).is_some_and(|r| r.starts_with('/'))
                    })
                    .cloned()
                    .collect();
                if !names.is_empty() || prefix.is_empty() {
                    return Some(names.into_iter().collect());
                }
            }
        }
        None
    }
}

/// The observation wrapper: every query a tool makes lands in the read-set —
/// including the ones that MISS. Recording semantics are language-owned
/// because they define what "same inputs" MEANS.
pub struct ObservedWorld<'a> {
    world: &'a dyn Snapshot,
    read_set: ReadSet,
}

impl<'a> ObservedWorld<'a> {
    pub fn new(world: &'a dyn Snapshot) -> Self {
        ObservedWorld {
            world,
            read_set: ReadSet::default(),
        }
    }

    pub fn read(&mut self, path: &str) -> Option<String> {
        let got = self.world.read(path);
        let obs = match &got {
            Some(contents) => ReadObservation::Content(content_hash(contents)),
            None => ReadObservation::Absent,
        };
        self.read_set.entries.insert(path.to_string(), obs);
        got
    }

    pub fn read_bytes(&mut self, path: &str) -> Option<Vec<u8>> {
        let got = self.world.read_bytes(path);
        let obs = match &got {
            Some(contents) => ReadObservation::Content(content_hash_bytes(contents)),
            None => ReadObservation::Absent,
        };
        self.read_set.entries.insert(path.to_string(), obs);
        got
    }

    pub fn peek_bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.world.read_bytes(path)
    }

    pub fn peek_list(&self, dir: &str) -> Option<Vec<String>> {
        self.world.list(dir)
    }

    pub fn list(&mut self, dir: &str) -> Option<Vec<String>> {
        let got = self.world.list(dir);
        let obs = match &got {
            Some(names) => ReadObservation::Listing(listing_hash(names)),
            None => ReadObservation::Absent,
        };
        self.read_set.entries.insert(format!("{dir}/"), obs);
        got
    }

    pub fn into_read_set(self) -> ReadSet {
        self.read_set
    }
}

/// THE language-owned correctness core: does a previously observed read-set
/// still hold in this world? Exact, entry by entry — content hashes equal,
/// absences still absent, listings unchanged. This is what makes tier-2 reuse
/// never a false positive.
pub fn verify(read_set: &ReadSet, world: &dyn Snapshot) -> bool {
    read_set.entries.iter().all(|(path, obs)| match obs {
        ReadObservation::Content(hash) => world
            .read_bytes(path)
            .is_some_and(|c| content_hash_bytes(&c) == *hash),
        ReadObservation::Absent => {
            if let Some(dir) = path.strip_suffix('/') {
                world.list(dir).is_none()
            } else {
                world.read(path).is_none()
            }
        }
        ReadObservation::Listing(hash) => {
            let dir = path.strip_suffix('/').unwrap_or(path);
            world
                .list(dir)
                .is_some_and(|names| listing_hash(&names) == *hash)
        }
    })
}

// ---------------------------------------------------------------------------
// The two-tier cache.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecEvent {
    /// Served by the coarse key: nothing declared changed at all.
    Tier1Hit,
    /// Coarse key missed, but the observed read-set verified against the new
    /// world: reused WITHOUT running (the anti-Nix event).
    Tier2Cutoff { verified: usize },
    /// Ran the tool.
    Ran,
    /// Attached to an identical live run (wire executors only — the local
    /// cache is single-demand).
    Joined,
}

pub struct ExecCache {
    /// tier-1: (closure × plan × capability × coarse mounts) -> outcome.
    tier1: HashMap<u64, Outcome>,
    /// tier-2 candidates: (closure × plan × capability) -> prior outcomes.
    /// Mount fingerprints are EXCLUDED here — the world is what tier 2
    /// relaxes; the computation identity is what it must not.
    candidates: HashMap<u64, Vec<Outcome>>,
    pub events: Vec<ExecEvent>,
}

impl Default for ExecCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecCache {
    pub fn new() -> Self {
        ExecCache {
            tier1: HashMap::new(),
            candidates: HashMap::new(),
            events: Vec::new(),
        }
    }

    fn keys(&self, plan: &ExecPlan, capability: u64, mounts: &[Mount]) -> (u64, u64) {
        let mut h = DefaultHasher::new();
        // NORMALIZED plan identity: equivalent invocations share both tiers
        // (`cc -c x.c -O2` == `cc -O2 -c x.c`). See ExecPlan::normalized.
        plan.identity_hash().hash(&mut h);
        capability.hash(&mut h);
        let identity = h.finish();

        let mut h = DefaultHasher::new();
        identity.hash(&mut h);
        for m in mounts {
            m.at.hash(&mut h);
            m.tree.fingerprint().hash(&mut h);
        }
        (identity, h.finish())
    }

    /// Run (or reuse) a plan against mounted trees under a capability.
    pub fn exec(
        &mut self,
        plan: &ExecPlan,
        capability: u64,
        mounts: &[Mount],
        tool: &dyn Tool,
    ) -> Result<Outcome, String> {
        let (identity, coarse) = self.keys(plan, capability, mounts);

        if let Some(hit) = self.tier1.get(&coarse) {
            self.events.push(ExecEvent::Tier1Hit);
            return Ok(hit.clone());
        }

        let world = MountedWorld::new(mounts);
        if let Some(prior) = self.candidates.get(&identity) {
            for outcome in prior {
                if verify(&outcome.read_set, &world) {
                    self.events.push(ExecEvent::Tier2Cutoff {
                        verified: outcome.read_set.entries.len(),
                    });
                    // Re-pin under the new coarse key: next time is tier-1.
                    self.tier1.insert(coarse, outcome.clone());
                    return Ok(outcome.clone());
                }
            }
        }

        let mut observed = ObservedWorld::new(&world);
        let outputs = tool.run(plan, &mut observed)?;
        let outcome = Outcome {
            outputs,
            read_set: observed.into_read_set(),
        };
        self.events.push(ExecEvent::Ran);
        self.tier1.insert(coarse, outcome.clone());
        self.candidates
            .entry(identity)
            .or_default()
            .push(outcome.clone());
        Ok(outcome)
    }
}

// ---------------------------------------------------------------------------
// A fake compiler: enough behavior to generate REAL read-set dynamics.
// Reads its inputs; for every `#include "x"` line, probes the search dirs in
// order (negative lookups!); "compiles" by hashing everything it read.
// ---------------------------------------------------------------------------

pub struct FakeCc;

impl Tool for FakeCc {
    fn run(&self, plan: &ExecPlan, world: &mut ObservedWorld<'_>) -> Result<Tree, String> {
        let search_dirs: Vec<&str> = plan
            .argv
            .iter()
            .filter(|(_, r)| *r == Role::SearchDir)
            .map(|(a, _)| a.as_str())
            .collect();
        let output = plan
            .argv
            .iter()
            .find(|(_, r)| *r == Role::Output)
            .map(|(a, _)| a.clone())
            .ok_or("cc: no output")?;

        let mut digest = DefaultHasher::new();
        for (arg, role) in &plan.argv {
            if *role == Role::Flag {
                arg.hash(&mut digest);
            }
        }
        for (input, _) in plan.argv.iter().filter(|(_, r)| *r == Role::Input) {
            let source = world
                .read(input)
                .ok_or_else(|| format!("cc: cannot read `{input}` (outside the mounts?)"))?;
            source.hash(&mut digest);
            // Quoted includes probe the including file's own directory FIRST
            // (C semantics), then the -I dirs, in order. Misses pin Absent.
            let own_dir = input.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            for line in source.lines() {
                if let Some(header) = line
                    .trim()
                    .strip_prefix("#include \"")
                    .and_then(|r| r.strip_suffix('"'))
                {
                    let mut probes: Vec<String> = Vec::new();
                    if !own_dir.is_empty() {
                        probes.push(format!("{own_dir}/{header}"));
                    }
                    probes.extend(search_dirs.iter().map(|d| format!("{d}/{header}")));
                    let mut found = false;
                    for probe in &probes {
                        if let Some(contents) = world.read(probe) {
                            contents.hash(&mut digest);
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return Err(format!("cc: header `{header}` not found"));
                    }
                }
            }
        }

        Ok(Tree::of(&[(
            output.as_str(),
            &format!("obj({:016x})", digest.finish()),
        )]))
    }
}

/// A fake rustc sibling for plan/cache tests. It reads its source inputs and
/// emits the requested artifact, but it does not spawn rustc or model Cargo.
pub struct FakeRustc;

impl Tool for FakeRustc {
    fn run(&self, plan: &ExecPlan, world: &mut ObservedWorld<'_>) -> Result<Tree, String> {
        let output = plan
            .argv
            .iter()
            .find(|(_, role)| *role == Role::Output)
            .map(|(arg, _)| arg.clone())
            .ok_or("rustc: no output")?;
        let mut digest = DefaultHasher::new();
        let mut crate_type = "lib";
        for (index, (arg, role)) in plan.argv.iter().enumerate() {
            match role {
                Role::Flag => {
                    arg.hash(&mut digest);
                    if index > 0 && plan.argv[index - 1].0 == "--crate-type" {
                        crate_type = arg;
                    }
                }
                Role::SearchDir => {
                    arg.hash(&mut digest);
                }
                Role::Output => {}
                Role::Input => {
                    let source = world.read(arg).ok_or_else(|| {
                        format!("rustc: cannot read `{arg}` (outside the mounts?)")
                    })?;
                    source.hash(&mut digest);
                }
            }
        }

        let kind = if crate_type == "bin" { "bin" } else { "rlib" };
        Ok(Tree::of(&[(
            output.as_str(),
            &format!("{kind}({:016x})", digest.finish()),
        )]))
    }
}

/// A fake archiver/linker sibling: concatenates what it reads. Enough to
/// exercise OUTPUTS-AS-MOUNTS composition (`ar!` consuming `cc!` products).
pub struct FakeAr;

impl Tool for FakeAr {
    fn run(&self, plan: &ExecPlan, world: &mut ObservedWorld<'_>) -> Result<Tree, String> {
        let output = plan
            .argv
            .iter()
            .find(|(_, r)| *r == Role::Output)
            .map(|(a, _)| a.clone())
            .ok_or("ar: no output")?;
        let mut digest = DefaultHasher::new();
        for (arg, role) in &plan.argv {
            if *role == Role::Flag {
                arg.hash(&mut digest);
            }
        }
        for (input, _) in plan.argv.iter().filter(|(_, r)| *r == Role::Input) {
            if let Some(contents) = world.read(input) {
                contents.hash(&mut digest);
                continue;
            }
            // A directory input: enumerate it (a LISTING observation — new
            // members will diverge the pin) and archive every file.
            let names = world
                .list(input)
                .ok_or_else(|| format!("ar: cannot read `{input}` (outside the mounts?)"))?;
            for name in names {
                let contents = world
                    .read(&format!("{input}/{name}"))
                    .ok_or_else(|| format!("ar: `{input}/{name}` vanished mid-run"))?;
                contents.hash(&mut digest);
            }
        }
        Ok(Tree::of(&[(
            output.as_str(),
            &format!("archive({:016x})", digest.finish()),
        )]))
    }
}

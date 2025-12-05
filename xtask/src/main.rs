//! xtask for facet workspace

use std::{
    env, fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("showcases") => generate_showcases(),
        Some("schema-build") => schema_build(&args[1..]),
        Some("schema") => generate_schema(),
        Some("help") | None => print_help(),
        Some(cmd) => {
            eprintln!("Unknown command: {cmd}");
            eprintln!();
            print_help();
            std::process::exit(1);
        }
    }
}

fn print_help() {
    eprintln!("Usage: cargo xtask <command>");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  showcases    Generate all showcase markdown files for the website");
    eprintln!("  schema       Generate deterministic schema set for bloat/compile benches");
    eprintln!("  schema-build Generate schema, then build facet/serde variants (debug or release)");
    eprintln!("  help         Show this help message");
}

fn generate_showcases() {
    let workspace_root = workspace_root();
    let output_dir = workspace_root.join("docs/content/guide/showcases");

    fs::create_dir_all(&output_dir).expect("Failed to create output directory");

    // Find all *_showcase.rs examples
    let mut showcases = Vec::new();
    for entry in fs::read_dir(&workspace_root).expect("Failed to read workspace root") {
        let entry = entry.expect("Failed to read entry");
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let examples_dir = path.join("examples");
        if !examples_dir.exists() {
            continue;
        }

        let pkg_name = path.file_name().unwrap().to_str().unwrap().to_string();

        for example in fs::read_dir(&examples_dir).expect("Failed to read examples dir") {
            let example = example.expect("Failed to read example");
            let example_path = example.path();

            if let Some(name) = example_path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with("_showcase.rs") {
                    let example_name = name.trim_end_matches(".rs").to_string();
                    let output_name = example_name.trim_end_matches("_showcase").to_string();
                    showcases.push((pkg_name.clone(), example_name, output_name));
                }
            }
        }
    }

    showcases.sort();

    let total = showcases.len();
    println!("Generating {total} showcases in parallel...");

    // Channel to collect results
    let (tx, rx) = mpsc::channel();

    // Spawn threads for each showcase
    let handles: Vec<_> = showcases
        .into_iter()
        .map(|(pkg, example, output)| {
            let tx = tx.clone();
            let output_dir = output_dir.clone();

            thread::spawn(move || {
                let output_path = output_dir.join(format!("{output}.md"));

                let result = Command::new("cargo")
                    .args(["run", "-p", &pkg, "--example", &example, "--all-features"])
                    .env("FACET_SHOWCASE_OUTPUT", "markdown")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output();

                let status = match result {
                    Ok(output_result) if output_result.status.success() => {
                        fs::write(&output_path, &output_result.stdout)
                            .expect("Failed to write output file");
                        Ok(())
                    }
                    Ok(output_result) => {
                        let stderr = String::from_utf8_lossy(&output_result.stderr);
                        Err(stderr.lines().take(10).collect::<Vec<_>>().join("\n"))
                    }
                    Err(e) => Err(e.to_string()),
                };

                tx.send((pkg, example, output, status)).unwrap();
            })
        })
        .collect();

    // Drop the original sender so rx.iter() terminates
    drop(tx);

    // Collect and print results
    let mut successes = 0;
    let mut failures = Vec::new();

    for (pkg, example, output, status) in rx {
        match status {
            Ok(()) => {
                println!("  {pkg}::{example} -> {output}.md");
                successes += 1;
            }
            Err(e) => {
                failures.push(format!("{pkg}::{example}: {e}"));
            }
        }
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    println!();
    println!("Generated {successes}/{total} showcases");

    if !failures.is_empty() {
        println!();
        println!("Failures:");
        for failure in failures {
            println!("  {failure}");
        }
    }
}

fn workspace_root() -> PathBuf {
    let output = Command::new("cargo")
        .args(["locate-project", "--workspace", "--message-format=plain"])
        .output()
        .expect("Failed to run cargo locate-project");

    let path = String::from_utf8(output.stdout).expect("Invalid UTF-8");
    PathBuf::from(path.trim())
        .parent()
        .expect("No parent directory")
        .to_path_buf()
}

// ============================================================================
// Schema generator (bloat / compile-time benchmarks)

fn generate_schema() {
    let cfg = SchemaConfig::from_env();
    let mut generator = SchemaGenerator::new(cfg);

    let output = generator.render();

    let out_path = workspace_root()
        .join("facet-bloatbench")
        .join("src")
        .join("generated.rs");

    fs::create_dir_all(
        out_path
            .parent()
            .expect("generated.rs should have a parent directory"),
    )
    .expect("Failed to create output directory");

    fs::write(&out_path, output).expect("Failed to write generated schema");

    println!(
        "Wrote schema with {} structs and {} enums to {}",
        generator.structs.len(),
        generator.enums.len(),
        out_path.display()
    );
}

fn schema_build(args: &[String]) {
    let mut target: Option<String> = None;
    let mut release = false;
    let mut toolchain: Option<String> = None;
    let mut timings_format: Option<String> = Some("html".to_string());
    let mut also_json = false;
    let schema_rustflags = env::var("FACET_SCHEMA_RUSTFLAGS").ok();
    let mut include_json = false;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--target" => {
                if let Some(t) = iter.next() {
                    target = Some(t.clone());
                } else {
                    eprintln!("--target expects a value");
                    std::process::exit(1);
                }
            }
            "--release" => release = true,
            "--toolchain" => {
                if let Some(tc) = iter.next() {
                    toolchain = Some(tc.clone());
                } else {
                    eprintln!("--toolchain expects a value (e.g., nightly)");
                    std::process::exit(1);
                }
            }
            "--timings-format" => {
                if let Some(fmt) = iter.next() {
                    let fmt_lc = fmt.to_ascii_lowercase();
                    if fmt_lc == "html" || fmt_lc == "json" || fmt_lc == "trace" {
                        timings_format = Some(fmt_lc);
                    } else {
                        eprintln!("--timings-format must be one of: html | json | trace");
                        std::process::exit(1);
                    }
                } else {
                    eprintln!("--timings-format expects a value");
                    std::process::exit(1);
                }
            }
            "--also-json" => {
                also_json = true;
            }
            "--json" => {
                include_json = true;
            }
            other => {
                eprintln!("Unknown flag for schema-build: {other}");
                std::process::exit(1);
            }
        }
    }

    // If timings requested but no toolchain specified, default to nightly to avoid -Z errors on stable.
    if timings_format.is_some() && toolchain.is_none() {
        toolchain = Some("nightly".to_string());
    }

    let workspace = workspace_root();
    let base_target_dir = env::var("FACET_SCHEMA_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace.join("target").join("schema-build"));

    generate_schema();

    let build = |feature: &str, fmt: &str, clean: bool| {
        let mut cmd = Command::new("cargo");
        if let Some(tc) = &toolchain {
            cmd.arg(format!("+{tc}"));
        }
        cmd.arg("build")
            .arg("-p")
            .arg("facet-bloatbench")
            .arg("--no-default-features")
            .arg("--features");

        let mut feature_list = vec![feature.to_string()];
        if include_json {
            feature_list.push("json".to_string());
        }
        cmd.arg(feature_list.join(","));

        if release {
            cmd.arg("--release");
        }
        if let Some(t) = &target {
            cmd.arg("--target").arg(t);
        }

        // Separate incremental caches for facet vs serde to avoid cross-contamination
        cmd.env(
            "CARGO_TARGET_DIR",
            base_target_dir.join(feature).to_string_lossy().to_string(),
        );

        // Section timings are unstable; requires nightly.
        cmd.arg("-Z").arg("section-timings");
        match fmt {
            "html" => {
                cmd.arg("--timings");
            }
            other => {
                cmd.arg("-Z").arg("unstable-options");
                cmd.arg(format!("--timings={other}"));
            }
        }

        println!(
            "Building facet-bloatbench ({feature}, {fmt}){}{}",
            if release { " --release" } else { "" },
            target
                .as_ref()
                .map(|t| format!(" --target {t}"))
                .unwrap_or_default()
        );

        // Clean previous build directory to keep timings comparable
        let feature_target = base_target_dir.join(feature);
        if clean && feature_target.exists() {
            let _ = fs::remove_dir_all(&feature_target);
        }

        // Override RUSTFLAGS for the schema build without affecting xtask itself.
        if let Some(rf) = &schema_rustflags {
            cmd.env("RUSTFLAGS", rf);
        }

        let status = cmd.status().expect("failed to run cargo build");
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }

        if fmt == "json" {
            let timings_dir = feature_target.join("cargo-timings");
            if let Ok(entries) = fs::read_dir(&timings_dir) {
                let mut newest = None;
                for e in entries.flatten() {
                    let path = e.path();
                    if path.extension().map(|s| s == "json").unwrap_or(false) {
                        if let Ok(meta) = e.metadata() {
                            let mtime = meta.modified().ok();
                            if newest
                                .as_ref()
                                .map(|(_, t)| mtime > Some(*t))
                                .unwrap_or(true)
                            {
                                if let Some(t) = mtime {
                                    newest = Some((path.clone(), t));
                                }
                            }
                        }
                    }
                }
                if let Some((p, _)) = newest {
                    println!("Latest JSON timings: {}", p.display());
                } else {
                    println!("No JSON timings found in {}", timings_dir.display());
                }
            } else {
                println!("No JSON timings dir found at {}", timings_dir.display());
            }
        }
    };

    let fmt_primary = timings_format.clone().unwrap_or_else(|| "html".to_string());

    build("facet", &fmt_primary, true);
    if also_json && fmt_primary != "json" {
        build("facet", "json", false);
    }

    build("serde", &fmt_primary, true);
    if also_json && fmt_primary != "json" {
        build("serde", "json", false);
    }
}

#[derive(Debug, Clone, Copy)]
struct SchemaConfig {
    seed: u64,
    structs: usize,
    enums: usize,
    max_fields: usize,
    max_variants: usize,
    max_depth: usize,
}

impl SchemaConfig {
    fn from_env() -> Self {
        let parse = |key: &str, default: u64| -> u64 {
            env::var(key)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(default)
        };

        SchemaConfig {
            seed: parse("FACET_SCHEMA_SEED", 42),
            structs: parse("FACET_SCHEMA_STRUCTS", 120) as usize,
            enums: parse("FACET_SCHEMA_ENUMS", 40) as usize,
            max_fields: parse("FACET_SCHEMA_MAX_FIELDS", 12) as usize,
            max_variants: parse("FACET_SCHEMA_MAX_VARIANTS", 8) as usize,
            max_depth: parse("FACET_SCHEMA_MAX_DEPTH", 3) as usize,
        }
    }
}

#[derive(Debug, Clone)]
struct StructSpec {
    name: String,
    fields: Vec<(String, TypeSpec)>,
}

#[derive(Debug, Clone)]
struct EnumSpec {
    name: String,
    variants: Vec<VariantSpec>,
}

#[derive(Debug, Clone)]
struct VariantSpec {
    name: String,
    kind: VariantKind,
}

#[derive(Debug, Clone)]
enum VariantKind {
    Unit,
    Tuple(Vec<TypeSpec>),
    Struct(Vec<(String, TypeSpec)>),
}

#[derive(Debug, Clone)]
enum TypeSpec {
    Primitive(&'static str),
    BorrowedStr,
    CowStr,
    Option(Box<TypeSpec>),
    Vec(Box<TypeSpec>),
    User(String),
}

impl TypeSpec {
    fn fmt(&self, mode: Mode) -> String {
        match (self, mode) {
            (TypeSpec::Primitive(p), _) => (*p).to_string(),
            (TypeSpec::BorrowedStr, Mode::Facet) => "String".to_string(),
            (TypeSpec::BorrowedStr, Mode::Serde) => "String".to_string(),
            (TypeSpec::CowStr, Mode::Facet) => "Cow<'static, str>".to_string(),
            (TypeSpec::CowStr, Mode::Serde) => "String".to_string(),
            (TypeSpec::Option(inner), m) => format!("Option<{}>", inner.fmt(m)),
            (TypeSpec::Vec(inner), m) => format!("Vec<{}>", inner.fmt(m)),
            (TypeSpec::User(name), _) => name.clone(),
        }
    }

    fn needs_cow(&self) -> bool {
        match self {
            TypeSpec::CowStr => true,
            TypeSpec::Option(inner) | TypeSpec::Vec(inner) => inner.needs_cow(),
            _ => false,
        }
    }
}

#[derive(Copy, Clone)]
enum Mode {
    Facet,
    Serde,
}

struct SchemaGenerator {
    cfg: SchemaConfig,
    rng: Lcg,
    structs: Vec<StructSpec>,
    enums: Vec<EnumSpec>,
    type_pool: Vec<String>,
}

impl SchemaGenerator {
    fn new(cfg: SchemaConfig) -> Self {
        let rng = Lcg::new(cfg.seed);
        let mut generator = SchemaGenerator {
            cfg,
            rng,
            structs: Vec::new(),
            enums: Vec::new(),
            type_pool: Vec::new(),
        };
        generator.build();
        generator
    }

    fn build(&mut self) {
        for idx in 0..self.cfg.structs {
            let name = format!("Struct{idx:03}");
            let field_count = self.rng.range(2, self.cfg.max_fields.max(2));
            let mut fields = Vec::new();
            for fidx in 0..field_count {
                let fname = format!("field_{fidx}");
                let ty = self.random_type(0);
                fields.push((fname, ty));
            }
            self.structs.push(StructSpec {
                name: name.clone(),
                fields,
            });
            // only expose completed types to avoid self-recursive shapes
            self.type_pool.push(name);
        }

        for idx in 0..self.cfg.enums {
            let name = format!("Enum{idx:03}");
            let variant_count = self.rng.range(2, self.cfg.max_variants.max(2));
            let mut variants = Vec::new();
            for vidx in 0..variant_count {
                let vname = format!("V{vidx}");
                let kind = match self.rng.next_u32() % 3 {
                    0 => VariantKind::Unit,
                    1 => {
                        let tuple_len = self.rng.range(1, 4);
                        let mut items = Vec::new();
                        for _ in 0..tuple_len {
                            items.push(self.random_type(0));
                        }
                        VariantKind::Tuple(items)
                    }
                    _ => {
                        let struct_len = self.rng.range(1, 4);
                        let mut items = Vec::new();
                        for fidx in 0..struct_len {
                            items.push((format!("f{fidx}"), self.random_type(0)));
                        }
                        VariantKind::Struct(items)
                    }
                };
                variants.push(VariantSpec { name: vname, kind });
            }
            self.enums.push(EnumSpec {
                name: name.clone(),
                variants,
            });
            self.type_pool.push(name);
        }
    }

    fn random_type(&mut self, depth: usize) -> TypeSpec {
        const PRIMS: &[&str] = &[
            "u8", "u16", "u32", "u64", "i32", "i64", "f32", "f64", "bool", "String",
        ];

        if depth >= self.cfg.max_depth {
            return if self.rng.next_u32() % 5 == 0 {
                self.user_type()
            } else {
                TypeSpec::Primitive(PRIMS[(self.rng.next_u32() as usize) % PRIMS.len()])
            };
        }

        match self.rng.next_u32() % 8 {
            0 => TypeSpec::BorrowedStr,
            1 => TypeSpec::CowStr,
            2 => TypeSpec::Option(Box::new(self.random_type(depth + 1))),
            3 => TypeSpec::Vec(Box::new(self.random_type(depth + 1))),
            4 => self.user_type(),
            _ => TypeSpec::Primitive(PRIMS[(self.rng.next_u32() as usize) % PRIMS.len()]),
        }
    }

    fn user_type(&mut self) -> TypeSpec {
        if self.type_pool.is_empty() {
            TypeSpec::Primitive("u8")
        } else {
            let idx = (self.rng.next_u32() as usize) % self.type_pool.len();
            TypeSpec::User(self.type_pool[idx].clone())
        }
    }

    fn render(&mut self) -> String {
        let mut out = String::new();
        out.push_str("// @generated by `cargo xtask schema`\n");
        out.push_str("// deterministic schema for compile-time/code-size benchmarking\n");
        out.push_str("#![allow(dead_code)]\n");
        out.push_str("#![allow(clippy::all)]\n\n");

        self.render_module(
            &mut out,
            Mode::Facet,
            "facet_types",
            "facet",
            "facet::Facet",
            "#[derive(Facet)]",
        );
        out.push('\n');
        self.render_module(
            &mut out,
            Mode::Serde,
            "serde_types",
            "serde",
            "serde::{Deserialize, Serialize}",
            "#[derive(Serialize, Deserialize)]",
        );

        out
    }

    fn render_module(
        &self,
        out: &mut String,
        mode: Mode,
        module: &str,
        cfg_feature: &str,
        uses: &str,
        derive: &str,
    ) {
        let uses_cow = matches!(mode, Mode::Facet)
            && (self
                .structs
                .iter()
                .any(|s| s.fields.iter().any(|(_, t)| t.needs_cow()))
                || self
                    .enums
                    .iter()
                    .any(|e| e.variants.iter().any(variant_needs_cow)));

        out.push_str(&format!(
            "#[cfg(feature = \"{cfg_feature}\")]\npub mod {module} {{\n",
        ));
        out.push_str(&format!("    use {uses};\n"));
        if uses_cow {
            out.push_str("    use std::borrow::Cow;\n");
        }
        out.push('\n');

        for s in &self.structs {
            out.push_str(&format!("    {derive}\n"));
            out.push_str("    #[derive(Default)]\n");
            out.push_str(&format!("    pub struct {} {{\n", s.name));
            for (fname, ty) in &s.fields {
                out.push_str(&format!("        pub {}: {},\n", fname, ty.fmt(mode)));
            }
            out.push_str("    }\n\n");
        }

        for e in &self.enums {
            out.push_str(&format!("    {derive}\n"));
            out.push_str("    #[repr(u16)]\n");
            out.push_str(&format!("    pub enum {} {{\n", e.name));
            for v in &e.variants {
                match &v.kind {
                    VariantKind::Unit => {
                        out.push_str(&format!("        {},\n", v.name));
                    }
                    VariantKind::Tuple(items) => {
                        let items_str = items
                            .iter()
                            .map(|t| t.fmt(mode))
                            .collect::<Vec<_>>()
                            .join(", ");
                        out.push_str(&format!("        {}({}),\n", v.name, items_str));
                    }
                    VariantKind::Struct(fields) => {
                        out.push_str(&format!("        {} {{\n", v.name));
                        for (fname, ty) in fields {
                            out.push_str(&format!("            {}: {},\n", fname, ty.fmt(mode)));
                        }
                        out.push_str("        },\n");
                    }
                }
            }
            out.push_str("    }\n\n");
        }

        out.push_str("}\n");

        fn variant_needs_cow(v: &VariantSpec) -> bool {
            match &v.kind {
                VariantKind::Unit => false,
                VariantKind::Tuple(items) => items.iter().any(TypeSpec::needs_cow),
                VariantKind::Struct(fields) => fields.iter().any(|(_, t)| t.needs_cow()),
            }
        }
    }
}

// Simple LCG to avoid adding RNG dependencies
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed | 1) // avoid zero cycles
    }

    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.0 >> 32) as u32
    }

    fn range(&mut self, min: usize, max: usize) -> usize {
        if max <= min {
            return min;
        }
        min + (self.next_u32() as usize % (max - min + 1))
    }
}

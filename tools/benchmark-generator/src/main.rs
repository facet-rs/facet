//! Generate benchmark code from KDL definitions for multiple formats.
//!
//! This tool reads all *.kdl files from facet-perf-shootout/benches/ and generates:
//! - src/{format}_types.rs: Type definitions for each format
//! - src/bench_ops/{format}.rs: Shared benchmark operations (inline functions)
//! - benches/unified_divan.rs: Wall-clock benchmarks using divan
//! - benches/unified_gungraun.rs: Instruction count benchmarks using gungraun
//! - tests/generated_tests.rs: Tests for debugging with valgrind
//!
//! The shared bench_ops module allows both divan and gungraun to reuse the same
//! benchmark logic without code duplication.
//!
//! Each format (json, postcard, msgpack, etc.) has its own KDL file defining
//! benchmarks, types, and the baseline/facet crate to use.

use benchmark_defs::{BenchmarkDef, BenchmarkFile, FormatConfig};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let workspace_root = find_workspace_root().unwrap_or_else(|| {
        eprintln!("Could not find workspace root");
        std::process::exit(1);
    });

    let benches_dir = workspace_root.join("facet-perf-shootout/benches");
    let output_dir = workspace_root.join("facet-perf-shootout");

    match generate_all(&benches_dir, &output_dir, &workspace_root) {
        Ok(()) => {
            println!("\n🎉 Success!");
            println!("Run benchmarks with:");
            println!("   cargo bench -p facet-perf-shootout --features jit");
        }
        Err(e) => {
            eprintln!("❌ Error generating benchmarks: {}", e);
            std::process::exit(1);
        }
    }
}

fn find_workspace_root() -> Option<PathBuf> {
    let mut current = env::current_dir().ok()?;
    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists()
            && let Ok(content) = fs::read_to_string(&cargo_toml)
            && content.contains("[workspace]")
        {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn generate_all(
    benches_dir: &Path,
    output_dir: &Path,
    workspace_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Discover all KDL files
    let files = benchmark_defs::discover_benchmark_files(benches_dir)?;

    if files.is_empty() {
        return Err("No benchmark KDL files found".into());
    }

    println!("📖 Found {} format(s):", files.len());
    for (format_name, file) in &files {
        println!(
            "   - {}: {} benchmarks, {} types",
            format_name,
            file.benchmarks.len(),
            file.type_defs.len()
        );
    }

    // Generate type modules (with pub types)
    for (format_name, file) in &files {
        let types_output = generate_types_module(file);
        let types_path = output_dir.join(format!("src/{}_types.rs", format_name));
        println!("✍️  Writing types to {}", types_path.display());
        fs::write(&types_path, types_output)?;
    }

    // Create bench_ops directory
    let bench_ops_dir = output_dir.join("src/bench_ops");
    fs::create_dir_all(&bench_ops_dir)?;

    // Generate bench_ops modules (shared benchmark operations)
    let mut bench_ops_mods = Vec::new();
    for (format_name, file) in &files {
        let bench_ops_output = generate_bench_ops_module(format_name, file, workspace_root)?;
        let bench_ops_path = bench_ops_dir.join(format!("{}.rs", format_name));
        println!("✍️  Writing bench_ops to {}", bench_ops_path.display());
        fs::write(&bench_ops_path, bench_ops_output)?;
        bench_ops_mods.push(format_name.clone());
    }

    // Generate bench_ops/mod.rs
    let bench_ops_mod = generate_bench_ops_mod(&bench_ops_mods);
    let bench_ops_mod_path = bench_ops_dir.join("mod.rs");
    println!(
        "✍️  Writing bench_ops mod to {}",
        bench_ops_mod_path.display()
    );
    fs::write(&bench_ops_mod_path, bench_ops_mod)?;

    // Generate divan benchmarks (thin wrapper)
    let divan_output = generate_divan_benchmarks(&files, workspace_root)?;
    let divan_path = output_dir.join("benches/unified_divan.rs");
    println!("✍️  Writing divan benchmarks to {}", divan_path.display());
    fs::write(&divan_path, divan_output)?;

    // Generate gungraun benchmarks (thin wrapper)
    let gungraun_output = generate_gungraun_benchmarks(&files, workspace_root)?;
    let gungraun_path = output_dir.join("benches/unified_gungraun.rs");
    println!(
        "✍️  Writing gungraun benchmarks to {}",
        gungraun_path.display()
    );
    fs::write(&gungraun_path, gungraun_output)?;

    // Generate tests
    let tests_output = generate_tests(&files, workspace_root)?;
    let tests_path = output_dir.join("tests/generated_tests.rs");
    println!("✍️  Writing tests to {}", tests_path.display());
    fs::write(&tests_path, tests_output)?;

    // Count totals
    let total_benchmarks: usize = files.values().map(|f| f.benchmarks.len()).sum();
    let total_types: usize = files.values().map(|f| f.type_defs.len()).sum();
    println!(
        "✅ Generated {} benchmarks across {} formats",
        total_benchmarks,
        files.len()
    );
    println!("✅ Generated {} type definitions", total_types);

    Ok(())
}

// =============================================================================
// Type Module Generation
// =============================================================================

fn generate_types_module(file: &BenchmarkFile) -> String {
    let mut output = String::new();

    output.push_str("//! ⚠️  AUTO-GENERATED by benchmark-generator ⚠️\n");
    output.push_str("//!\n");
    output.push_str("//! ❌ DO NOT EDIT THIS FILE DIRECTLY\n");
    output.push_str(&format!(
        "//! ✅ Instead, edit: facet-perf-shootout/benches/{}.kdl\n",
        file.format.name.value
    ));
    output.push_str("//!\n");
    output.push_str("//! To regenerate: cargo xtask gen-benchmarks\n\n");

    output.push_str("#![allow(dead_code)]\n");
    output.push_str("#![allow(unused_imports)]\n\n");
    output.push_str("use facet::Facet;\n");
    output.push_str("use serde::{Deserialize, Serialize};\n\n");

    for type_def in &file.type_defs {
        // Make types public by adding `pub` before struct/enum
        let code = make_types_public(&type_def.code.content);
        output.push_str(&code);
        output.push_str("\n\n");
    }

    output
}

/// Add `pub` visibility to struct/enum definitions and their fields
fn make_types_public(code: &str) -> String {
    use regex::Regex;

    let mut result = code.to_string();

    // Add pub to struct/enum declarations that don't have it
    let struct_enum_re = Regex::new(r"(?m)^(\s*)(struct |enum )").unwrap();
    result = struct_enum_re
        .replace_all(&result, "${1}pub ${2}")
        .to_string();

    // Add pub to field declarations inside structs
    // Match field patterns like "    field_name: Type," or " field_name: Type,"
    // The field name must be preceded by whitespace or comma (not ::)
    // Field names are lowercase identifiers, followed by : (not ::) and then space + type
    // Use negative lookahead equivalent: match ": " but not "::"
    let field_re = Regex::new(r"(?m)(^|[,\s])([a-z_][a-z0-9_]*)(\s*:\s+)").unwrap();
    result = field_re
        .replace_all(&result, "${1}pub ${2}${3}")
        .to_string();

    // Fix double pub (in case field already had pub)
    result = result.replace("pub pub ", "pub ");

    result
}

// =============================================================================
// Bench Ops Module Generation (shared benchmark operations)
// =============================================================================

fn generate_bench_ops_mod(formats: &[String]) -> String {
    let mut output = String::new();

    output.push_str("//! ⚠️  AUTO-GENERATED by benchmark-generator ⚠️\n");
    output.push_str("//!\n");
    output.push_str("//! Shared benchmark operations used by both divan and gungraun.\n");
    output.push_str(
        "//! Each format module contains inline functions for serialize/deserialize.\n\n",
    );

    for format in formats {
        output.push_str(&format!("pub mod {};\n", format));
    }

    output
}

fn generate_bench_ops_module(
    format_name: &str,
    file: &BenchmarkFile,
    workspace_root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();

    output.push_str("//! ⚠️  AUTO-GENERATED by benchmark-generator ⚠️\n");
    output.push_str("//!\n");
    output.push_str("//! ❌ DO NOT EDIT THIS FILE DIRECTLY\n");
    output.push_str(&format!(
        "//! ✅ Instead, edit: facet-perf-shootout/benches/{}.kdl\n",
        format_name
    ));
    output.push_str("//!\n");
    output.push_str("//! Shared benchmark operations for the {} format.\n");
    output.push_str(
        "//! These inline functions are called by both divan and gungraun benchmarks.\n\n",
    );

    output.push_str("#![allow(dead_code)]\n");
    output.push_str("#![allow(unused_imports)]\n");
    output.push_str("#![allow(clippy::redundant_closure)]\n");
    output.push_str("#![allow(clippy::explicit_auto_deref)]\n\n");

    // Common imports
    output.push_str("use std::hint::black_box;\n");
    output.push_str("use std::sync::LazyLock;\n\n");

    // Format-specific imports
    output.push_str(&generate_bench_ops_imports(&file.format));

    // Re-export types from the types module
    output.push_str(&format!("pub use crate::{}_types::*;\n\n", format_name));

    // Generate benchmark modules
    for bench_def in &file.benchmarks {
        output.push_str(&generate_bench_ops_benchmark_module(
            &file.format,
            bench_def,
            workspace_root,
        )?);
    }

    Ok(output)
}

fn generate_bench_ops_imports(format: &FormatConfig) -> String {
    let mut output = String::new();

    match format.name.value.as_str() {
        "json" => {
            output.push_str("#[cfg(feature = \"jit\")]\n");
            output.push_str("use facet_format::jit as format_jit;\n");
            output.push_str("#[cfg(feature = \"jit\")]\n");
            output.push_str("use facet_json::JsonParser;\n\n");
        }
        "postcard" => {
            output.push_str("#[cfg(feature = \"jit\")]\n");
            output.push_str("use facet_format::jit as format_jit;\n");
            output.push_str("#[cfg(feature = \"jit\")]\n");
            output.push_str("use facet_postcard::PostcardParser;\n\n");
            output.push_str("use ::postcard as postcard_crate;\n\n");
        }
        _ => {}
    }

    output
}

fn generate_bench_ops_benchmark_module(
    format: &FormatConfig,
    bench_def: &BenchmarkDef,
    workspace_root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();

    let (baseline_target, t0_target, t1_target, t2_target) = benchmark_defs::format_targets(format);

    output.push_str(&format!("pub mod {} {{\n", bench_def.name));
    output.push_str("    use super::*;\n\n");

    // Generate data loading
    match format.name.value.as_str() {
        "json" => {
            output.push_str(&generate_bench_ops_json_data(bench_def, workspace_root)?);
        }
        "postcard" => {
            output.push_str(&generate_bench_ops_postcard_data(bench_def)?);
        }
        _ => {
            return Err(format!("Unknown format: {}", format.name.value).into());
        }
    }

    // Generate inline deserialize functions
    output.push_str(&generate_bench_ops_deserialize(
        format,
        bench_def,
        &baseline_target,
        &t0_target,
        &t1_target,
        &t2_target,
    ));

    // Generate inline serialize functions
    output.push_str(&generate_bench_ops_serialize(
        format,
        bench_def,
        &baseline_target,
        &t0_target,
    ));

    // Generate JIT warmup functions (for gungraun)
    match format.name.value.as_str() {
        "json" => {
            output.push_str(&generate_bench_ops_jit_warmup_json(format, bench_def));
        }
        "postcard" => {
            output.push_str(&generate_bench_ops_jit_warmup_postcard(format, bench_def));
        }
        _ => {}
    }

    output.push_str("}\n\n");
    Ok(output)
}

fn generate_bench_ops_json_data(
    bench_def: &BenchmarkDef,
    workspace_root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();

    let is_brotli = bench_def.json_brotli.is_some();

    if is_brotli {
        let brotli_path = bench_def.json_brotli.as_ref().unwrap();
        output.push_str(&format!(
            "    static COMPRESSED: &[u8] = include_bytes!(\"../../../tools/benchmark-generator/{}\");\n\n",
            brotli_path.path
        ));
        output.push_str("    pub static JSON: LazyLock<Vec<u8>> = LazyLock::new(|| {\n");
        output.push_str("        let mut decompressed = Vec::new();\n");
        output.push_str(
            "        brotli::BrotliDecompress(&mut std::io::Cursor::new(COMPRESSED), &mut decompressed).unwrap();\n",
        );
        output.push_str("        decompressed\n");
        output.push_str("    });\n\n");
        output.push_str("    #[inline(always)]\n");
        output.push_str("    pub fn json_bytes() -> &'static [u8] { &*JSON }\n\n");
    } else {
        let json_content = get_json_content(bench_def, workspace_root)?;
        output.push_str(&format!(
            "    pub static JSON: &[u8] = br#\"{}\"#;\n\n",
            json_content
        ));
        output.push_str("    #[inline(always)]\n");
        output.push_str("    pub fn json_bytes() -> &'static [u8] { JSON }\n\n");
    }

    // Pre-deserialize data for serialization benchmarks
    let json_ref = if is_brotli { "&*JSON" } else { "JSON" };
    output.push_str(&format!(
        "    pub static DATA: LazyLock<{}> = LazyLock::new(|| serde_json::from_slice({}).unwrap());\n\n",
        bench_def.type_name, json_ref
    ));
    output.push_str("    #[inline(always)]\n");
    output.push_str(&format!(
        "    pub fn data() -> &'static {} {{ &*DATA }}\n\n",
        bench_def.type_name
    ));

    Ok(output)
}

fn generate_bench_ops_postcard_data(
    bench_def: &BenchmarkDef,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();

    if let Some(ref generated) = bench_def.generated {
        output.push_str(&format!(
            "    pub static DATA: LazyLock<{}> = LazyLock::new(|| {});\n",
            bench_def.type_name,
            generate_postcard_data(&generated.generator_name, &bench_def.type_name)?
        ));
        output.push_str(
            "    pub static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| postcard_crate::to_allocvec(&*DATA).unwrap());\n\n"
        );
        output.push_str("    #[inline(always)]\n");
        output.push_str("    pub fn encoded_bytes() -> &'static [u8] { &*ENCODED }\n\n");
        output.push_str("    #[inline(always)]\n");
        output.push_str(&format!(
            "    pub fn data() -> &'static {} {{ &*DATA }}\n\n",
            bench_def.type_name
        ));
    } else {
        return Err(format!(
            "Postcard benchmark '{}' must use 'generated' data source",
            bench_def.name
        )
        .into());
    }

    Ok(output)
}

fn generate_bench_ops_deserialize(
    format: &FormatConfig,
    bench_def: &BenchmarkDef,
    baseline_target: &str,
    t0_target: &str,
    t1_target: &str,
    t2_target: &str,
) -> String {
    let mut output = String::new();

    output.push_str("    // ===== DESERIALIZE =====\n\n");

    match format.name.value.as_str() {
        "json" => {
            // Baseline (serde_json)
            output.push_str("    #[inline(always)]\n");
            output.push_str(&format!(
                "    pub fn {}_deserialize() -> {} {{\n",
                baseline_target, bench_def.type_name
            ));
            output.push_str(&format!(
                "        serde_json::from_slice::<{}>(black_box(json_bytes())).unwrap()\n",
                bench_def.type_name
            ));
            output.push_str("    }\n\n");

            // T0 (no JIT)
            output.push_str("    #[inline(always)]\n");
            output.push_str(&format!(
                "    pub fn {}_deserialize() -> {} {{\n",
                t0_target, bench_def.type_name
            ));
            output.push_str(&format!(
                "        facet_json::from_slice::<{}>(black_box(json_bytes())).unwrap()\n",
                bench_def.type_name
            ));
            output.push_str("    }\n\n");

            // T1 (Tier-1 JIT)
            output.push_str("    #[cfg(feature = \"jit\")]\n");
            output.push_str("    #[inline(always)]\n");
            output.push_str(&format!(
                "    pub fn {}_deserialize() -> {} {{\n",
                t1_target, bench_def.type_name
            ));
            output.push_str(&format!(
                "        format_jit::deserialize_with_fallback::<{}, _>(JsonParser::new(black_box(json_bytes()))).unwrap()\n",
                bench_def.type_name
            ));
            output.push_str("    }\n\n");

            // T2 (Tier-2 JIT)
            output.push_str("    #[cfg(feature = \"jit\")]\n");
            output.push_str("    #[inline(always)]\n");
            output.push_str(&format!(
                "    pub fn {}_deserialize() -> {} {{\n",
                t2_target, bench_def.type_name
            ));
            output.push_str(&format!(
                "        format_jit::deserialize_with_format_jit_fallback::<{}, _>(JsonParser::new(black_box(json_bytes()))).unwrap()\n",
                bench_def.type_name
            ));
            output.push_str("    }\n\n");
        }
        "postcard" => {
            // Baseline (postcard)
            output.push_str("    #[inline(always)]\n");
            output.push_str(&format!(
                "    pub fn {}_deserialize() -> {} {{\n",
                baseline_target, bench_def.type_name
            ));
            output.push_str(&format!(
                "        postcard_crate::from_bytes::<{}>(black_box(encoded_bytes())).unwrap()\n",
                bench_def.type_name
            ));
            output.push_str("    }\n\n");

            // T0 (facet_postcard)
            output.push_str("    #[inline(always)]\n");
            output.push_str(&format!(
                "    pub fn {}_deserialize() -> {} {{\n",
                t0_target, bench_def.type_name
            ));
            output.push_str(&format!(
                "        facet_postcard::from_slice::<{}>(black_box(encoded_bytes())).unwrap()\n",
                bench_def.type_name
            ));
            output.push_str("    }\n\n");

            // T1 (facet_postcard with JIT tier 1) - only if format supports T1
            if format.has_t1() {
                output.push_str("    #[cfg(feature = \"jit\")]\n");
                output.push_str("    #[inline(always)]\n");
                output.push_str(&format!(
                    "    pub fn {}_deserialize() -> {} {{\n",
                    t1_target, bench_def.type_name
                ));
                output.push_str(&format!(
                    "        format_jit::deserialize_with_fallback::<{}, _>(PostcardParser::new(black_box(encoded_bytes()))).unwrap()\n",
                    bench_def.type_name
                ));
                output.push_str("    }\n\n");
            }

            // T2 (facet_postcard with JIT tier 2) - only if format supports T2
            if format.has_t2() {
                output.push_str("    #[cfg(feature = \"jit\")]\n");
                output.push_str("    #[inline(always)]\n");
                output.push_str(&format!(
                    "    pub fn {}_deserialize() -> {} {{\n",
                    t2_target, bench_def.type_name
                ));
                output.push_str(&format!(
                    "        format_jit::deserialize_with_format_jit_fallback::<{}, _>(PostcardParser::new(black_box(encoded_bytes()))).unwrap()\n",
                    bench_def.type_name
                ));
                output.push_str("    }\n\n");
            }
        }
        _ => {}
    }

    output
}

fn generate_bench_ops_serialize(
    format: &FormatConfig,
    _bench_def: &BenchmarkDef,
    baseline_target: &str,
    t0_target: &str,
) -> String {
    let mut output = String::new();

    output.push_str("    // ===== SERIALIZE =====\n\n");

    match format.name.value.as_str() {
        "json" => {
            // Baseline (serde_json)
            output.push_str("    #[inline(always)]\n");
            output.push_str(&format!(
                "    pub fn {}_serialize() -> String {{\n",
                baseline_target
            ));
            output.push_str("        serde_json::to_string(black_box(data())).unwrap()\n");
            output.push_str("    }\n\n");

            // T0 (facet_json)
            output.push_str("    #[inline(always)]\n");
            output.push_str(&format!(
                "    pub fn {}_serialize() -> String {{\n",
                t0_target
            ));
            output.push_str("        facet_json::to_string(black_box(data())).unwrap()\n");
            output.push_str("    }\n\n");
        }
        "postcard" => {
            // Baseline (postcard)
            output.push_str("    #[inline(always)]\n");
            output.push_str(&format!(
                "    pub fn {}_serialize() -> Vec<u8> {{\n",
                baseline_target
            ));
            output.push_str("        postcard_crate::to_allocvec(black_box(data())).unwrap()\n");
            output.push_str("    }\n\n");

            // T0 (facet_postcard)
            output.push_str("    #[inline(always)]\n");
            output.push_str(&format!(
                "    pub fn {}_serialize() -> Vec<u8> {{\n",
                t0_target
            ));
            output.push_str("        facet_postcard::to_vec(black_box(data())).unwrap()\n");
            output.push_str("    }\n\n");
        }
        _ => {}
    }

    output
}

fn generate_bench_ops_jit_warmup_json(format: &FormatConfig, bench_def: &BenchmarkDef) -> String {
    let mut output = String::new();

    output.push_str("    // ===== JIT WARMUP (for gungraun) =====\n\n");

    // T1 warmup - only if format supports T1
    if format.has_t1() {
        output.push_str("    #[cfg(feature = \"jit\")]\n");
        output.push_str("    pub fn warmup_t1() {\n");
        output.push_str(&format!(
            "        let _ = format_jit::deserialize_with_fallback::<{}, _>(JsonParser::new(json_bytes()));\n",
            bench_def.type_name
        ));
        output.push_str("    }\n\n");
    }

    // T2 warmup with tier stats - only if format supports T2
    if format.has_t2() {
        output.push_str("    #[cfg(feature = \"jit\")]\n");
        output.push_str("    pub fn warmup_t2() {\n");
        output.push_str("        format_jit::reset_tier_stats();\n");
        output.push_str("        let mut parser = JsonParser::new(json_bytes());\n");
        output.push_str(&format!(
            "        let _ = format_jit::try_deserialize_with_format_jit::<{}, _>(&mut parser);\n",
            bench_def.type_name
        ));
        output.push_str("        let (t2_attempts, t2_successes, _, _, _, t1_fallbacks) = format_jit::get_tier_stats();\n");
        output.push_str(&format!(
            "        eprintln!(\"[TIER_STATS] benchmark={} target=facet_json_t2 operation=deserialize tier2_attempts={{}} tier2_successes={{}} tier1_fallbacks={{}}\", t2_attempts, t2_successes, t1_fallbacks);\n",
            bench_def.name
        ));
        output.push_str("    }\n\n");
    }

    output
}

fn generate_bench_ops_jit_warmup_postcard(
    format: &FormatConfig,
    bench_def: &BenchmarkDef,
) -> String {
    let mut output = String::new();

    output.push_str("    // ===== JIT WARMUP (for gungraun) =====\n\n");

    // T1 warmup - only if format supports T1
    if format.has_t1() {
        output.push_str("    #[cfg(feature = \"jit\")]\n");
        output.push_str("    pub fn warmup_t1() {\n");
        output.push_str(&format!(
            "        let _ = format_jit::deserialize_with_fallback::<{}, _>(PostcardParser::new(encoded_bytes()));\n",
            bench_def.type_name
        ));
        output.push_str("    }\n\n");
    }

    // T2 warmup with tier stats - only if format supports T2
    if format.has_t2() {
        output.push_str("    #[cfg(feature = \"jit\")]\n");
        output.push_str("    pub fn warmup_t2() {\n");
        output.push_str("        format_jit::reset_tier_stats();\n");
        output.push_str("        let mut parser = PostcardParser::new(encoded_bytes());\n");
        output.push_str(&format!(
            "        let _ = format_jit::try_deserialize_with_format_jit::<{}, _>(&mut parser);\n",
            bench_def.type_name
        ));
        output.push_str("        let (t2_attempts, t2_successes, _, _, _, t1_fallbacks) = format_jit::get_tier_stats();\n");
        output.push_str(&format!(
            "        eprintln!(\"[TIER_STATS] benchmark={} target=facet_postcard_t2 operation=deserialize tier2_attempts={{}} tier2_successes={{}} tier1_fallbacks={{}}\", t2_attempts, t2_successes, t1_fallbacks);\n",
            bench_def.name
        ));
        output.push_str("    }\n\n");
    }

    output
}

// =============================================================================
// Divan Benchmark Generation
// =============================================================================

fn generate_divan_benchmarks(
    files: &HashMap<String, BenchmarkFile>,
    _workspace_root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();

    // File header
    output.push_str("//! ⚠️  AUTO-GENERATED by benchmark-generator ⚠️\n");
    output.push_str("//!\n");
    output.push_str("//! ❌ DO NOT EDIT THIS FILE DIRECTLY\n");
    output.push_str("//! ✅ Instead, edit: facet-perf-shootout/benches/*.kdl\n");
    output.push_str("//!\n");
    output.push_str("//! To regenerate: cargo xtask gen-benchmarks\n");
    output.push_str("//! To run benchmarks: cargo bench -p facet-perf-shootout --features jit\n");
    output.push_str("//!\n");
    output.push_str("//! This file contains thin wrappers around the shared bench_ops module.\n\n");

    output.push_str("#![allow(clippy::redundant_closure)]\n\n");

    // Common imports
    output.push_str("use divan::Bencher;\n");
    output.push_str("use std::hint::black_box;\n");
    output.push_str("use facet_perf_shootout::bench_ops;\n\n");

    // Conditional JIT import for tier stats
    output.push_str("#[cfg(feature = \"jit\")]\n");
    output.push_str("use facet_format::jit as format_jit;\n\n");

    // Generate per-format modules
    for (format_name, file) in files {
        output.push_str(&generate_divan_format_module_thin(format_name, file)?);
    }

    // Entry point
    output.push_str("fn main() {\n");
    output.push_str("    divan::main();\n");
    output.push_str("}\n");

    Ok(output)
}

fn generate_divan_format_module_thin(
    format_name: &str,
    file: &BenchmarkFile,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();

    output.push_str(
        "// ============================================================================\n",
    );
    output.push_str(&format!(
        "// FORMAT: {} (baseline: {}, facet: {})\n",
        format_name, file.format.baseline.value, file.format.facet_crate.value
    ));
    output.push_str(
        "// ============================================================================\n\n",
    );

    output.push_str(&format!("mod {} {{\n", format_name));
    output.push_str("    use super::*;\n\n");

    // Generate benchmark modules
    for bench_def in &file.benchmarks {
        output.push_str(&generate_divan_benchmark_module_thin(
            format_name,
            &file.format,
            bench_def,
        )?);
    }

    output.push_str("}\n\n");
    Ok(output)
}

fn generate_divan_benchmark_module_thin(
    format_name: &str,
    format: &FormatConfig,
    bench_def: &BenchmarkDef,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();

    let (baseline_target, t0_target, t1_target, t2_target) = benchmark_defs::format_targets(format);

    output.push_str(&format!("    mod {} {{\n", bench_def.name));
    output.push_str("        use super::*;\n\n");

    match format_name {
        "json" => {
            // Baseline (serde_json) deserialize
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_deserialize(bencher: Bencher) {{\n",
                baseline_target
            ));
            output.push_str(&format!(
                "            bencher.bench(|| black_box(bench_ops::{}::{}::{}_deserialize()));\n",
                format_name, bench_def.name, baseline_target
            ));
            output.push_str("        }\n\n");

            // T0 deserialize
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_deserialize(bencher: Bencher) {{\n",
                t0_target
            ));
            output.push_str(&format!(
                "            bencher.bench(|| black_box(bench_ops::{}::{}::{}_deserialize()));\n",
                format_name, bench_def.name, t0_target
            ));
            output.push_str("        }\n\n");

            // T1 deserialize
            output.push_str("        #[cfg(feature = \"jit\")]\n");
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_deserialize(bencher: Bencher) {{\n",
                t1_target
            ));
            output.push_str(&format!(
                "            bencher.bench(|| black_box(bench_ops::{}::{}::{}_deserialize()));\n",
                format_name, bench_def.name, t1_target
            ));
            output.push_str("        }\n\n");

            // T2 deserialize (with tier stats)
            output.push_str("        #[cfg(feature = \"jit\")]\n");
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_deserialize(bencher: Bencher) {{\n",
                t2_target
            ));
            output.push_str("            format_jit::reset_tier_stats();\n");
            output.push_str(&format!(
                "            bencher.bench(|| black_box(bench_ops::{}::{}::{}_deserialize()));\n",
                format_name, bench_def.name, t2_target
            ));
            output.push_str("            let (t2_attempts, t2_successes, _, _, _, t1_fallbacks) = format_jit::get_tier_stats();\n");
            output.push_str(&format!(
                "            eprintln!(\"[TIER_STATS] benchmark={} target={} operation=deserialize tier2_attempts={{}} tier2_successes={{}} tier1_fallbacks={{}}\", t2_attempts, t2_successes, t1_fallbacks);\n",
                bench_def.name, t2_target
            ));
            output.push_str("        }\n\n");

            // Baseline serialize
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_serialize(bencher: Bencher) {{\n",
                baseline_target
            ));
            output.push_str(&format!(
                "            bencher.bench(|| black_box(bench_ops::{}::{}::{}_serialize()));\n",
                format_name, bench_def.name, baseline_target
            ));
            output.push_str("        }\n\n");

            // T0 serialize
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_serialize(bencher: Bencher) {{\n",
                t0_target
            ));
            output.push_str(&format!(
                "            bencher.bench(|| black_box(bench_ops::{}::{}::{}_serialize()));\n",
                format_name, bench_def.name, t0_target
            ));
            output.push_str("        }\n\n");
        }
        "postcard" => {
            // Baseline deserialize
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_deserialize(bencher: Bencher) {{\n",
                baseline_target
            ));
            output.push_str(&format!(
                "            bencher.bench(|| black_box(bench_ops::{}::{}::{}_deserialize()));\n",
                format_name, bench_def.name, baseline_target
            ));
            output.push_str("        }\n\n");

            // T0 deserialize
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_deserialize(bencher: Bencher) {{\n",
                t0_target
            ));
            output.push_str(&format!(
                "            bencher.bench(|| black_box(bench_ops::{}::{}::{}_deserialize()));\n",
                format_name, bench_def.name, t0_target
            ));
            output.push_str("        }\n\n");

            // T1 deserialize - only if format supports T1
            if format.has_t1() {
                output.push_str("        #[cfg(feature = \"jit\")]\n");
                output.push_str("        #[divan::bench]\n");
                output.push_str(&format!(
                    "        fn {}_deserialize(bencher: Bencher) {{\n",
                    t1_target
                ));
                output.push_str(&format!(
                    "            bencher.bench(|| black_box(bench_ops::{}::{}::{}_deserialize()));\n",
                    format_name, bench_def.name, t1_target
                ));
                output.push_str("        }\n\n");
            }

            // T2 deserialize (with tier stats) - only if format supports T2
            if format.has_t2() {
                output.push_str("        #[cfg(feature = \"jit\")]\n");
                output.push_str("        #[divan::bench]\n");
                output.push_str(&format!(
                    "        fn {}_deserialize(bencher: Bencher) {{\n",
                    t2_target
                ));
                output.push_str("            format_jit::reset_tier_stats();\n");
                output.push_str(&format!(
                    "            bencher.bench(|| black_box(bench_ops::{}::{}::{}_deserialize()));\n",
                    format_name, bench_def.name, t2_target
                ));
                output.push_str("            let (t2_attempts, t2_successes, _, _, _, t1_fallbacks) = format_jit::get_tier_stats();\n");
                output.push_str(&format!(
                    "            eprintln!(\"[TIER_STATS] benchmark={} target={} operation=deserialize tier2_attempts={{}} tier2_successes={{}} tier1_fallbacks={{}}\", t2_attempts, t2_successes, t1_fallbacks);\n",
                    bench_def.name, t2_target
                ));
                output.push_str("        }\n\n");
            }

            // Baseline serialize
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_serialize(bencher: Bencher) {{\n",
                baseline_target
            ));
            output.push_str(&format!(
                "            bencher.bench(|| black_box(bench_ops::{}::{}::{}_serialize()));\n",
                format_name, bench_def.name, baseline_target
            ));
            output.push_str("        }\n\n");

            // T0 serialize
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_serialize(bencher: Bencher) {{\n",
                t0_target
            ));
            output.push_str(&format!(
                "            bencher.bench(|| black_box(bench_ops::{}::{}::{}_serialize()));\n",
                format_name, bench_def.name, t0_target
            ));
            output.push_str("        }\n\n");
        }
        _ => {}
    }

    output.push_str("    }\n\n");
    Ok(output)
}

#[allow(dead_code)]
fn generate_divan_format_module(
    format_name: &str,
    file: &BenchmarkFile,
    workspace_root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();

    output.push_str(
        "// ============================================================================\n",
    );
    output.push_str(&format!(
        "// FORMAT: {} (baseline: {}, facet: {})\n",
        format_name, file.format.baseline.value, file.format.facet_crate.value
    ));
    output.push_str(
        "// ============================================================================\n\n",
    );

    output.push_str(&format!("mod {} {{\n", format_name));
    output.push_str("    use super::*;\n");

    // Format-specific imports
    output.push_str(&generate_format_imports(&file.format));

    // Type definitions inline
    output.push_str("\n    // Type definitions\n");
    for type_def in &file.type_defs {
        // Indent each line
        for line in type_def.code.content.lines() {
            output.push_str("    ");
            output.push_str(line);
            output.push('\n');
        }
        output.push('\n');
    }

    // Generate benchmark modules
    for bench_def in &file.benchmarks {
        output.push_str(&generate_divan_benchmark_module(
            &file.format,
            bench_def,
            workspace_root,
        )?);
    }

    output.push_str("}\n\n");
    Ok(output)
}

#[allow(dead_code)]
fn generate_format_imports(format: &FormatConfig) -> String {
    let mut output = String::new();

    match format.name.value.as_str() {
        "json" => {
            output.push_str("    use facet::Facet;\n");
            output.push_str("    #[cfg(feature = \"jit\")]\n");
            output.push_str("    use facet_format::jit as format_jit;\n");
            output.push_str("    #[cfg(feature = \"jit\")]\n");
            output.push_str("    use facet_json::JsonParser;\n");
        }
        "postcard" => {
            output.push_str("    use facet::Facet;\n");
            output.push_str(
                "    // Re-import postcard crate with alias to avoid shadowing by module name\n",
            );
            output.push_str("    use ::postcard as postcard_crate;\n");
            output.push_str("    #[cfg(feature = \"jit\")]\n");
            output.push_str("    use facet_format::jit as format_jit;\n");
            output.push_str("    #[cfg(feature = \"jit\")]\n");
            output.push_str("    use facet_postcard::PostcardParser;\n");
        }
        _ => {
            output.push_str("    use facet::Facet;\n");
        }
    }

    output
}

#[allow(dead_code)]
fn generate_divan_benchmark_module(
    format: &FormatConfig,
    bench_def: &BenchmarkDef,
    workspace_root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();

    let (baseline_target, t0_target, t1_target, t2_target) = benchmark_defs::format_targets(format);

    output.push_str(&format!("    mod {} {{\n", bench_def.name));
    output.push_str("        use super::*;\n\n");

    // Generate data loading based on format
    match format.name.value.as_str() {
        "json" => {
            output.push_str(&generate_json_data_loading(bench_def, workspace_root)?);
        }
        "postcard" => {
            output.push_str(&generate_postcard_data_loading(bench_def)?);
        }
        _ => {
            return Err(format!("Unknown format: {}", format.name.value).into());
        }
    }

    // Generate benchmark functions
    output.push_str(&generate_divan_deserialize_benchmarks(
        format,
        bench_def,
        &baseline_target,
        &t0_target,
        &t1_target,
        &t2_target,
    ));

    output.push_str(&generate_divan_serialize_benchmarks(
        format,
        bench_def,
        &baseline_target,
        &t0_target,
    ));

    output.push_str("    }\n\n");
    Ok(output)
}

#[allow(dead_code)]
fn generate_json_data_loading(
    bench_def: &BenchmarkDef,
    workspace_root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();

    let is_brotli = bench_def.json_brotli.is_some();

    if is_brotli {
        let brotli_path = bench_def.json_brotli.as_ref().unwrap();
        output.push_str(&format!(
            "        static COMPRESSED: &[u8] = include_bytes!(\"../../tools/benchmark-generator/{}\");\n\n",
            brotli_path.path
        ));
        output.push_str("        static JSON: LazyLock<Vec<u8>> = LazyLock::new(|| {\n");
        output.push_str("            let mut decompressed = Vec::new();\n");
        output.push_str(
            "            brotli::BrotliDecompress(&mut std::io::Cursor::new(COMPRESSED), &mut decompressed).unwrap();\n",
        );
        output.push_str("            decompressed\n");
        output.push_str("        });\n\n");
    } else {
        let json_content = get_json_content(bench_def, workspace_root)?;
        output.push_str(&format!(
            "        static JSON: &[u8] = br#\"{}\"#;\n\n",
            json_content
        ));
    }

    // Pre-deserialize data for serialization benchmarks
    let json_ref = if is_brotli { "&*JSON" } else { "JSON" };
    output.push_str(&format!(
        "        static DATA: LazyLock<{}> = LazyLock::new(|| serde_json::from_slice({}).unwrap());\n\n",
        bench_def.type_name, json_ref
    ));

    Ok(output)
}

#[allow(dead_code)]
fn generate_postcard_data_loading(
    bench_def: &BenchmarkDef,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();

    // For postcard, we generate data at compile time and encode it
    if let Some(ref generated) = bench_def.generated {
        output.push_str(&format!(
            "        static DATA: LazyLock<{}> = LazyLock::new(|| {});\n",
            bench_def.type_name,
            generate_postcard_data(&generated.generator_name, &bench_def.type_name)?
        ));
        output.push_str(
            "        static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| postcard_crate::to_allocvec(&*DATA).unwrap());\n\n"
        );
    } else {
        return Err(format!(
            "Postcard benchmark '{}' must use 'generated' data source",
            bench_def.name
        )
        .into());
    }

    Ok(output)
}

fn generate_postcard_data(
    generator_name: &str,
    _type_name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    // Return Rust code that generates the data at runtime
    match generator_name {
        "vec_bool_1k" => Ok("(0..1000).map(|i| i % 3 != 0).collect()".to_string()),
        "vec_u8_empty" => Ok("Vec::new()".to_string()),
        "vec_u8_16" => Ok("(0..16u8).collect()".to_string()),
        "vec_u8_256" => Ok("(0u8..=255).collect()".to_string()),
        "vec_u8_1k" => Ok("(0..1000).map(|i| (i % 256) as u8).collect()".to_string()),
        "vec_u8_64k" => Ok("(0..65536).map(|i| (i % 256) as u8).collect()".to_string()),
        "vec_u32_1k" => Ok(r#"(0..1000).map(|i| match i % 4 {
                0 => i as u32,
                1 => (i as u32) * 100,
                2 => (i as u32) * 10000,
                _ => (i as u32) * 1000000,
            }).collect()"#
            .to_string()),
        "vec_u64_1k" => Ok(r#"(0..1000).map(|i| match i % 5 {
                0 => i as u64,
                1 => (i as u64) * 1000,
                2 => (i as u64) * 1000000,
                3 => (i as u64) * 1000000000,
                _ => u64::MAX / (i as u64 + 1),
            }).collect()"#
            .to_string()),
        "vec_u64_small" => Ok("(0..10).map(|i| i * 12345u64).collect()".to_string()),
        "vec_u64_large" => Ok("(0..10000).map(|i| i * 12345u64).collect()".to_string()),
        "vec_i32_1k" => Ok(r#"(0..1000i32).map(|i| {
                let base = i * 100;
                if i % 2 == 0 { base } else { -base }
            }).collect()"#
            .to_string()),
        "vec_i64_1k" => Ok(r#"(0..1000i64).map(|i| {
                let base = i * 1000000;
                if i % 2 == 0 { base } else { -base }
            }).collect()"#
            .to_string()),
        "simple_struct" => Ok(r#"SimpleStruct {
                id: 42,
                name: "test".to_string(),
                active: true,
            }"#
        .to_string()),
        "nested_struct" => Ok(r#"NestedStruct {
                id: 42,
                inner: NestedInner { x: 10, y: 20, label: "inner".to_string() },
                enabled: true,
            }"#
        .to_string()),
        "wide_struct" => Ok(r#"WideStruct {
                field_00: 0, field_01: 1, field_02: 2, field_03: 3, field_04: 4,
                field_05: 5, field_06: 6, field_07: 7, field_08: 8, field_09: 9,
                field_10: "s10".to_string(), field_11: "s11".to_string(),
                field_12: "s12".to_string(), field_13: "s13".to_string(),
                field_14: "s14".to_string(),
                field_15: true, field_16: false, field_17: true, field_18: false, field_19: true,
            }"#
        .to_string()),
        "vec_simple_struct" => Ok(r#"(0..100).map(|i| SimpleStruct {
                id: i,
                name: format!("name_{}", i),
                active: i % 2 == 0,
            }).collect()"#
            .to_string()),
        "vec_string_short" => {
            Ok(r#"(0..1000).map(|i| format!("str_{:06}", i)).collect()"#.to_string())
        }
        "vec_string_long" => Ok(
            r#"(0..100).map(|i| "x".repeat(1000) + format!("_{}", i).as_str()).collect()"#
                .to_string(),
        ),
        _ => Err(format!("Unknown postcard generator: {}", generator_name).into()),
    }
}

#[allow(dead_code)]
fn generate_divan_deserialize_benchmarks(
    format: &FormatConfig,
    bench_def: &BenchmarkDef,
    baseline_target: &str,
    t0_target: &str,
    t1_target: &str,
    t2_target: &str,
) -> String {
    let mut output = String::new();

    output.push_str("        // ===== DESERIALIZE =====\n\n");

    match format.name.value.as_str() {
        "json" => {
            let is_brotli = bench_def.json_brotli.is_some();
            let json_ref = if is_brotli { "json.as_slice()" } else { "JSON" };
            let json_setup = if is_brotli {
                "            let json = &*JSON;\n"
            } else {
                ""
            };

            // Baseline (serde_json)
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_deserialize(bencher: Bencher) {{\n",
                baseline_target
            ));
            output.push_str(json_setup);
            output.push_str("            bencher.bench(|| {\n");
            output.push_str(&format!(
                "                black_box(serde_json::from_slice::<{}>(black_box({})).unwrap())\n",
                bench_def.type_name, json_ref
            ));
            output.push_str("            });\n");
            output.push_str("        }\n\n");

            // T0 (no JIT)
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_deserialize(bencher: Bencher) {{\n",
                t0_target
            ));
            output.push_str(json_setup);
            output.push_str("            bencher.bench(|| {\n");
            output.push_str(&format!(
                "                black_box(facet_json::from_slice::<{}>(black_box({})).unwrap())\n",
                bench_def.type_name, json_ref
            ));
            output.push_str("            });\n");
            output.push_str("        }\n\n");

            // T1 (Tier-1 JIT)
            output.push_str("        #[cfg(feature = \"jit\")]\n");
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_deserialize(bencher: Bencher) {{\n",
                t1_target
            ));
            output.push_str(json_setup);
            output.push_str("            bencher.bench(|| {\n");
            output.push_str(&format!(
                "                black_box(format_jit::deserialize_with_fallback::<{}, _>(JsonParser::new(black_box({}))).unwrap())\n",
                bench_def.type_name, json_ref
            ));
            output.push_str("            });\n");
            output.push_str("        }\n\n");

            // T2 (Tier-2 JIT)
            output.push_str("        #[cfg(feature = \"jit\")]\n");
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_deserialize(bencher: Bencher) {{\n",
                t2_target
            ));
            output.push_str(json_setup);
            output.push_str("            format_jit::reset_tier_stats();\n");
            output.push_str("            bencher.bench(|| {\n");
            output.push_str(&format!(
                "                black_box(format_jit::deserialize_with_format_jit_fallback::<{}, _>(JsonParser::new(black_box({}))).unwrap())\n",
                bench_def.type_name, json_ref
            ));
            output.push_str("            });\n");
            output.push_str("            let (t2_attempts, t2_successes, _, _, _, t1_fallbacks) = format_jit::get_tier_stats();\n");
            output.push_str(&format!(
                "            eprintln!(\"[TIER_STATS] benchmark={} target={} operation=deserialize tier2_attempts={{}} tier2_successes={{}} tier1_fallbacks={{}}\", t2_attempts, t2_successes, t1_fallbacks);\n",
                bench_def.name, t2_target
            ));
            output.push_str("        }\n\n");
        }
        "postcard" => {
            // Baseline (postcard)
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_deserialize(bencher: Bencher) {{\n",
                baseline_target
            ));
            output.push_str("            let data = &*ENCODED;\n");
            output.push_str("            bencher.bench(|| {\n");
            output.push_str(&format!(
                "                black_box(postcard_crate::from_bytes::<{}>(black_box(data)).unwrap())\n",
                bench_def.type_name
            ));
            output.push_str("            });\n");
            output.push_str("        }\n\n");

            // T0 (no JIT) - facet_postcard::from_slice
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_deserialize(bencher: Bencher) {{\n",
                t0_target
            ));
            output.push_str("            let data = &*ENCODED;\n");
            output.push_str("            bencher.bench(|| {\n");
            output.push_str(&format!(
                "                black_box(facet_postcard::from_slice::<{}>(black_box(data)).unwrap())\n",
                bench_def.type_name
            ));
            output.push_str("            });\n");
            output.push_str("        }\n\n");

            // T1 (JIT tier 1) - format_jit::deserialize_with_fallback
            output.push_str("        #[cfg(feature = \"jit\")]\n");
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_deserialize(bencher: Bencher) {{\n",
                t1_target
            ));
            output.push_str("            let data = &*ENCODED;\n");
            output.push_str("            bencher.bench(|| {\n");
            output.push_str(&format!(
                "                black_box(format_jit::deserialize_with_fallback::<{}, _>(PostcardParser::new(black_box(data))).unwrap())\n",
                bench_def.type_name
            ));
            output.push_str("            });\n");
            output.push_str("        }\n\n");

            // T2 (JIT tier 2) - format_jit::deserialize_with_format_jit_fallback
            output.push_str("        #[cfg(feature = \"jit\")]\n");
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_deserialize(bencher: Bencher) {{\n",
                t2_target
            ));
            output.push_str("            let data = &*ENCODED;\n");
            output.push_str("            bencher.bench(|| {\n");
            output.push_str(&format!(
                "                black_box(format_jit::deserialize_with_format_jit_fallback::<{}, _>(PostcardParser::new(black_box(data))).unwrap())\n",
                bench_def.type_name
            ));
            output.push_str("            });\n");
            output.push_str("        }\n\n");
        }
        _ => {}
    }

    output
}

#[allow(dead_code)]
fn generate_divan_serialize_benchmarks(
    format: &FormatConfig,
    _bench_def: &BenchmarkDef,
    baseline_target: &str,
    t0_target: &str,
) -> String {
    let mut output = String::new();

    output.push_str("        // ===== SERIALIZE =====\n\n");

    match format.name.value.as_str() {
        "json" => {
            // Baseline (serde_json)
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_serialize(bencher: Bencher) {{\n",
                baseline_target
            ));
            output.push_str("            let data = &*DATA;\n");
            output.push_str("            bencher.bench(|| {\n");
            output.push_str(
                "                black_box(serde_json::to_string(black_box(data)).unwrap())\n",
            );
            output.push_str("            });\n");
            output.push_str("        }\n\n");

            // T0 (facet_json)
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_serialize(bencher: Bencher) {{\n",
                t0_target
            ));
            output.push_str("            let data = &*DATA;\n");
            output.push_str("            bencher.bench(|| {\n");
            output.push_str(
                "                black_box(facet_json::to_string(black_box(data)).unwrap())\n",
            );
            output.push_str("            });\n");
            output.push_str("        }\n\n");
        }
        "postcard" => {
            // Baseline (postcard)
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_serialize(bencher: Bencher) {{\n",
                baseline_target
            ));
            output.push_str("            let data = &*DATA;\n");
            output.push_str("            bencher.bench(|| {\n");
            output.push_str(
                "                black_box(postcard_crate::to_allocvec(black_box(data)).unwrap())\n",
            );
            output.push_str("            });\n");
            output.push_str("        }\n\n");

            // T0 (facet_postcard) - if available
            output.push_str("        #[divan::bench]\n");
            output.push_str(&format!(
                "        fn {}_serialize(bencher: Bencher) {{\n",
                t0_target
            ));
            output.push_str("            let data = &*DATA;\n");
            output.push_str("            bencher.bench(|| {\n");
            output.push_str(
                "                black_box(facet_postcard::to_vec(black_box(data)).unwrap())\n",
            );
            output.push_str("            });\n");
            output.push_str("        }\n\n");
        }
        _ => {}
    }

    output
}

// =============================================================================
// Gungraun Benchmark Generation
// =============================================================================

fn generate_gungraun_benchmarks(
    files: &HashMap<String, BenchmarkFile>,
    _workspace_root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();

    // File header
    output.push_str("//! ⚠️  AUTO-GENERATED by benchmark-generator ⚠️\n");
    output.push_str("//!\n");
    output.push_str("//! ❌ DO NOT EDIT THIS FILE DIRECTLY\n");
    output.push_str("//! ✅ Instead, edit: facet-perf-shootout/benches/*.kdl\n");
    output.push_str("//!\n");
    output.push_str("//! To regenerate: cargo xtask gen-benchmarks\n");
    output.push_str("//! Gungraun benchmarks (instruction counts via valgrind)\n");
    output.push_str("//!\n");
    output.push_str("//! This file contains thin wrappers around the shared bench_ops module.\n\n");

    output.push_str("#![allow(clippy::redundant_closure)]\n");
    output.push_str("#![allow(unused_imports)]\n\n");

    // Common imports
    output.push_str("use std::hint::black_box;\n");
    output.push_str("use std::collections::HashMap;\n");
    output.push_str("use facet_perf_shootout::bench_ops;\n");

    // Import all types from each format's bench_ops module
    for format_name in files.keys() {
        output.push_str(&format!("use bench_ops::{}::*;\n", format_name));
    }
    output.push('\n');

    // Collect all benchmark groups for the gungraun::main! macro
    let mut all_groups: Vec<String> = Vec::new();

    // Generate per-format modules
    for (format_name, file) in files {
        let (groups, module_output) = generate_gungraun_format_module(format_name, file)?;
        output.push_str(&module_output);
        all_groups.extend(groups);
    }

    // Generate the gungraun::main! macro invocation
    output.push_str("gungraun::main!(\n");
    output.push_str("    library_benchmark_groups =\n");
    for (i, group) in all_groups.iter().enumerate() {
        if i > 0 {
            output.push_str(",\n");
        }
        output.push_str(&format!("        {}", group));
    }
    output.push_str("\n);\n");

    Ok(output)
}

fn generate_gungraun_format_module(
    format_name: &str,
    file: &BenchmarkFile,
) -> Result<(Vec<String>, String), Box<dyn std::error::Error>> {
    let mut output = String::new();
    let mut groups = Vec::new();

    output.push_str(
        "// ============================================================================\n",
    );
    output.push_str(&format!(
        "// FORMAT: {} (baseline: {}, facet: {})\n",
        format_name, file.format.baseline.value, file.format.facet_crate.value
    ));
    output.push_str(
        "// ============================================================================\n\n",
    );

    // Generate benchmark modules
    for bench_def in &file.benchmarks {
        let (bench_groups, bench_output) =
            generate_gungraun_benchmark_module(format_name, &file.format, bench_def)?;
        output.push_str(&bench_output);
        groups.extend(bench_groups);
    }

    Ok((groups, output))
}

/// For gungraun, we just use the type name as-is.
/// The types are brought into scope via `use bench_ops::{format}::*;`
fn gungraun_return_type(_format_name: &str, _bench_name: &str, type_name: &str) -> String {
    type_name.to_string()
}

fn generate_gungraun_benchmark_module(
    format_name: &str,
    format: &FormatConfig,
    bench_def: &BenchmarkDef,
) -> Result<(Vec<String>, String), Box<dyn std::error::Error>> {
    let mut output = String::new();
    let mut groups = Vec::new();

    let (baseline_target, t0_target, t1_target, t2_target) = benchmark_defs::format_targets(format);
    let return_type = gungraun_return_type(format_name, &bench_def.name, &bench_def.type_name);

    // No module - gungraun's library_benchmark_group! macro doesn't support paths
    // Use flat naming with format_benchmark_target pattern

    match format_name {
        "json" => {
            // Baseline deserialize
            output.push_str("#[gungraun::library_benchmark]\n");
            output.push_str(&format!(
                "fn gungraun_{}_{}_{}_deserialize() -> {} {{\n",
                format_name, bench_def.name, baseline_target, return_type
            ));
            output.push_str(&format!(
                "    black_box(bench_ops::{}::{}::{}_deserialize())\n",
                format_name, bench_def.name, baseline_target
            ));
            output.push_str("}\n\n");

            // T0 deserialize
            output.push_str("#[gungraun::library_benchmark]\n");
            output.push_str(&format!(
                "fn gungraun_{}_{}_{}_deserialize() -> {} {{\n",
                format_name, bench_def.name, t0_target, return_type
            ));
            output.push_str(&format!(
                "    black_box(bench_ops::{}::{}::{}_deserialize())\n",
                format_name, bench_def.name, t0_target
            ));
            output.push_str("}\n\n");

            // T1 deserialize (with setup for JIT warmup)
            output.push_str("#[cfg(feature = \"jit\")]\n");
            output.push_str(&format!(
                "fn setup_{}_{}_{}_t1() {{\n",
                format_name, bench_def.name, t1_target
            ));
            output.push_str(&format!(
                "    bench_ops::{}::{}::warmup_t1();\n",
                format_name, bench_def.name
            ));
            output.push_str("}\n\n");

            output.push_str("#[cfg(feature = \"jit\")]\n");
            output.push_str("#[gungraun::library_benchmark]\n");
            output.push_str(&format!(
                "#[bench::cached(setup = setup_{}_{}_{}_t1)]\n",
                format_name, bench_def.name, t1_target
            ));
            output.push_str(&format!(
                "fn gungraun_{}_{}_{}_deserialize(_: ()) -> {} {{\n",
                format_name, bench_def.name, t1_target, return_type
            ));
            output.push_str(&format!(
                "    black_box(bench_ops::{}::{}::{}_deserialize())\n",
                format_name, bench_def.name, t1_target
            ));
            output.push_str("}\n\n");

            // T2 deserialize (with setup for JIT warmup + tier stats)
            output.push_str("#[cfg(feature = \"jit\")]\n");
            output.push_str(&format!(
                "fn setup_{}_{}_{}_t2() {{\n",
                format_name, bench_def.name, t2_target
            ));
            output.push_str(&format!(
                "    bench_ops::{}::{}::warmup_t2();\n",
                format_name, bench_def.name
            ));
            output.push_str("}\n\n");

            output.push_str("#[cfg(feature = \"jit\")]\n");
            output.push_str("#[gungraun::library_benchmark]\n");
            output.push_str(&format!(
                "#[bench::cached(setup = setup_{}_{}_{}_t2)]\n",
                format_name, bench_def.name, t2_target
            ));
            output.push_str(&format!(
                "fn gungraun_{}_{}_{}_deserialize(_: ()) -> {} {{\n",
                format_name, bench_def.name, t2_target, return_type
            ));
            output.push_str(&format!(
                "    black_box(bench_ops::{}::{}::{}_deserialize())\n",
                format_name, bench_def.name, t2_target
            ));
            output.push_str("}\n\n");

            // Baseline serialize
            output.push_str("#[gungraun::library_benchmark]\n");
            output.push_str(&format!(
                "fn gungraun_{}_{}_{}_serialize() -> String {{\n",
                format_name, bench_def.name, baseline_target
            ));
            output.push_str(&format!(
                "    black_box(bench_ops::{}::{}::{}_serialize())\n",
                format_name, bench_def.name, baseline_target
            ));
            output.push_str("}\n\n");

            // T0 serialize
            output.push_str("#[gungraun::library_benchmark]\n");
            output.push_str(&format!(
                "fn gungraun_{}_{}_{}_serialize() -> String {{\n",
                format_name, bench_def.name, t0_target
            ));
            output.push_str(&format!(
                "    black_box(bench_ops::{}::{}::{}_serialize())\n",
                format_name, bench_def.name, t0_target
            ));
            output.push_str("}\n\n");

            // Add to groups
            groups.push(format!("{}_{}_deser", format_name, bench_def.name));
            groups.push(format!("{}_{}_ser", format_name, bench_def.name));
        }
        "postcard" => {
            // Baseline deserialize
            output.push_str("#[gungraun::library_benchmark]\n");
            output.push_str(&format!(
                "fn gungraun_{}_{}_{}_deserialize() -> {} {{\n",
                format_name, bench_def.name, baseline_target, return_type
            ));
            output.push_str(&format!(
                "    black_box(bench_ops::{}::{}::{}_deserialize())\n",
                format_name, bench_def.name, baseline_target
            ));
            output.push_str("}\n\n");

            // T0 deserialize
            output.push_str("#[gungraun::library_benchmark]\n");
            output.push_str(&format!(
                "fn gungraun_{}_{}_{}_deserialize() -> {} {{\n",
                format_name, bench_def.name, t0_target, return_type
            ));
            output.push_str(&format!(
                "    black_box(bench_ops::{}::{}::{}_deserialize())\n",
                format_name, bench_def.name, t0_target
            ));
            output.push_str("}\n\n");

            // T1 deserialize (with setup for JIT warmup) - only if format supports T1
            if format.has_t1() {
                output.push_str("#[cfg(feature = \"jit\")]\n");
                output.push_str(&format!(
                    "fn setup_{}_{}_{}_t1() {{\n",
                    format_name, bench_def.name, t1_target
                ));
                output.push_str(&format!(
                    "    bench_ops::{}::{}::warmup_t1();\n",
                    format_name, bench_def.name
                ));
                output.push_str("}\n\n");

                output.push_str("#[cfg(feature = \"jit\")]\n");
                output.push_str("#[gungraun::library_benchmark]\n");
                output.push_str(&format!(
                    "#[bench::cached(setup = setup_{}_{}_{}_t1)]\n",
                    format_name, bench_def.name, t1_target
                ));
                output.push_str(&format!(
                    "fn gungraun_{}_{}_{}_deserialize(_: ()) -> {} {{\n",
                    format_name, bench_def.name, t1_target, return_type
                ));
                output.push_str(&format!(
                    "    black_box(bench_ops::{}::{}::{}_deserialize())\n",
                    format_name, bench_def.name, t1_target
                ));
                output.push_str("}\n\n");
            }

            // T2 deserialize (with setup for JIT warmup + tier stats) - only if format supports T2
            if format.has_t2() {
                output.push_str("#[cfg(feature = \"jit\")]\n");
                output.push_str(&format!(
                    "fn setup_{}_{}_{}_t2() {{\n",
                    format_name, bench_def.name, t2_target
                ));
                output.push_str(&format!(
                    "    bench_ops::{}::{}::warmup_t2();\n",
                    format_name, bench_def.name
                ));
                output.push_str("}\n\n");

                output.push_str("#[cfg(feature = \"jit\")]\n");
                output.push_str("#[gungraun::library_benchmark]\n");
                output.push_str(&format!(
                    "#[bench::cached(setup = setup_{}_{}_{}_t2)]\n",
                    format_name, bench_def.name, t2_target
                ));
                output.push_str(&format!(
                    "fn gungraun_{}_{}_{}_deserialize(_: ()) -> {} {{\n",
                    format_name, bench_def.name, t2_target, return_type
                ));
                output.push_str(&format!(
                    "    black_box(bench_ops::{}::{}::{}_deserialize())\n",
                    format_name, bench_def.name, t2_target
                ));
                output.push_str("}\n\n");
            }

            // Baseline serialize
            output.push_str("#[gungraun::library_benchmark]\n");
            output.push_str(&format!(
                "fn gungraun_{}_{}_{}_serialize() -> Vec<u8> {{\n",
                format_name, bench_def.name, baseline_target
            ));
            output.push_str(&format!(
                "    black_box(bench_ops::{}::{}::{}_serialize())\n",
                format_name, bench_def.name, baseline_target
            ));
            output.push_str("}\n\n");

            // T0 serialize
            output.push_str("#[gungraun::library_benchmark]\n");
            output.push_str(&format!(
                "fn gungraun_{}_{}_{}_serialize() -> Vec<u8> {{\n",
                format_name, bench_def.name, t0_target
            ));
            output.push_str(&format!(
                "    black_box(bench_ops::{}::{}::{}_serialize())\n",
                format_name, bench_def.name, t0_target
            ));
            output.push_str("}\n\n");

            // Add to groups
            groups.push(format!("{}_{}_deser", format_name, bench_def.name));
            groups.push(format!("{}_{}_ser", format_name, bench_def.name));
        }
        _ => {}
    }

    // Generate the benchmark group macros
    // For JSON with JIT, we need separate groups for jit and non-jit
    match format_name {
        "json" => {
            // Non-JIT deserialize group
            output.push_str("#[cfg(not(feature = \"jit\"))]\n");
            output.push_str("gungraun::library_benchmark_group!(\n");
            output.push_str(&format!(
                "    name = {}_{}_deser;\n",
                format_name, bench_def.name
            ));
            output.push_str("    benchmarks =\n");
            output.push_str(&format!(
                "        gungraun_{}_{}_{}_deserialize,\n",
                format_name, bench_def.name, baseline_target
            ));
            output.push_str(&format!(
                "        gungraun_{}_{}_{}_deserialize\n",
                format_name, bench_def.name, t0_target
            ));
            output.push_str(");\n\n");

            // JIT deserialize group - dynamically include only supported tiers
            output.push_str("#[cfg(feature = \"jit\")]\n");
            output.push_str("gungraun::library_benchmark_group!(\n");
            output.push_str(&format!(
                "    name = {}_{}_deser;\n",
                format_name, bench_def.name
            ));
            output.push_str("    benchmarks =\n");
            output.push_str(&format!(
                "        gungraun_{}_{}_{}_deserialize,\n",
                format_name, bench_def.name, baseline_target
            ));
            // Build the list of targets based on what's supported
            let mut jit_targets = vec![t0_target.clone()];
            if format.has_t1() {
                jit_targets.push(t1_target.clone());
            }
            if format.has_t2() {
                jit_targets.push(t2_target.clone());
            }
            // Output all but last with trailing comma
            for (i, target) in jit_targets.iter().enumerate() {
                if i < jit_targets.len() - 1 {
                    output.push_str(&format!(
                        "        gungraun_{}_{}_{}_deserialize,\n",
                        format_name, bench_def.name, target
                    ));
                } else {
                    output.push_str(&format!(
                        "        gungraun_{}_{}_{}_deserialize\n",
                        format_name, bench_def.name, target
                    ));
                }
            }
            output.push_str(");\n\n");

            // Serialize group
            output.push_str("gungraun::library_benchmark_group!(\n");
            output.push_str(&format!(
                "    name = {}_{}_ser;\n",
                format_name, bench_def.name
            ));
            output.push_str("    benchmarks =\n");
            output.push_str(&format!(
                "        gungraun_{}_{}_{}_serialize,\n",
                format_name, bench_def.name, baseline_target
            ));
            output.push_str(&format!(
                "        gungraun_{}_{}_{}_serialize\n",
                format_name, bench_def.name, t0_target
            ));
            output.push_str(");\n\n");
        }
        "postcard" => {
            // Non-JIT deserialize group
            output.push_str("#[cfg(not(feature = \"jit\"))]\n");
            output.push_str("gungraun::library_benchmark_group!(\n");
            output.push_str(&format!(
                "    name = {}_{}_deser;\n",
                format_name, bench_def.name
            ));
            output.push_str("    benchmarks =\n");
            output.push_str(&format!(
                "        gungraun_{}_{}_{}_deserialize,\n",
                format_name, bench_def.name, baseline_target
            ));
            output.push_str(&format!(
                "        gungraun_{}_{}_{}_deserialize\n",
                format_name, bench_def.name, t0_target
            ));
            output.push_str(");\n\n");

            // JIT deserialize group - dynamically include only supported tiers
            output.push_str("#[cfg(feature = \"jit\")]\n");
            output.push_str("gungraun::library_benchmark_group!(\n");
            output.push_str(&format!(
                "    name = {}_{}_deser;\n",
                format_name, bench_def.name
            ));
            output.push_str("    benchmarks =\n");
            output.push_str(&format!(
                "        gungraun_{}_{}_{}_deserialize,\n",
                format_name, bench_def.name, baseline_target
            ));
            // Build the list of targets based on what's supported
            let mut jit_targets = vec![t0_target.clone()];
            if format.has_t1() {
                jit_targets.push(t1_target.clone());
            }
            if format.has_t2() {
                jit_targets.push(t2_target.clone());
            }
            // Output all but last with trailing comma
            for (i, target) in jit_targets.iter().enumerate() {
                if i < jit_targets.len() - 1 {
                    output.push_str(&format!(
                        "        gungraun_{}_{}_{}_deserialize,\n",
                        format_name, bench_def.name, target
                    ));
                } else {
                    output.push_str(&format!(
                        "        gungraun_{}_{}_{}_deserialize\n",
                        format_name, bench_def.name, target
                    ));
                }
            }
            output.push_str(");\n\n");

            // Serialize group
            output.push_str("gungraun::library_benchmark_group!(\n");
            output.push_str(&format!(
                "    name = {}_{}_ser;\n",
                format_name, bench_def.name
            ));
            output.push_str("    benchmarks =\n");
            output.push_str(&format!(
                "        gungraun_{}_{}_{}_serialize,\n",
                format_name, bench_def.name, baseline_target
            ));
            output.push_str(&format!(
                "        gungraun_{}_{}_{}_serialize\n",
                format_name, bench_def.name, t0_target
            ));
            output.push_str(");\n\n");
        }
        _ => {}
    }

    Ok((groups, output))
}

// =============================================================================
// Test Generation
// =============================================================================

fn generate_tests(
    _files: &HashMap<String, BenchmarkFile>,
    _workspace_root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut output = String::new();

    output.push_str("//! ⚠️  AUTO-GENERATED by benchmark-generator ⚠️\n");
    output.push_str("//!\n");
    output.push_str("//! ❌ DO NOT EDIT THIS FILE DIRECTLY\n");
    output.push_str("//! ✅ Instead, edit: facet-perf-shootout/benches/*.kdl\n");
    output.push_str("//!\n");
    output.push_str("//! To regenerate: cargo xtask gen-benchmarks\n\n");
    output.push_str("#![allow(dead_code)]\n\n");

    output.push_str("// Tests - TODO: implement\n");

    Ok(output)
}

// =============================================================================
// JSON Content Helpers
// =============================================================================

fn get_json_content(
    bench_def: &BenchmarkDef,
    workspace_root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(ref json_data) = bench_def.json {
        Ok(json_data.content.clone())
    } else if let Some(ref json_file) = bench_def.json_file {
        let file_path = workspace_root
            .join("facet-json/benches")
            .join(&json_file.path);
        fs::read_to_string(&file_path)
            .map_err(|e| format!("Failed to read {}: {}", file_path.display(), e).into())
    } else if let Some(ref generated) = bench_def.generated {
        generate_json_data(&generated.generator_name)
    } else {
        Err("Benchmark must have 'json', 'json_file', 'json_brotli', or 'generated'".into())
    }
}

fn generate_json_data(generator_name: &str) -> Result<String, Box<dyn std::error::Error>> {
    match generator_name {
        "booleans" => {
            let data: Vec<bool> = (0..10000).map(|i| i % 2 == 0).collect();
            Ok(serde_json::to_string(&data)?)
        }
        "integers" => {
            let data: Vec<u64> = (0..1000).map(|i| i * 12345678901234).collect();
            Ok(serde_json::to_string(&data)?)
        }
        "floats" => {
            let data: Vec<f64> = (0..1000).map(|i| i as f64 * 1.23456789).collect();
            Ok(serde_json::to_string(&data)?)
        }
        "short_strings" => {
            let data: Vec<String> = (0..1000).map(|i| format!("str_{:06}", i)).collect();
            Ok(serde_json::to_string(&data)?)
        }
        "long_strings" => {
            let data: Vec<String> = (0..100)
                .map(|i| "x".repeat(1000) + format!("_{}", i).as_str())
                .collect();
            Ok(serde_json::to_string(&data)?)
        }
        "escaped_strings" => {
            let data: Vec<String> = (0..1000)
                .map(|i| format!("line_{}\nwith\ttabs\tand \"quotes\" and \\backslashes\\", i))
                .collect();
            Ok(serde_json::to_string(&data)?)
        }
        "hashmaps" => {
            let data: std::collections::HashMap<String, u64> =
                (0..1000).map(|i| (format!("key_{}", i), i * 2)).collect();
            Ok(serde_json::to_string(&data)?)
        }
        "nested_structs" => {
            let data: Vec<serde_json::Value> = (0..500)
                .map(|i| {
                    serde_json::json!({
                        "id": i,
                        "inner": {
                            "name": format!("name_{}", i),
                            "value": i as f64 * 1.5,
                            "deep": {
                                "flag": i % 2 == 0,
                                "count": i * 10
                            }
                        }
                    })
                })
                .collect();
            Ok(serde_json::to_string(&data)?)
        }
        "options" => {
            let data: Vec<serde_json::Value> = (0..500)
                .map(|i| {
                    let mut obj = serde_json::json!({ "required": i });
                    if i % 2 == 0 {
                        obj["optional_string"] = serde_json::Value::String(format!("str_{}", i));
                    }
                    if i % 3 == 0 {
                        obj["optional_number"] = serde_json::Value::Number(
                            serde_json::Number::from_f64(i as f64).unwrap(),
                        );
                    }
                    obj
                })
                .collect();
            Ok(serde_json::to_string(&data)?)
        }
        "wide_struct_50" => {
            let mut obj = serde_json::Map::new();
            for i in 0..10 {
                obj.insert(
                    format!("field_{:02}", i),
                    serde_json::json!(i as u64 * 1000),
                );
            }
            for i in 10..20 {
                obj.insert(
                    format!("field_{:02}", i),
                    serde_json::json!(format!("string_{}", i)),
                );
            }
            for i in 20..30 {
                obj.insert(format!("field_{:02}", i), serde_json::json!(i as f64 * 1.5));
            }
            for i in 30..40 {
                obj.insert(format!("field_{:02}", i), serde_json::json!(i % 2 == 0));
            }
            for i in 40..50 {
                obj.insert(
                    format!("field_{:02}", i),
                    serde_json::json!(i as i64 * -100),
                );
            }
            Ok(serde_json::to_string(&obj)?)
        }
        "wide_struct_63" => {
            let mut obj = serde_json::Map::new();
            for i in 0..16 {
                obj.insert(
                    format!("field_{:02}", i),
                    serde_json::json!(i as u64 * 1000),
                );
            }
            for i in 16..32 {
                obj.insert(
                    format!("field_{:02}", i),
                    serde_json::json!(format!("string_{}", i)),
                );
            }
            for i in 32..48 {
                obj.insert(format!("field_{:02}", i), serde_json::json!(i as f64 * 1.5));
            }
            for i in 48..50 {
                obj.insert(format!("field_{:02}", i), serde_json::json!(i % 2 == 0));
            }
            for i in 50..63 {
                if i % 2 == 0 {
                    if i < 58 {
                        obj.insert(format!("field_{:02}", i), serde_json::json!(i % 3 == 0));
                    } else {
                        obj.insert(format!("field_{:02}", i), serde_json::json!(i as i64 * 100));
                    }
                }
            }
            Ok(serde_json::to_string(&obj)?)
        }
        "unknown_fields" => {
            let data: Vec<serde_json::Value> = (0..100)
                .map(|i| {
                    let mut obj = serde_json::json!({
                        "id": i,
                        "name": format!("record_{}", i),
                        "value": i as f64 * 1.5,
                        "active": i % 2 == 0,
                        "count": i * 10,
                        "score": i as f64 * 2.5,
                        "enabled": i % 3 == 0,
                        "index": i * 2,
                        "label": format!("label_{}", i),
                        "flag": i % 5 == 0,
                    });
                    for j in 0..40 {
                        obj[format!("unknown_{:02}", j)] =
                            serde_json::json!(format!("ignored_{}", j));
                    }
                    obj
                })
                .collect();
            Ok(serde_json::to_string(&data)?)
        }
        "deep_nesting_5levels" => {
            let obj = serde_json::json!({
                "id": 1,
                "data": "level1",
                "nested": {
                    "value": 1.5,
                    "flag": true,
                    "nested": {
                        "count": 42,
                        "name": "level3",
                        "nested": {
                            "x": std::f64::consts::PI,
                            "y": std::f64::consts::E,
                            "nested": {
                                "leaf_id": 999,
                                "leaf_value": "bottom",
                                "leaf_flag": false
                            }
                        }
                    }
                }
            });
            Ok(serde_json::to_string(&obj)?)
        }
        "large_strings_escaped" => {
            let data: Vec<String> = (0..50)
                .map(|i| {
                    let size = 1024 + (i * 200);
                    let chunk = "line with\nnewlines and\ttabs and \"quotes\" and \\backslashes\\ ";
                    let repeats = size / chunk.len();
                    chunk.repeat(repeats) + format!("_{}", i).as_str()
                })
                .collect();
            Ok(serde_json::to_string(&data)?)
        }
        "large_strings_unescaped" => {
            let data: Vec<String> = (0..50)
                .map(|i| {
                    let size = 1024 + (i * 200);
                    "x".repeat(size) + format!("_{}", i).as_str()
                })
                .collect();
            Ok(serde_json::to_string(&data)?)
        }
        "big_array_10k" => {
            let data: Vec<i64> = (0..10000).map(|i| i * 123456789).collect();
            Ok(serde_json::to_string(&data)?)
        }
        "flatten_2enums" => {
            let data: Vec<serde_json::Value> = (0..250)
                .flat_map(|i| {
                    vec![
                        serde_json::json!({
                            "name": format!("service_{}", i * 4),
                            "Password": { "password": "secret" },
                            "Tcp": { "tcp_port": 8080 }
                        }),
                        serde_json::json!({
                            "name": format!("service_{}", i * 4 + 1),
                            "Password": { "password": "secret" },
                            "Unix": { "socket_path": "/tmp/sock" }
                        }),
                        serde_json::json!({
                            "name": format!("service_{}", i * 4 + 2),
                            "Token": { "token": "abc123", "token_expiry": 3600 },
                            "Tcp": { "tcp_port": 9090 }
                        }),
                        serde_json::json!({
                            "name": format!("service_{}", i * 4 + 3),
                            "Token": { "token": "xyz789", "token_expiry": 7200 },
                            "Unix": { "socket_path": "/var/run/app.sock" }
                        }),
                    ]
                })
                .collect();
            Ok(serde_json::to_string(&data)?)
        }
        "flatten_4enums" => {
            let data: Vec<serde_json::Value> = (0..64)
                .flat_map(|batch| {
                    let auths = [
                        serde_json::json!({"Password": {"password": "secret"}}),
                        serde_json::json!({"Token": {"token": "abc123", "token_expiry": 3600}}),
                    ];
                    let transports = [
                        serde_json::json!({"Tcp": {"tcp_port": 8080}}),
                        serde_json::json!({"Unix": {"socket_path": "/tmp/sock"}}),
                    ];
                    let storages = [
                        serde_json::json!({"Local": {"local_path": "/data"}}),
                        serde_json::json!({"Remote": {"remote_url": "https://example.com"}}),
                    ];
                    let loggings = [
                        serde_json::json!({"File": {"log_path": "/var/log/app.log"}}),
                        serde_json::json!({"Stdout": {"log_color": true}}),
                    ];

                    let mut configs = Vec::with_capacity(16);
                    let mut idx = 0;
                    for auth in &auths {
                        for transport in &transports {
                            for storage in &storages {
                                for logging in &loggings {
                                    let mut obj = serde_json::json!({
                                        "name": format!("service_{}_{}", batch, idx),
                                    });
                                    if let serde_json::Value::Object(m) = auth {
                                        for (k, v) in m {
                                            obj[k] = v.clone();
                                        }
                                    }
                                    if let serde_json::Value::Object(m) = transport {
                                        for (k, v) in m {
                                            obj[k] = v.clone();
                                        }
                                    }
                                    if let serde_json::Value::Object(m) = storage {
                                        for (k, v) in m {
                                            obj[k] = v.clone();
                                        }
                                    }
                                    if let serde_json::Value::Object(m) = logging {
                                        for (k, v) in m {
                                            obj[k] = v.clone();
                                        }
                                    }
                                    configs.push(obj);
                                    idx += 1;
                                }
                            }
                        }
                    }
                    configs
                })
                .collect();
            Ok(serde_json::to_string(&data)?)
        }
        _ => Err(format!("Unknown JSON generator: {}", generator_name).into()),
    }
}

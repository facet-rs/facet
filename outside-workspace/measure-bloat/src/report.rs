// measure-bloat/src/report.rs

use crate::types::{BuildResult, CrateLlvmLines, CrateRlibSize, CrateSizeChange, LlvmCrateDiff};
use anyhow::Result;
use owo_colors::OwoColorize;
use std::collections::HashMap;

// --- Formatting Utilities ---

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes < KB {
        format!("{} B", bytes)
    } else if bytes < MB {
        format!("{:.2} KiB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.2} MiB", bytes as f64 / MB as f64)
    } else {
        format!("{:.2} GiB", bytes as f64 / GB as f64)
    }
}

fn format_number(num: u64) -> String {
    // Could use num-format crate for thousands separators
    num.to_string()
}

fn format_signed_bytes(bytes: i64) -> String {
    let sign = if bytes < 0 { "-" } else { "+" };
    format!("{}{}", sign, format_bytes(bytes.unsigned_abs()))
}

fn format_signed_number(num: i64) -> String {
    format!("{:+}", num)
}

fn format_percentage_change(current: u64, base: u64) -> String {
    if base == 0 {
        if current > 0 {
            return "(New)".to_string();
        } else {
            return "(N/A)".to_string(); // Or ""
        }
    }
    let delta = current as i64 - base as i64;
    let percentage = (delta as f64 * 100.0) / (base as f64);
    format!("{:+.2}%", percentage)
}

// --- Report Generation ---

/// Wrapper to match expected API; calls generate_comparison_report_content.
pub(crate) fn generate_comparison_report(results: &[BuildResult]) -> Result<String> {
    generate_comparison_report_content(results)
}

/// Generates the main comparison report as a Markdown string.
pub(crate) fn generate_comparison_report_content(results: &[BuildResult]) -> Result<String> {
    let mut report_md = String::new();

    // Report Title
    report_md.push_str("# Measurement Comparison Report\n\n");
    // TODO: Add generated date/time using a chrono dependency if desired
    // report_md.push_str(&format!("Generated on: {}\n\n", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));

    // Group results by target name for structured reporting
    let mut results_by_target: HashMap<String, Vec<&BuildResult>> = HashMap::new();
    for res in results {
        results_by_target
            .entry(res.target_name.clone())
            .or_default()
            .push(res);
    }

    // --- Overall Summary Table ---
    report_md.push_str("## Overall Summary\n\n");
    report_md.push_str("| Target | Variant | Build Time (s) | Executable Size | Total .rlib (Analyzed) | Total LLVM Lines (Analyzed) |\n");
    report_md.push_str("|:-------|:--------|---------------:|----------------:|-------------------------:|----------------------------:|\n");

    let mut target_names_sorted: Vec<_> = results_by_target.keys().cloned().collect();
    target_names_sorted.sort_unstable();

    for target_name in target_names_sorted {
        if let Some(target_results) = results_by_target.get(&target_name) {
            let mut sorted_target_results = target_results.clone();
            sorted_target_results.sort_by_key(|r| match r.variant_name.as_str() {
                "main-facet" => 0,
                "head-facet" => 1,
                "serde" => 2,
                _ => 3,
            });

            for res in sorted_target_results {
                let exec_size_str = res
                    .main_executable_size
                    .map_or_else(|| "N/A".to_string(), format_bytes);

                let total_rlib_analyzed_size =
                    format_bytes(res.rlib_sizes.iter().map(|rs| rs.size).sum());

                let total_llvm_lines_analyzed = res.llvm_lines.as_ref().map_or_else(
                    || "N/A".to_string(),
                    |llvm| format_number(llvm.crate_results.iter().map(|cr| cr.lines).sum()),
                );

                report_md.push_str(&format!(
                    "| {} | {} | {:.2} | {} | {} | {} |\n",
                    target_name,
                    res.variant_name,
                    res.build_time_ms as f64 / 1000.0,
                    exec_size_str,
                    total_rlib_analyzed_size,
                    total_llvm_lines_analyzed
                ));
            }
        }
    }
    report_md.push_str("\n\n");
    log::info!(
        "{} {} Generated overall summary table.",
        "📊".bright_green(),
        "[report]".bright_black()
    );

    // --- Detailed Comparisons per Target ---
    report_md.push_str("## Detailed Comparisons per Target\n\n");

    let mut target_names_for_details: Vec<_> = results_by_target.keys().cloned().collect();
    target_names_for_details.sort_unstable();

    for target_name in target_names_for_details {
        if let Some(target_results) = results_by_target.get(&target_name) {
            report_md.push_str(&format!("### Target: {}\n\n", target_name));

            let head_facet_res = target_results
                .iter()
                .find(|r| r.variant_name == "head-facet");
            let main_facet_res = target_results
                .iter()
                .find(|r| r.variant_name == "main-facet");
            let serde_res = target_results.iter().find(|r| r.variant_name == "serde");

            if let (Some(head), Some(main)) = (head_facet_res, main_facet_res) {
                report_md.push_str("#### HEAD Facet vs. Main Facet\n\n");
                report_md.push_str(&generate_individual_comparison_section(head, main)?);
            }

            if let (Some(head_facet), Some(serde)) = (head_facet_res, serde_res) {
                report_md.push_str("#### HEAD Facet vs. Serde\n\n");
                report_md.push_str(&generate_individual_comparison_section(head_facet, serde)?);
            }
            report_md.push('\n');
        }
    }
    log::info!(
        "{} {} Generated detailed comparison sections.",
        "✅".green(),
        "[report]".bright_black()
    );
    Ok(report_md)
}

/// Generates a Markdown section comparing two BuildResult instances.
fn generate_individual_comparison_section(
    primary_res: &BuildResult,
    baseline_res: &BuildResult,
) -> Result<String> {
    let mut section = String::new();

    section.push_str(&format!(
        "Comparing `{}` (Primary) against `{}` (Baseline):\n\n",
        primary_res.variant_name, baseline_res.variant_name
    ));

    // Build Time
    let build_time_delta = primary_res.build_time_ms as i128 - baseline_res.build_time_ms as i128;
    section.push_str(&format!(
        "- **Build Time**: Primary: {:.2}s, Baseline: {:.2}s, Change: **{:+.2}s** ({})\n",
        primary_res.build_time_ms as f64 / 1000.0,
        baseline_res.build_time_ms as f64 / 1000.0,
        build_time_delta as f64 / 1000.0,
        format_percentage_change(
            primary_res.build_time_ms as u64,
            baseline_res.build_time_ms as u64
        )
    ));

    // Executable Size
    match (
        primary_res.main_executable_size,
        baseline_res.main_executable_size,
    ) {
        (Some(p_fs), Some(b_fs)) => {
            let delta = p_fs as i64 - b_fs as i64;
            section.push_str(&format!(
                "- **Executable Size**: Primary: {}, Baseline: {}, Change: **{}** ({})\n",
                format_bytes(p_fs),
                format_bytes(b_fs),
                format_signed_bytes(delta),
                format_percentage_change(p_fs, b_fs)
            ));
        }
        (Some(p_fs), None) => {
            section.push_str(&format!(
                "- **Executable Size**: Primary: {}, Baseline: N/A\n",
                format_bytes(p_fs)
            ));
        }
        (None, Some(b_fs)) => {
            section.push_str(&format!(
                "- **Executable Size**: Primary: N/A, Baseline: {}\n",
                format_bytes(b_fs)
            ));
        }
        (None, None) => {
            section.push_str("- **Executable Size**: N/A for both variants.\n");
        }
    }
    section.push('\n');

    // .rlib Size Analysis for analyzed crates
    section.push_str(&generate_rlib_size_diff_table(
        &primary_res.rlib_sizes,
        &baseline_res.rlib_sizes,
        &format!(
            ".rlib Sizes for Analyzed Crates ({} vs {})",
            primary_res.variant_name, baseline_res.variant_name
        ),
    ));
    section.push('\n');

    // LLVM Crates Analysis (if available for both)
    if let (Some(primary_llvm), Some(baseline_llvm)) =
        (&primary_res.llvm_lines, &baseline_res.llvm_lines)
    {
        section.push_str(&generate_llvm_crate_diff_table(
            &primary_llvm.crate_results,
            &baseline_llvm.crate_results,
            &format!(
                "LLVM IR Lines per Crate ({} vs {})",
                primary_res.variant_name, baseline_res.variant_name
            ),
        ));
        section.push('\n');

        // Overall LLVM Lines from analyzed crates
        let primary_total_llvm: u64 = primary_llvm.crate_results.iter().map(|c| c.lines).sum();
        let baseline_total_llvm: u64 = baseline_llvm.crate_results.iter().map(|c| c.lines).sum();
        let llvm_delta = primary_total_llvm as i64 - baseline_total_llvm as i64;

        section.push_str(&format!(
            "- **Total LLVM Lines (Analyzed Crates)**: Primary: {}, Baseline: {}, Change: **{}** ({})\n",
            format_number(primary_total_llvm),
            format_number(baseline_total_llvm),
            format_signed_number(llvm_delta),
            format_percentage_change(primary_total_llvm, baseline_total_llvm)
        ));
        section.push('\n');
    }

    Ok(section)
}

/// Generates a Markdown table for comparing .rlib sizes.
fn generate_rlib_size_diff_table(
    primary_rlibs: &[CrateRlibSize],
    baseline_rlibs: &[CrateRlibSize],
    title: &str,
) -> String {
    let mut report_part = String::new();
    report_part.push_str(&format!("**{}**\n\n", title));

    let baseline_map: HashMap<_, _> = baseline_rlibs.iter().map(|c| (&c.name, c.size)).collect();
    let primary_map: HashMap<_, _> = primary_rlibs.iter().map(|c| (&c.name, c.size)).collect();

    let mut all_crate_names: Vec<_> = baseline_map.keys().cloned().collect();
    for name_ref in primary_map.keys() {
        if !baseline_map.contains_key(name_ref) {
            all_crate_names.push(name_ref);
        }
    }
    all_crate_names.sort_unstable();
    all_crate_names.dedup();

    if all_crate_names.is_empty() {
        report_part.push_str("No .rlib data to compare for analyzed crates.\n");
        return report_part;
    }

    let mut changes = Vec::new();
    for name in all_crate_names {
        let p_size = primary_map.get(name).cloned().unwrap_or(0);
        let b_size = baseline_map.get(name).cloned().unwrap_or(0);

        if p_size == 0 && b_size == 0 {
            continue;
        }

        changes.push(CrateSizeChange {
            name: name.clone(),
            base_size: b_size,
            current_size: p_size,
            delta: p_size as i64 - b_size as i64,
        });
    }

    if changes.is_empty() {
        report_part.push_str("No differences in .rlib sizes for analyzed crates.\n");
        return report_part;
    }

    changes.sort_by(|a, b| b.delta.abs().cmp(&a.delta.abs()));

    report_part.push_str("| Crate | Baseline Size | Primary Size | Change | % Change |\n");
    report_part.push_str("|:------|--------------:|-------------:|---------:|---------:|\n");

    for change in changes.iter().take(25) {
        report_part.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            change.name,
            if change.base_size == 0 && change.current_size > 0 {
                "N/A (New)".to_string()
            } else {
                format_bytes(change.base_size)
            },
            if change.current_size == 0 && change.base_size > 0 {
                "N/A (Removed)".to_string()
            } else {
                format_bytes(change.current_size)
            },
            format_signed_bytes(change.delta),
            format_percentage_change(change.current_size, change.base_size)
        ));
    }
    report_part.push('\n');
    report_part
}

/// Generates a Markdown table for comparing LLVM IR lines per crate.
fn generate_llvm_crate_diff_table(
    primary_llvm_crates: &[CrateLlvmLines],
    baseline_llvm_crates: &[CrateLlvmLines],
    title: &str,
) -> String {
    let mut report_part = String::new();
    report_part.push_str(&format!("**{}**\n\n", title));

    let baseline_map: HashMap<_, _> = baseline_llvm_crates
        .iter()
        .map(|c| (&c.name, (c.lines, c.copies)))
        .collect();
    let primary_map: HashMap<_, _> = primary_llvm_crates
        .iter()
        .map(|c| (&c.name, (c.lines, c.copies)))
        .collect();

    let mut all_crate_names: Vec<_> = baseline_map.keys().cloned().collect();
    for name_ref in primary_map.keys() {
        if !baseline_map.contains_key(name_ref) {
            all_crate_names.push(name_ref);
        }
    }
    all_crate_names.sort_unstable();
    all_crate_names.dedup();

    if all_crate_names.is_empty() {
        report_part.push_str("No LLVM lines data to compare for analyzed crates.\n");
        return report_part;
    }

    let mut changes: Vec<LlvmCrateDiff> = Vec::new();

    for name in all_crate_names {
        let (p_lines, p_copies) = primary_map.get(name).cloned().unwrap_or((0, 0));
        let (b_lines, b_copies) = baseline_map.get(name).cloned().unwrap_or((0, 0));

        if p_lines == 0 && b_lines == 0 && p_copies == 0 && b_copies == 0 {
            continue;
        }

        changes.push(LlvmCrateDiff {
            crate_name: name.clone(),
            base_lines: b_lines,
            current_lines: p_lines,
            base_copies: b_copies,
            current_copies: p_copies,
            delta_lines: p_lines as i64 - b_lines as i64,
            delta_copies: p_copies as i64 - b_copies as i64,
        });
    }

    if changes.is_empty() {
        report_part.push_str("No differences in LLVM IR lines for analyzed crates.\n");
        return report_part;
    }

    changes.sort_by(|a, b| b.delta_lines.abs().cmp(&a.delta_lines.abs()));

    report_part.push_str(
        "| Crate | Baseline Lines (Copies) | Primary Lines (Copies) | Δ Lines | Δ Copies |\n",
    );
    report_part.push_str(
        "|:------|------------------------:|-----------------------:|----------:|-----------:|\n",
    );

    for diff_entry in changes.iter().take(25) {
        report_part.push_str(&format!(
            "| `{}` | {} ({}) | {} ({}) | {} | {} |\n",
            diff_entry.crate_name,
            format_number(diff_entry.base_lines),
            format_number(diff_entry.base_copies),
            format_number(diff_entry.current_lines),
            format_number(diff_entry.current_copies),
            format_signed_number(diff_entry.delta_lines),
            format_signed_number(diff_entry.delta_copies)
        ));
    }
    report_part.push('\n');
    report_part
}

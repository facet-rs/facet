// measure-bloat/src/report.rs

use crate::types::{BuildResult, CrateRlibSize, CrateSizeChange};
use anyhow::Result;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use substance::SymbolInfo; // Added for generate_top_symbols_table

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
    report_md.push_str("| Target | Variant | Build Time (s) | Executable Size | Text Section | Total .rlib (Analyzed) | Total Crate Size (Substance) | Total LLVM Lines (Binary) |\n");
    report_md.push_str("|:-------|:--------|---------------:|----------------:|-------------:|-------------------------:|-----------------------------:|--------------------------:|\n");

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
                // Use substance_file_size exclusively
                let exec_size_str = res
                    .substance_analysis_wrapper
                    .as_ref()
                    .map_or_else(|| "N/A".to_string(), |s| format_bytes(s.file_size));

                let text_section_str = res
                    .substance_analysis_wrapper
                    .as_ref()
                    .map_or_else(|| "N/A".to_string(), |s| format_bytes(s.text_size));

                let total_rlib_analyzed_size =
                    format_bytes(res.rlib_sizes.iter().map(|rs| rs.size).sum());

                let total_substance_crate_size_str = res
                    .substance_calculated_crate_contributions // Use the new field
                    .as_ref()
                    .map_or_else(
                        || "N/A".to_string(),
                        |contributions| format_bytes(contributions.values().sum()),
                    );

                // Use llvm_ir_data from the substance_analysis_wrapper
                let total_llvm_lines_str = res
                    .substance_analysis_wrapper
                    .as_ref()
                    .and_then(|s_wrapper| s_wrapper.llvm_ir_data.as_ref())
                    .map_or_else(
                        || "N/A".to_string(), // Fallback if no substance llvm data
                        |s_llvm_data| format_number(s_llvm_data.total_lines),
                    );

                report_md.push_str(&format!(
                    "| {} | {} | {:.2} | {} | {} | {} | {} | {} |\n",
                    target_name,
                    res.variant_name,
                    res.build_time_ms as f64 / 1000.0,
                    exec_size_str,
                    text_section_str, // Added text section string
                    total_rlib_analyzed_size,
                    total_substance_crate_size_str, // Added substance crate size
                    total_llvm_lines_str            // Use new total_llvm_lines_str
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

    // Executable Size (using substance_file_size)
    match (
        primary_res
            .substance_analysis_wrapper
            .as_ref()
            .map(|s| s.file_size),
        baseline_res
            .substance_analysis_wrapper
            .as_ref()
            .map(|s| s.file_size),
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

    // Text Section Size (from substance_text_size)
    match (
        primary_res
            .substance_analysis_wrapper
            .as_ref()
            .map(|s| s.text_size),
        baseline_res
            .substance_analysis_wrapper
            .as_ref()
            .map(|s| s.text_size),
    ) {
        (Some(p_ts), Some(b_ts)) => {
            let delta = p_ts as i64 - b_ts as i64;
            section.push_str(&format!(
                "- **Text Section Size**: Primary: {}, Baseline: {}, Change: **{}** ({})\n",
                format_bytes(p_ts),
                format_bytes(b_ts),
                format_signed_bytes(delta),
                format_percentage_change(p_ts, b_ts)
            ));
        }
        (Some(p_ts), None) => {
            section.push_str(&format!(
                "- **Text Section Size**: Primary: {}, Baseline: N/A\n",
                format_bytes(p_ts)
            ));
        }
        (None, Some(b_ts)) => {
            section.push_str(&format!(
                "- **Text Section Size**: Primary: N/A, Baseline: {}\n",
                format_bytes(b_ts)
            ));
        }
        (None, None) => {
            section.push_str("- **Text Section Size**: N/A for both variants.\n");
        }
    }
    section.push('\n');

    // .rlib Size Analysis for analyzed crates
    // Clarify title for existing .rlib diff table
    section.push_str(&generate_rlib_size_diff_table(
        &primary_res.rlib_sizes,
        &baseline_res.rlib_sizes,
        &format!(
            "Raw .rlib Sizes for Analyzed Crates (Legacy) ({} vs {})",
            primary_res.variant_name, baseline_res.variant_name
        ),
    ));
    section.push('\n');

    // Add new table for Substance Crate Contributions
    section.push_str(&generate_substance_crate_contribution_diff_table(
        primary_res
            .substance_calculated_crate_contributions
            .as_ref(), // Use the new field
        baseline_res
            .substance_calculated_crate_contributions
            .as_ref(), // Use the new field
        &format!(
            "Crate Contributions (from Substance Symbols) ({} vs {})",
            primary_res.variant_name, baseline_res.variant_name
        ),
    ));
    section.push('\n');

    // LLVM IR Analysis (Substance)
    section.push_str("**LLVM IR Analysis (Substance)**\n\n");
    match (
        primary_res
            .substance_analysis_wrapper
            .as_ref()
            .and_then(|s| s.llvm_ir_data.as_ref()),
        baseline_res
            .substance_analysis_wrapper
            .as_ref()
            .and_then(|s| s.llvm_ir_data.as_ref()),
    ) {
        (Some(p_llvm), Some(b_llvm)) => {
            let delta = p_llvm.total_lines as i64 - b_llvm.total_lines as i64;
            section.push_str(&format!(
                "- **Total LLVM Lines (Binary)**: Primary: {}, Baseline: {}, Change: **{}** ({})\n",
                format_number(p_llvm.total_lines),
                format_number(b_llvm.total_lines),
                format_signed_number(delta),
                format_percentage_change(p_llvm.total_lines, b_llvm.total_lines)
            ));

            section.push_str("\n**Top 5 LLVM IR Functions (Primary - from Substance)**:\n\n");
            if !p_llvm.instantiations.is_empty() {
                let mut p_funcs: Vec<_> = p_llvm.instantiations.iter().collect();
                p_funcs.sort_by_key(|(_name, stats)| std::cmp::Reverse(stats.total_lines));
                for (i, (name, stats)) in p_funcs.iter().take(5).enumerate() {
                    section.push_str(&format!(
                        "{}. {} lines ({} copies): `{}`\n",
                        i + 1,
                        stats.total_lines,
                        stats.copies,
                        name
                    ));
                }
            } else {
                section.push_str("No LLVM function instantiation data available for primary variant from Substance.\n");
            }
            section.push('\n');
        }
        (Some(p_llvm), None) => {
            section.push_str(&format!(
                "- **Total LLVM Lines (Binary)**: Primary: {}, Baseline: N/A (No Substance LLVM data)\n",
                format_number(p_llvm.total_lines)
            ));
            section.push_str("\n**Top 5 LLVM IR Functions (Primary - from Substance)**:\n\n");
            if !p_llvm.instantiations.is_empty() {
                let mut p_funcs: Vec<_> = p_llvm.instantiations.iter().collect();
                p_funcs.sort_by_key(|(_name, stats)| std::cmp::Reverse(stats.total_lines));
                for (i, (name, stats)) in p_funcs.iter().take(5).enumerate() {
                    section.push_str(&format!(
                        "{}. {} lines ({} copies): `{}`\n",
                        i + 1,
                        stats.total_lines,
                        stats.copies,
                        name
                    ));
                }
            } else {
                section.push_str("No LLVM function instantiation data available for primary variant from Substance.\n");
            }
            section.push('\n');
        }
        (None, Some(b_llvm)) => {
            section.push_str(&format!(
                "- **Total LLVM Lines (Binary)**: Primary: N/A (No Substance LLVM data), Baseline: {}\n",
                format_number(b_llvm.total_lines)
            ));
        }
        (None, None) => {
            section.push_str(
                "- **Total LLVM Lines (Binary)**: N/A for Substance data in both variants.\n",
            );
        }
    }
    section.push('\n');

    // Top Symbols from Substance
    section.push_str(&generate_top_symbols_table(
        primary_res
            .substance_analysis_wrapper
            .as_ref()
            .map(|s| &s.symbols),
        primary_res
            .substance_analysis_wrapper
            .as_ref()
            .map(|s| s.text_size),
        &primary_res.variant_name,
        10, // Show top 10
    ));
    section.push_str(&generate_top_symbols_table(
        baseline_res
            .substance_analysis_wrapper
            .as_ref()
            .map(|s| &s.symbols),
        baseline_res
            .substance_analysis_wrapper
            .as_ref()
            .map(|s| s.text_size),
        &baseline_res.variant_name,
        10, // Show top 10
    ));

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

/// Generates a Markdown table for comparing crate contributions from Substance.
fn generate_substance_crate_contribution_diff_table(
    primary_contributions_opt: Option<&HashMap<String, u64>>,
    baseline_contributions_opt: Option<&HashMap<String, u64>>,
    title: &str,
) -> String {
    let mut report_part = String::new();
    report_part.push_str(&format!("**{}**\n\n", title));

    let primary_map = primary_contributions_opt.cloned().unwrap_or_default();
    let baseline_map = baseline_contributions_opt.cloned().unwrap_or_default();

    let mut all_crate_names: Vec<_> = baseline_map.keys().cloned().collect();
    for name_ref in primary_map.keys() {
        if !baseline_map.contains_key(name_ref) {
            all_crate_names.push(name_ref.clone());
        }
    }
    all_crate_names.sort_unstable();
    all_crate_names.dedup();

    if all_crate_names.is_empty() {
        report_part.push_str("No Substance crate contribution data to compare.\n");
        return report_part;
    }

    let mut changes = Vec::new();
    for name in all_crate_names {
        let p_size = primary_map.get(&name).cloned().unwrap_or(0);
        let b_size = baseline_map.get(&name).cloned().unwrap_or(0);

        if p_size == 0 && b_size == 0 {
            continue;
        }

        changes.push(CrateSizeChange {
            // Reusing CrateSizeChange for this table too
            name: name.clone(),
            base_size: b_size,
            current_size: p_size,
            delta: p_size as i64 - b_size as i64,
        });
    }

    if changes.is_empty() {
        report_part.push_str("No differences in Substance crate contributions.\n");
        return report_part;
    }

    changes.sort_by(|a, b| b.delta.abs().cmp(&a.delta.abs()));

    report_part.push_str(
        "| Crate | Baseline Size (Substance) | Primary Size (Substance) | Change | % Change |\n",
    );
    report_part.push_str(
        "|:------|--------------------------:|-------------------------:|---------:|---------:|\n",
    );

    for change in changes.iter().take(25) {
        // Limit to top 25 changes
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
    if changes.len() > 25 {
        report_part.push_str(&format!("... and {} more changes.\n", changes.len() - 25));
    }
    report_part.push('\n');
    report_part
}

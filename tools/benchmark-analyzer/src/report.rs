//! HTML report generation using maud.

use crate::parser::{BenchmarkData, Operation};
use maud::{DOCTYPE, Markup, PreEscaped, html};
use std::collections::HashMap;

// Re-export from benchmark_defs
pub use benchmark_defs::load_categories;

/// Target configuration for display
struct TargetConfig {
    label: String,
}

fn get_target_config(target: &str) -> TargetConfig {
    match target {
        "facet_format_jit" => TargetConfig {
            label: "facet-format+jit".to_string(),
        },
        "facet_format_json" => TargetConfig {
            label: "facet-format".to_string(),
        },
        "facet_json" => TargetConfig {
            label: "facet-json".to_string(),
        },
        "facet_json_cranelift" => TargetConfig {
            label: "facet-json+jit".to_string(),
        },
        "serde_json" => TargetConfig {
            label: "serde_json".to_string(),
        },
        _ => TargetConfig {
            label: target.to_string(),
        },
    }
}

/// Format nanoseconds into readable string
fn format_time(ns: f64) -> String {
    if ns < 1_000.0 {
        format!("{:.1} ns", ns)
    } else if ns < 1_000_000.0 {
        format!("{:.2} µs", ns / 1_000.0)
    } else {
        format!("{:.2} ms", ns / 1_000_000.0)
    }
}

/// Format instruction counts into readable string with SI suffixes
fn format_instructions(count: u64) -> String {
    if count < 1_000 {
        format!("{}", count)
    } else if count < 1_000_000 {
        format!("{:.2}K", count as f64 / 1_000.0)
    } else if count < 1_000_000_000 {
        format!("{:.2}M", count as f64 / 1_000_000.0)
    } else {
        format!("{:.2}G", count as f64 / 1_000_000_000.0)
    }
}

/// Generate the complete HTML report
pub fn generate_report(
    data: &BenchmarkData,
    git_info: &GitInfo,
    categories: &HashMap<String, String>,
) -> String {
    // Build section info for sidebar
    let sections = build_sections(data, categories);

    let markup = html! {
        (DOCTYPE)
        html {
            head {
                meta charset="UTF-8";
                title { "facet-json performance tracker" }
                (styles())
                // Load Observable Plot via ES module and expose as global
                script type="module" {
                    (PreEscaped(r#"
import * as Plot from "https://cdn.jsdelivr.net/npm/@observablehq/plot@0.6/+esm";
window.Plot = Plot;
window.dispatchEvent(new Event('plot-ready'));
                    "#))
                }
            }
            body {
                (sidebar(&sections))
                div.container {
                    (header_section(git_info))
                    (summary_chart_section(data, categories))
                    (legend_section())
                    (benchmark_sections(data, categories))
                    (footer_section())
                }
                (interactive_js())
            }
        }
    };
    markup.into_string()
}

/// Section info for sidebar navigation
struct SectionInfo {
    id: String,
    label: String,
    benchmarks: Vec<(String, String)>, // (id, label)
}

/// Build section information from data and categories
fn build_sections(data: &BenchmarkData, categories: &HashMap<String, String>) -> Vec<SectionInfo> {
    let mut sorted_benchmarks: Vec<_> = data.divan.keys().cloned().collect();
    sorted_benchmarks.sort();

    let categorize =
        |name: &str| -> &str { categories.get(name).map(|s| s.as_str()).unwrap_or("other") };

    // Define category order
    let category_order = ["micro", "synthetic", "realistic", "other"];
    let category_labels: HashMap<&str, &str> = [
        ("micro", "Micro Benchmarks"),
        ("synthetic", "Synthetic Benchmarks"),
        ("realistic", "Realistic Benchmarks"),
        ("other", "Other Benchmarks"),
    ]
    .into_iter()
    .collect();

    let mut sections = Vec::new();

    for cat in category_order {
        let benches: Vec<_> = sorted_benchmarks
            .iter()
            .filter(|b| categorize(b) == cat)
            .collect();

        if !benches.is_empty() {
            let benchmarks: Vec<_> = benches
                .iter()
                .map(|b| (format!("bench-{}_deser", b), b.replace('_', " ")))
                .collect();

            sections.push(SectionInfo {
                id: format!("section-{}", cat),
                label: category_labels.get(cat).unwrap_or(&cat).to_string(),
                benchmarks,
            });
        }
    }

    sections
}

/// Sidebar navigation
fn sidebar(sections: &[SectionInfo]) -> Markup {
    html! {
        nav.sidebar id="sidebar" {
            div.sidebar-header {
                "Navigation"
            }
            @for section in sections {
                div.sidebar-section data-section=(section.id) {
                    a.sidebar-category href=(format!("#{}", section.id)) {
                        (section.label)
                    }
                    div.sidebar-benchmarks {
                        @for (bench_id, bench_label) in &section.benchmarks {
                            a.sidebar-benchmark href=(format!("#{}", bench_id)) data-bench=(bench_id) {
                                (bench_label)
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Git information for the report header
pub struct GitInfo {
    pub commit: String,
    pub branch: String,
    pub timestamp: String,
}

fn styles() -> Markup {
    html! {
        style {
            (PreEscaped(r#"
/* Facet Benchmark Report — sober mono theme (light/dark via light-dark()) */

@font-face {
  font-family: 'Iosevka FTL';
  src: url('IosevkaFtl-Regular.ttf') format('truetype');
  font-weight: 400;
  font-style: normal;
  font-display: swap;
}

@font-face {
  font-family: 'Iosevka FTL';
  src: url('IosevkaFtl-Bold.ttf') format('truetype');
  font-weight: 700;
  font-style: normal;
  font-display: swap;
}

* { margin: 0; padding: 0; box-sizing: border-box; }

:root {
  color-scheme: light dark;

  --mono: 'Iosevka FTL', ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
          "Liberation Mono", "Courier New", monospace;

  /* Surfaces */
  --bg:     light-dark(#fbfbfc, #0b0e14);
  --panel:  light-dark(#ffffff, #0f1420);
  --panel2: light-dark(#f6f7f9, #0c111b);

  /* Text */
  --text:  light-dark(#0e1116, #e7eaf0);
  --muted: light-dark(#3a4556, #a3adbd);
  --faint: light-dark(#66738a, #77839a);

  /* Lines */
  --border:  light-dark(rgba(0,0,0,0.08), rgba(255,255,255,0.08));
  --border2: light-dark(rgba(0,0,0,0.14), rgba(255,255,255,0.14));

  /* Semantic */
  --accent: light-dark(#2457f5, #7aa2f7);
  --good:   light-dark(#1a7f37, #7ee787);
  --warn:   light-dark(#9a6700, #f2cc60);
  --bad:    light-dark(#cf222e, #ff7b72);

  /* Neutrals for charts */
  --chart:  light-dark(#223047, #a3b2cc);
  --chart-fade: light-dark(rgba(34,48,71,0.25), rgba(163,178,204,0.20));

  /* Sizing */
  --w: 1120px;
  --s1: 4px;
  --s2: 8px;
  --s3: 12px;
  --s4: 16px;
  --s5: 20px;
  --s6: 32px;
}

html, body { height: 100%; }

body {
  font-family: var(--mono);
  background: var(--bg);
  color: var(--text);
  line-height: 1.5;
  font-variant-numeric: tabular-nums;
  text-rendering: geometricPrecision;
}

.container {
  max-width: var(--w);
  margin: 0 auto;
  padding: 20px 16px 40px;
}

/* Header */
header {
  background: transparent;
  color: inherit;
  padding: 0 0 var(--s4);
  border-radius: 0;
  margin-bottom: var(--s5);
  box-shadow: none;
  border-bottom: 1px solid var(--border);
}

h1 {
  font-size: 24px;
  font-weight: 650;
  letter-spacing: -0.01em;
  margin-bottom: var(--s2);
}

.meta {
  color: var(--muted);
  font-size: 12px;
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
}
.meta-item { margin-right: 0; }
.meta strong { color: var(--text); font-weight: 650; }

/* Section headers */
.section-header {
  background: transparent;
  padding: var(--s5) 0 var(--s2);
  border-radius: 0;
  margin: var(--s6) 0 var(--s2);
  border-left: none;
  border-top: 1px solid var(--border);
}

h2 {
  color: var(--text);
  font-size: 18px;
  font-weight: 650;
  margin: 0;
  padding: 0;
  border-bottom: none;
}

h3 {
  color: var(--muted);
  margin: 0 0 var(--s3);
  font-size: 14px;
  font-weight: 650;
}

/* Panels */
.benchmark-item,
.summary-chart,
.legend {
  background: var(--panel);
  padding: var(--s4);
  margin: var(--s4) 0;
  border: 1px solid var(--border);
}

/* Legend */
.legend h3 { color: var(--text); margin-bottom: var(--s2); }
.legend-item {
  display: inline-flex;
  align-items: baseline;
  gap: 8px;
  margin: 4px 10px 4px 0;
  padding: 4px 8px;
  background: transparent;
  border: 1px solid var(--border);
  color: var(--muted);
}
.legend-item strong { color: var(--text); font-weight: 650; }

/* Table/chart layout */
.table-chart-container {
  display: grid;
  grid-template-columns: 1.1fr 0.9fr;
  gap: var(--s5);
  margin-top: var(--s3);
}
@media (max-width: 1100px) {
  .table-chart-container { grid-template-columns: 1fr; }
}

/* Tables */
table { width: 100%; border-collapse: collapse; }

th, td {
  padding: 6px 10px;
  border-bottom: 1px solid var(--border);
  vertical-align: middle;
}

th {
  background: transparent;
  color: var(--muted);
  text-align: left;
  font-weight: 650;
  font-size: 12px;
  position: sticky;
  top: 0;
  backdrop-filter: blur(6px);
}

td { font-size: 13px; }

.metric { font-family: var(--mono); font-size: 13px; color: var(--text); }

td.num, th.num { text-align: right; white-space: nowrap; }

tbody tr:hover {
  background: color-mix(in srgb, var(--panel2) 75%, transparent) !important;
  cursor: default;
  transition: background 0.12s;
}

tr.dimmed { opacity: 0.25; transition: opacity 0.12s; }

/* Row semantics (subtle, no paint buckets) */
.fastest,
.jit-highlight,
.baseline {
  background: transparent;
  font-weight: 650;
  border-left: 0;
}

.fastest td:first-child {
  border-left: 3px solid var(--good);
  padding-left: 9px;
}
.baseline td:first-child {
  border-left: 3px solid var(--muted);
  padding-left: 9px;
}
.jit-highlight td:first-child {
  border-left: 3px solid var(--accent);
  padding-left: 9px;
}

/* Baseline gets a stronger separator */
tr.baseline td { border-top: 1px solid var(--border2); }

/* Speed indicators: not color-only */
.speedup, .neutral, .slowdown { font-weight: 650; }
.speedup { color: var(--good); }
.neutral { color: var(--muted); }
.slowdown { color: var(--bad); }

/* Text labels for speed indicators */
.k { color: var(--muted); font-weight: 500; font-size: 12px; margin-left: 6px; }

/* Chart wrapper */
.chart-wrapper {
  padding: 8px;
  border: 1px solid var(--border);
  background: var(--panel2);
}

/* Summary chart */
.summary-chart h2 {
  margin: 0 0 var(--s3) 0;
  border-bottom: none;
}

.summary-chart-wrapper {
  padding: 8px;
  border: 1px solid var(--border);
  background: var(--panel2);
}

/* Observable Plot SVG styling - inherits from CSS vars */
.chart-wrapper svg,
.summary-chart-wrapper svg {
  display: block;
  font-family: var(--mono);
  overflow: visible;
}

/* Axis text */
.chart-wrapper [aria-label="y-axis tick label"],
.chart-wrapper [aria-label="x-axis tick label"],
.summary-chart-wrapper [aria-label="y-axis tick label"],
.summary-chart-wrapper [aria-label="x-axis tick label"] {
  fill: var(--muted);
  font-size: 11px;
}

/* Axis lines */
.chart-wrapper [aria-label="y-axis tick"],
.chart-wrapper [aria-label="x-axis tick"],
.summary-chart-wrapper [aria-label="y-axis tick"],
.summary-chart-wrapper [aria-label="x-axis tick"] {
  stroke: var(--border);
}

/* Grid lines */
.chart-wrapper g[aria-label="x-axis grid"] line,
.summary-chart-wrapper g[aria-label="x-axis grid"] line,
.summary-chart-wrapper g[aria-label="y-axis grid"] line {
  stroke: var(--border);
}

/* Bar styling for individual benchmark charts */
.chart-wrapper rect {
  fill: var(--chart-fade);
  stroke: var(--chart);
  stroke-width: 1;
  transition: fill 0.15s, stroke 0.15s, opacity 0.15s;
}

/* Highlighted bar */
.chart-wrapper rect.highlighted {
  fill: var(--accent);
  fill-opacity: 0.6;
  stroke: var(--accent);
  stroke-width: 2;
}

/* Dimmed bars */
.chart-wrapper rect.dimmed {
  opacity: 0.3;
}

/* Summary chart - don't override Plot's colors, just add transitions */
.summary-chart-wrapper rect {
  transition: opacity 0.15s;
}

/* Axis labels */
.chart-wrapper text[aria-label="x-axis label"],
.summary-chart-wrapper text[aria-label="y-axis label"] {
  fill: var(--muted);
  font-size: 12px;
}

/* Footer */
footer {
  text-align: left;
  margin-top: 40px;
  padding-top: 16px;
  color: var(--muted);
  font-size: 12px;
  border-top: 1px solid var(--border);
}

/* Sidebar */
.sidebar {
  position: fixed;
  left: 0;
  top: 0;
  bottom: 0;
  width: 220px;
  background: var(--panel);
  border-right: 1px solid var(--border);
  overflow-y: auto;
  padding: var(--s3);
  z-index: 100;
  font-size: 12px;
}

.sidebar-header {
  font-weight: 650;
  color: var(--text);
  padding: var(--s2) var(--s2) var(--s3);
  border-bottom: 1px solid var(--border);
  margin-bottom: var(--s3);
}

.sidebar-section {
  margin-bottom: var(--s3);
}

.sidebar-category {
  display: block;
  padding: var(--s1) var(--s2);
  color: var(--text);
  font-weight: 650;
  text-decoration: none;
  border-radius: 4px;
  transition: background 0.15s;
}

.sidebar-category:hover {
  background: var(--panel2);
}

.sidebar-category.active {
  background: color-mix(in srgb, var(--accent) 15%, transparent);
  color: var(--accent);
}

.sidebar-benchmarks {
  padding-left: var(--s3);
  margin-top: var(--s1);
}

.sidebar-benchmark {
  display: block;
  padding: 3px var(--s2);
  color: var(--muted);
  text-decoration: none;
  border-radius: 4px;
  transition: background 0.15s, color 0.15s;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.sidebar-benchmark:hover {
  background: var(--panel2);
  color: var(--text);
}

.sidebar-benchmark.active {
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  color: var(--accent);
}

/* Adjust container for sidebar */
body {
  padding-left: 220px;
}

@media (max-width: 900px) {
  .sidebar {
    display: none;
  }
  body {
    padding-left: 0;
  }
}
            "#))
        }
    }
}

fn header_section(git_info: &GitInfo) -> Markup {
    let commit_url = format!(
        "https://github.com/facet-rs/facet/commit/{}",
        git_info.commit
    );
    let branch_url = format!("https://github.com/facet-rs/facet/tree/{}", git_info.branch);

    html! {
        header {
            h1 { "facet-json performance tracker" }
            div.meta {
                span.meta-item {
                    strong { "Generated: " }
                    (git_info.timestamp)
                }
                span.meta-item {
                    strong { "Commit: " }
                    a href=(commit_url) target="_blank" { (git_info.commit) }
                }
                span.meta-item {
                    strong { "Branch: " }
                    a href=(branch_url) target="_blank" { (git_info.branch) }
                }
            }
        }
    }
}

fn summary_chart_section(data: &BenchmarkData, categories: &HashMap<String, String>) -> Markup {
    let category_order = ["micro", "synthetic", "realistic"];
    let category_labels: HashMap<&str, &str> = [
        ("micro", "Micro Benchmarks"),
        ("synthetic", "Synthetic Benchmarks"),
        ("realistic", "Realistic Benchmarks"),
    ]
    .into_iter()
    .collect();

    let jit_config = get_target_config("facet_format_jit");
    let serde_config = get_target_config("serde_json");

    html! {
        @for cat in &category_order {
            (summary_chart_for_category(data, categories, cat, category_labels.get(cat).unwrap_or(cat), &jit_config, &serde_config))
        }
    }
}

fn summary_chart_for_category(
    data: &BenchmarkData,
    categories: &HashMap<String, String>,
    category: &str,
    category_label: &str,
    jit_config: &TargetConfig,
    serde_config: &TargetConfig,
) -> Markup {
    // Collect data for this category
    let mut benchmarks: Vec<(String, Option<f64>, Option<f64>)> = Vec::new();

    let mut sorted_names: Vec<_> = data.divan.keys().cloned().collect();
    sorted_names.sort();

    for bench_name in &sorted_names {
        // Filter by category
        let bench_category = categories
            .get(bench_name)
            .map(|s| s.as_str())
            .unwrap_or("other");
        if bench_category != category {
            continue;
        }

        if let Some(ops) = data.divan.get(bench_name)
            && let Some(targets) = ops.get(&Operation::Deserialize)
        {
            let jit = targets.get("facet_format_jit").copied();
            let serde = targets.get("serde_json").copied();
            if jit.is_some() || serde.is_some() {
                benchmarks.push((bench_name.replace('_', " "), jit, serde));
            }
        }
    }

    if benchmarks.is_empty() {
        return html! {};
    }

    // Build data array for Observable Plot (grouped bar chart)
    let mut chart_data: Vec<serde_json::Value> = Vec::new();
    for (name, jit, serde) in &benchmarks {
        if let Some(j) = jit {
            chart_data.push(serde_json::json!({
                "benchmark": name,
                "target": jit_config.label,
                "time": j / 1000.0
            }));
        }
        if let Some(s) = serde {
            chart_data.push(serde_json::json!({
                "benchmark": name,
                "target": serde_config.label,
                "time": s / 1000.0
            }));
        }
    }

    let chart_id = format!("summary-chart-{}", category);

    html! {
        div.summary-chart {
            h2 { (category_label) ": facet-format+jit vs serde_json" }
            div.summary-chart-wrapper id=(chart_id) {}
            script {
                (PreEscaped(format!(r#"
(function() {{
    function render() {{
        const data = {};
        const container = document.getElementById('{}');
        const width = container.clientWidth || 600;

        // Detect dark mode for color selection
        const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        const accentColor = isDark ? '#7aa2f7' : '#2457f5';
        const mutedColor = isDark ? '#6b7280' : '#9ca3af';

        const chart = Plot.plot({{
            width: width,
            height: Math.max(200, data.length * 20 + 60),
            marginLeft: 140,
            marginRight: 40,
            x: {{
                label: "Time (µs)",
                grid: true
            }},
            y: {{
                label: null
            }},
            color: {{
                domain: ["{}","{}"],
                range: [accentColor, mutedColor],
                legend: true
            }},
            marks: [
                Plot.barX(data, {{
                    x: "time",
                    y: "benchmark",
                    fill: "target",
                    sort: {{ y: "-x" }}
                }}),
                Plot.ruleX([0])
            ]
        }});

        container.appendChild(chart);
    }}
    if (window.Plot) render();
    else window.addEventListener('plot-ready', render);
}})();
"#,
                    serde_json::to_string(&chart_data).unwrap_or_default(),
                    chart_id,
                    jit_config.label,
                    serde_config.label,
                )))
            }
        }
    }
}

fn legend_section() -> Markup {
    let targets = [
        ("facet_format_jit", "Format-agnostic JIT (our work!)"),
        ("facet_json_cranelift", "JSON-specific JIT"),
        ("facet_format_json", "Format-agnostic, no JIT"),
        ("facet_json", "JSON-specific, no JIT"),
        ("serde_json", "The baseline to beat"),
    ];

    html! {
        div.legend {
            h3 { "Targets" }
            div {
                @for (target, desc) in &targets {
                    @let config = get_target_config(target);
                    span.legend-item {
                        strong { (config.label) }
                        @if !desc.is_empty() {
                            " — " (*desc)
                        }
                    }
                }
            }
        }
    }
}

fn benchmark_sections(data: &BenchmarkData, categories: &HashMap<String, String>) -> Markup {
    let mut sorted_benchmarks: Vec<_> = data.divan.keys().cloned().collect();
    sorted_benchmarks.sort();

    let categorize =
        |name: &str| -> &str { categories.get(name).map(|s| s.as_str()).unwrap_or("other") };

    // Define category order and labels
    let category_order = ["micro", "synthetic", "realistic", "other"];
    let category_labels: HashMap<&str, &str> = [
        ("micro", "Micro Benchmarks"),
        ("synthetic", "Synthetic Benchmarks"),
        ("realistic", "Realistic Benchmarks"),
        ("other", "Other Benchmarks"),
    ]
    .into_iter()
    .collect();

    html! {
        @for cat in &category_order {
            @let benches: Vec<_> = sorted_benchmarks.iter()
                .filter(|b| categorize(b) == *cat)
                .collect();
            @if !benches.is_empty() {
                div.section-header id=(format!("section-{}", cat)) {
                    h2 { (category_labels.get(cat).unwrap_or(cat)) }
                }
                @for bench_name in &benches {
                    (benchmark_item(bench_name, data))
                }
            }
        }
    }
}

fn benchmark_item(bench_name: &str, data: &BenchmarkData) -> Markup {
    let ops = data.divan.get(bench_name);

    html! {
        @if let Some(ops) = ops {
            @for op in &[Operation::Deserialize, Operation::Serialize] {
                @if let Some(targets) = ops.get(op) {
                    @if !targets.is_empty() {
                        @let bench_id = format!("{}_{}", bench_name, match op {
                            Operation::Deserialize => "deser",
                            Operation::Serialize => "ser",
                        });
                        (benchmark_table_and_chart(bench_name, *op, targets, &bench_id, data))
                    }
                }
            }
        }
    }
}

fn benchmark_table_and_chart(
    bench_name: &str,
    operation: Operation,
    targets: &HashMap<String, f64>,
    bench_id: &str,
    data: &BenchmarkData,
) -> Markup {
    let serde_time = targets.get("serde_json").copied();
    let fastest_time = targets.values().copied().fold(f64::INFINITY, f64::min);

    // Sort by time (fastest first)
    let mut sorted: Vec<_> = targets.iter().collect();
    sorted.sort_by(|a, b| a.1.partial_cmp(b.1).unwrap());

    // Prepare chart data for Observable Plot
    let chart_data: Vec<serde_json::Value> = sorted
        .iter()
        .map(|(t, v)| {
            serde_json::json!({
                "target": t.to_string(),
                "label": get_target_config(t).label,
                "time": **v / 1000.0
            })
        })
        .collect();

    html! {
        div.benchmark-item id=(format!("bench-{}", bench_id)) {
            h3 {
                (bench_name.replace('_', " "))
                " — "
                (operation.to_string())
            }

            div.table-chart-container {
                div.table-wrapper {
                    table id=(format!("table-{}", bench_id)) {
                        thead {
                            tr {
                                th { "Target" }
                                th.num { "Median Time" }
                                th.num { "Instructions" }
                                th.num { "vs serde_json" }
                            }
                        }
                        tbody {
                            @for (target, time_ns) in &sorted {
                                @let config = get_target_config(target);
                                @let instructions = data.gungraun.get(&(bench_name.to_string(), (*target).clone()));
                                @let vs_serde = serde_time.map(|s| *time_ns / s);

                                @let row_class = if **time_ns == fastest_time {
                                    "fastest"
                                } else if *target == "serde_json" {
                                    "baseline"
                                } else if *target == "facet_format_jit" {
                                    "jit-highlight"
                                } else {
                                    ""
                                };

                                @let (vs_serde_class, speed_label) = match vs_serde {
                                    Some(r) if r < 1.0 => ("speedup", "faster"),
                                    Some(r) if r > 1.0 => ("slowdown", "slower"),
                                    Some(_) => ("neutral", ""),
                                    None => ("", ""),
                                };

                                tr class=(row_class) data-target=(*target)
                                   onmouseenter=(format!("highlightChart('{}', '{}')", bench_id, target))
                                   onmouseleave=(format!("unhighlightChart('{}')", bench_id)) {
                                    td { (config.label) }
                                    td.metric.num { (format_time(**time_ns)) }
                                    td.metric.num {
                                        @if let Some(i) = instructions {
                                            (format_instructions(*i))
                                        } @else {
                                            "-"
                                        }
                                    }
                                    td class=(format!("num {}", vs_serde_class)) {
                                        @if let Some(r) = vs_serde {
                                            span.metric { (format!("{:.2}×", r)) }
                                            @if !speed_label.is_empty() {
                                                span.k { (speed_label) }
                                            }
                                        } @else {
                                            "-"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div.chart-wrapper id=(format!("chart-{}", bench_id)) {}
            }

            // Observable Plot chart initialization
            script {
                (PreEscaped(format!(r#"
(function() {{
    function render() {{
        const data = {};
        const container = document.getElementById('chart-{}');
        const width = container.clientWidth || 400;
        const barHeight = 28;

        const chart = Plot.plot({{
            width: width,
            height: data.length * barHeight + 50,
            marginLeft: 130,
            marginRight: 20,
            marginTop: 10,
            marginBottom: 40,
            x: {{
                label: "Time (µs)",
                grid: true
            }},
            y: {{
                label: null
            }},
            marks: [
                Plot.barX(data, {{
                    x: "time",
                    y: "label",
                    sort: {{ y: "x" }}
                }}),
                Plot.ruleX([0])
            ]
        }});

        container.appendChild(chart);

        // Add data-target attributes to bars for hover interaction
        const bars = chart.querySelectorAll('rect');
        data.sort((a, b) => a.time - b.time).forEach((d, i) => {{
            if (bars[i]) {{
                bars[i].setAttribute('data-target', d.target);
            }}
        }});
    }}
    if (window.Plot) render();
    else window.addEventListener('plot-ready', render);
}})();
"#,
                    serde_json::to_string(&chart_data).unwrap_or_default(),
                    bench_id,
                )))
            }
        }
    }
}

fn footer_section() -> Markup {
    html! {
        footer {
            p {
                strong { "Generated by " }
                "benchmark-analyzer"
            }
            p { "Benchmark tools: divan (wall-clock) + gungraun (instruction counts)" }
        }
    }
}

fn interactive_js() -> Markup {
    html! {
        script {
            (PreEscaped(r#"
// Highlight chart bars when hovering table rows (SVG version)
function highlightChart(benchId, targetName) {
    const chartContainer = document.getElementById('chart-' + benchId);
    const table = document.getElementById('table-' + benchId);

    if (!chartContainer) return;

    // Dim non-hovered table rows
    if (table) {
        const rows = table.querySelectorAll('tbody tr');
        rows.forEach(row => {
            if (row.getAttribute('data-target') === targetName) {
                row.classList.remove('dimmed');
            } else {
                row.classList.add('dimmed');
            }
        });
    }

    // Highlight/dim SVG bars
    const bars = chartContainer.querySelectorAll('rect[data-target]');
    bars.forEach(bar => {
        if (bar.getAttribute('data-target') === targetName) {
            bar.classList.add('highlighted');
            bar.classList.remove('dimmed');
        } else {
            bar.classList.add('dimmed');
            bar.classList.remove('highlighted');
        }
    });
}

function unhighlightChart(benchId) {
    const chartContainer = document.getElementById('chart-' + benchId);
    const table = document.getElementById('table-' + benchId);

    // Reset table rows
    if (table) {
        const rows = table.querySelectorAll('tbody tr');
        rows.forEach(row => row.classList.remove('dimmed'));
    }

    // Reset SVG bars
    if (chartContainer) {
        const bars = chartContainer.querySelectorAll('rect[data-target]');
        bars.forEach(bar => {
            bar.classList.remove('highlighted', 'dimmed');
        });
    }
}

window.highlightChart = highlightChart;
window.unhighlightChart = unhighlightChart;

// Sidebar scroll highlighting
(function() {
    const sidebar = document.getElementById('sidebar');
    if (!sidebar) return;

    const sectionHeaders = document.querySelectorAll('.section-header[id]');
    const benchmarkItems = document.querySelectorAll('.benchmark-item[id]');
    const sidebarCategories = sidebar.querySelectorAll('.sidebar-category');
    const sidebarBenchmarks = sidebar.querySelectorAll('.sidebar-benchmark');

    // Map from element IDs to sidebar links
    const sectionMap = new Map();
    sidebarCategories.forEach(link => {
        const href = link.getAttribute('href');
        if (href && href.startsWith('#')) {
            sectionMap.set(href.slice(1), link);
        }
    });

    const benchMap = new Map();
    sidebarBenchmarks.forEach(link => {
        const benchId = link.getAttribute('data-bench');
        if (benchId) {
            benchMap.set(benchId, link);
        }
    });

    function updateSidebarHighlight() {
        const scrollTop = window.scrollY;
        const viewportHeight = window.innerHeight;
        const offset = 100; // How far from top to consider "active"

        // Find active section
        let activeSection = null;
        sectionHeaders.forEach(header => {
            const rect = header.getBoundingClientRect();
            if (rect.top <= offset) {
                activeSection = header.id;
            }
        });

        // Find active benchmark (closest to top of viewport)
        let activeBench = null;
        let closestDistance = Infinity;
        benchmarkItems.forEach(item => {
            const rect = item.getBoundingClientRect();
            if (rect.top <= offset && rect.bottom > 0) {
                const distance = Math.abs(rect.top - offset);
                if (distance < closestDistance) {
                    closestDistance = distance;
                    activeBench = item.id;
                }
            }
        });

        // Update sidebar highlighting
        sidebarCategories.forEach(link => link.classList.remove('active'));
        sidebarBenchmarks.forEach(link => link.classList.remove('active'));

        if (activeSection && sectionMap.has(activeSection)) {
            sectionMap.get(activeSection).classList.add('active');
        }
        if (activeBench && benchMap.has(activeBench)) {
            benchMap.get(activeBench).classList.add('active');
            // Also scroll sidebar to show active item
            const activeLink = benchMap.get(activeBench);
            const sidebarRect = sidebar.getBoundingClientRect();
            const linkRect = activeLink.getBoundingClientRect();
            if (linkRect.top < sidebarRect.top || linkRect.bottom > sidebarRect.bottom) {
                activeLink.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
            }
        }
    }

    // Throttle scroll handler
    let ticking = false;
    window.addEventListener('scroll', () => {
        if (!ticking) {
            window.requestAnimationFrame(() => {
                updateSidebarHighlight();
                ticking = false;
            });
            ticking = true;
        }
    });

    // Initial highlight
    updateSidebarHighlight();
})();
            "#))
        }
    }
}

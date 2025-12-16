//! HTML report generation using maud.

use crate::parser::{BenchmarkData, Operation};
use maud::{DOCTYPE, Markup, PreEscaped, html};
use std::collections::HashMap;

/// Target configuration for display
struct TargetConfig {
    emoji: &'static str,
    label: String,
    color: &'static str,
}

fn get_target_config(target: &str) -> TargetConfig {
    match target {
        "facet_format_jit" => TargetConfig {
            emoji: "⚡",
            label: "facet-format+jit".to_string(),
            color: "#FFD700",
        },
        "facet_format_json" => TargetConfig {
            emoji: "📦",
            label: "facet-format".to_string(),
            color: "#FF6B6B",
        },
        "facet_json" => TargetConfig {
            emoji: "🔧",
            label: "facet-json".to_string(),
            color: "#4ECDC4",
        },
        "facet_json_cranelift" => TargetConfig {
            emoji: "🚀",
            label: "facet-json+jit".to_string(),
            color: "#95E1D3",
        },
        "serde_json" => TargetConfig {
            emoji: "🎯",
            label: "serde_json".to_string(),
            color: "#9B59B6",
        },
        _ => TargetConfig {
            emoji: "❓",
            label: target.to_string(),
            color: "#888888",
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
pub fn generate_report(data: &BenchmarkData, git_info: &GitInfo) -> String {
    let markup = html! {
        (DOCTYPE)
        html {
            head {
                meta charset="UTF-8";
                title { "Facet JIT Benchmark Report" }
                script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.0/dist/chart.umd.min.js" {}
                (styles())
            }
            body {
                div.container {
                    (header_section(git_info))
                    (summary_chart_section(data))
                    (legend_section())
                    (benchmark_sections(data))
                    (footer_section())
                }
                (interactive_js())
            }
        }
    };
    markup.into_string()
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
* { margin: 0; padding: 0; box-sizing: border-box; }

body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: #f8f9fa;
    color: #212529;
    line-height: 1.6;
}

.container {
    max-width: 1800px;
    margin: 0 auto;
    padding: 20px;
}

header {
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    color: white;
    padding: 40px;
    border-radius: 12px;
    margin-bottom: 30px;
    box-shadow: 0 4px 6px rgba(0,0,0,0.1);
}

h1 { font-size: 2.5em; margin-bottom: 10px; }
h2 {
    color: #495057;
    margin: 40px 0 20px 0;
    padding-bottom: 10px;
    border-bottom: 3px solid #dee2e6;
}
h3 {
    color: #6c757d;
    margin: 25px 0 15px 0;
    font-size: 1.3em;
}

.meta { opacity: 0.95; font-size: 0.95em; margin-top: 10px; }
.meta-item { display: inline-block; margin-right: 30px; }

.legend {
    background: white;
    padding: 25px;
    border-radius: 12px;
    margin: 20px 0;
    box-shadow: 0 2px 8px rgba(0,0,0,0.08);
}
.legend h3 { margin-top: 0; color: #495057; }
.legend-item {
    display: inline-block;
    margin: 10px 20px 10px 0;
    padding: 8px 15px;
    background: #f8f9fa;
    border-radius: 6px;
    border-left: 4px solid;
}

.benchmark-item {
    background: white;
    padding: 30px;
    margin: 30px 0;
    border-radius: 12px;
    box-shadow: 0 2px 8px rgba(0,0,0,0.08);
}

.table-chart-container {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 30px;
    margin-top: 20px;
}

@media (max-width: 1200px) {
    .table-chart-container {
        grid-template-columns: 1fr;
    }
}

table {
    width: 100%;
    border-collapse: collapse;
}

th {
    background: linear-gradient(to bottom, #4CAF50, #45a049);
    color: white;
    padding: 12px;
    text-align: left;
    font-weight: 600;
    position: sticky;
    top: 0;
}

td {
    padding: 12px;
    border-bottom: 1px solid #e9ecef;
}

tr:hover {
    background: #e3f2fd !important;
    cursor: pointer;
    transition: background 0.15s;
}

tr.dimmed {
    opacity: 0.3;
    transition: opacity 0.15s;
}

.fastest {
    background: #c8e6c9;
    border-left: 4px solid #2e7d32;
}

.jit-highlight {
    background: #fff9c4;
    border-left: 4px solid #f57f17;
    font-weight: 600;
}

.baseline {
    background: #e1bee7;
    border-left: 4px solid #7b1fa2;
    font-weight: 600;
}

.emoji {
    font-size: 1.2em;
    margin-right: 5px;
}

.metric {
    font-family: 'SF Mono', Monaco, 'Courier New', monospace;
    font-size: 0.95em;
}

.speedup { color: #2e7d32; font-weight: 600; }
.neutral { color: #f57f17; }
.slowdown { color: #c62828; }

.chart-wrapper {
    position: relative;
    height: 300px;
}

canvas {
    max-height: 300px;
}

.section-header {
    background: #e3f2fd;
    padding: 15px 25px;
    border-radius: 8px;
    margin: 30px 0 15px 0;
    border-left: 5px solid #1976d2;
}

.summary-chart {
    background: white;
    padding: 30px;
    margin: 30px 0;
    border-radius: 12px;
    box-shadow: 0 2px 8px rgba(0,0,0,0.08);
}

.summary-chart h2 {
    margin: 0 0 20px 0;
    border-bottom: none;
}

.summary-chart-wrapper {
    position: relative;
    height: 400px;
}

footer {
    text-align: center;
    margin-top: 60px;
    padding: 30px;
    color: #6c757d;
    font-size: 0.9em;
    border-top: 2px solid #dee2e6;
}
            "#))
        }
    }
}

fn header_section(git_info: &GitInfo) -> Markup {
    html! {
        header {
            h1 { "🚀 Facet JIT Benchmark Report" }
            div.meta {
                span.meta-item {
                    strong { "Generated: " }
                    (git_info.timestamp)
                }
                span.meta-item {
                    strong { "Commit: " }
                    (git_info.commit)
                }
                span.meta-item {
                    strong { "Branch: " }
                    (git_info.branch)
                }
            }
        }
    }
}

fn summary_chart_section(data: &BenchmarkData) -> Markup {
    // Collect data for facet_format_jit vs serde_json across all benchmarks (deserialize only)
    let mut benchmarks: Vec<(String, Option<f64>, Option<f64>)> = Vec::new();

    let mut sorted_names: Vec<_> = data.divan.keys().cloned().collect();
    sorted_names.sort();

    for bench_name in &sorted_names {
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

    let labels: Vec<_> = benchmarks.iter().map(|(name, _, _)| name.clone()).collect();
    let jit_values: Vec<_> = benchmarks
        .iter()
        .map(|(_, jit, _)| jit.map(|v| v / 1000.0).unwrap_or(0.0))
        .collect();
    let serde_values: Vec<_> = benchmarks
        .iter()
        .map(|(_, _, serde)| serde.map(|v| v / 1000.0).unwrap_or(0.0))
        .collect();

    let jit_config = get_target_config("facet_format_jit");
    let serde_config = get_target_config("serde_json");

    html! {
        div.summary-chart {
            h2 { "📊 Summary: facet-format+jit vs serde_json (Deserialize)" }
            div.summary-chart-wrapper {
                canvas id="summary-chart" {}
            }
            script {
                (PreEscaped(format!(r#"
(function() {{
    const ctx = document.getElementById('summary-chart').getContext('2d');
    new Chart(ctx, {{
        type: 'bar',
        data: {{
            labels: {},
            datasets: [
                {{
                    label: '{} {}',
                    data: {:?},
                    backgroundColor: '{}CC',
                    borderColor: '{}',
                    borderWidth: 2
                }},
                {{
                    label: '{} {}',
                    data: {:?},
                    backgroundColor: '{}CC',
                    borderColor: '{}',
                    borderWidth: 2
                }}
            ]
        }},
        options: {{
            responsive: true,
            maintainAspectRatio: false,
            plugins: {{
                legend: {{
                    position: 'top',
                    labels: {{ font: {{ size: 14 }} }}
                }},
                tooltip: {{
                    callbacks: {{
                        label: function(context) {{
                            return context.dataset.label + ': ' + context.parsed.y.toFixed(2) + ' µs';
                        }}
                    }}
                }}
            }},
            scales: {{
                y: {{
                    beginAtZero: true,
                    title: {{ display: true, text: 'Time (µs)', font: {{ size: 14 }} }}
                }},
                x: {{
                    ticks: {{ font: {{ size: 11 }} }}
                }}
            }}
        }}
    }});
}})();
"#,
                    serde_json::to_string(&labels).unwrap_or_default(),
                    jit_config.emoji, jit_config.label,
                    jit_values,
                    jit_config.color, jit_config.color,
                    serde_config.emoji, serde_config.label,
                    serde_values,
                    serde_config.color, serde_config.color,
                )))
            }
        }
    }
}

fn legend_section() -> Markup {
    let targets = [
        ("facet_format_jit", "Format-agnostic JIT (our work!)"),
        ("facet_json_cranelift", ""),
        ("facet_format_json", ""),
        ("facet_json", ""),
        ("serde_json", "The baseline to beat"),
    ];

    html! {
        div.legend {
            h3 { "The 5 Targets" }
            div {
                @for (target, desc) in &targets {
                    @let config = get_target_config(target);
                    span.legend-item style=(format!("border-color: {};", config.color)) {
                        span.emoji { (config.emoji) }
                        " "
                        strong { (config.label) }
                        @if !desc.is_empty() {
                            " - " (*desc)
                        }
                    }
                }
            }
        }
    }
}

fn benchmark_sections(data: &BenchmarkData) -> Markup {
    // Categorize benchmarks
    let micro = [
        "simple_struct",
        "single_nested_struct",
        "simple_with_options",
        "nested_struct",
    ];
    let realistic = ["twitter", "canada", "hashmaps", "nested_structs"];
    let array = [
        "floats",
        "integers",
        "booleans",
        "short_strings",
        "long_strings",
        "escaped_strings",
    ];

    let mut sorted_benchmarks: Vec<_> = data.divan.keys().cloned().collect();
    sorted_benchmarks.sort();

    let categorize = |name: &str| -> &'static str {
        if micro.contains(&name) {
            "micro"
        } else if realistic.contains(&name) {
            "realistic"
        } else if array.contains(&name) {
            "array"
        } else {
            "other"
        }
    };

    html! {
        // Micro benchmarks
        @let micro_benches: Vec<_> = sorted_benchmarks.iter()
            .filter(|b| categorize(b) == "micro")
            .collect();
        @if !micro_benches.is_empty() {
            div.section-header { h2 { "🔬 Micro Benchmarks (JIT Testing)" } }
            @for bench_name in &micro_benches {
                (benchmark_item(bench_name, data))
            }
        }

        // Realistic benchmarks
        @let realistic_benches: Vec<_> = sorted_benchmarks.iter()
            .filter(|b| categorize(b) == "realistic")
            .collect();
        @if !realistic_benches.is_empty() {
            div.section-header { h2 { "🌍 Realistic Benchmarks (Real-World Data)" } }
            @for bench_name in &realistic_benches {
                (benchmark_item(bench_name, data))
            }
        }

        // Array benchmarks
        @let array_benches: Vec<_> = sorted_benchmarks.iter()
            .filter(|b| categorize(b) == "array")
            .collect();
        @if !array_benches.is_empty() {
            div.section-header { h2 { "📊 Array Benchmarks (Vec<T>)" } }
            @for bench_name in &array_benches {
                (benchmark_item(bench_name, data))
            }
        }

        // Other benchmarks
        @let other_benches: Vec<_> = sorted_benchmarks.iter()
            .filter(|b| categorize(b) == "other")
            .collect();
        @if !other_benches.is_empty() {
            div.section-header { h2 { "📦 Other Benchmarks" } }
            @for bench_name in &other_benches {
                (benchmark_item(bench_name, data))
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

    // Prepare chart data
    let chart_labels: Vec<_> = sorted
        .iter()
        .map(|(t, _)| {
            let config = get_target_config(t);
            format!("{} {}", config.emoji, config.label)
        })
        .collect();
    let chart_values: Vec<_> = sorted
        .iter()
        .map(|(_, v)| **v / 1000.0) // Convert to microseconds
        .collect();
    let chart_colors: Vec<_> = sorted
        .iter()
        .map(|(t, _)| get_target_config(t).color.to_string())
        .collect();

    html! {
        div.benchmark-item id=(format!("bench-{}", bench_id)) {
            h3 {
                (bench_name.replace('_', " "))
                " - "
                (operation.to_string())
            }

            div.table-chart-container {
                div.table-wrapper {
                    table id=(format!("table-{}", bench_id)) {
                        thead {
                            tr {
                                th { "Target" }
                                th { "Median Time" }
                                th { "Instructions" }
                                th { "vs serde 🎯" }
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

                                @let vs_serde_class = match vs_serde {
                                    Some(r) if r <= 1.0 => "speedup",
                                    Some(r) if r <= 2.0 => "neutral",
                                    Some(_) => "slowdown",
                                    None => "",
                                };

                                tr class=(row_class) data-target=(*target)
                                   onmouseenter=(format!("highlightChart('{}', '{}')", bench_id, target))
                                   onmouseleave=(format!("unhighlightChart('{}')", bench_id)) {
                                    td {
                                        span.emoji { (config.emoji) }
                                        " "
                                        (config.label)
                                    }
                                    td.metric { (format_time(**time_ns)) }
                                    td.metric {
                                        @if let Some(i) = instructions {
                                            (format_instructions(*i))
                                        } @else {
                                            "-"
                                        }
                                    }
                                    td class=(vs_serde_class) {
                                        @if let Some(r) = vs_serde {
                                            (format!("{:.2}x", r))
                                        } @else {
                                            "-"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div.chart-wrapper {
                    canvas id=(format!("chart-{}", bench_id)) {}
                }
            }

            // Chart initialization script
            script {
                (PreEscaped(format!(r#"
(function() {{
    const ctx = document.getElementById('chart-{}').getContext('2d');
    const chartData = {{
        labels: {},
        datasets: [{{
            label: 'Time (µs)',
            data: {:?},
            backgroundColor: {},
            borderColor: {},
            borderWidth: 2
        }}]
    }};
    const config = {{
        type: 'bar',
        data: chartData,
        options: {{
            responsive: true,
            maintainAspectRatio: true,
            indexAxis: 'y',
            plugins: {{
                legend: {{ display: false }},
                tooltip: {{
                    callbacks: {{
                        label: function(context) {{
                            return context.parsed.x.toFixed(3) + ' µs';
                        }}
                    }}
                }}
            }},
            scales: {{
                x: {{
                    beginAtZero: true,
                    title: {{ display: true, text: 'Time (microseconds)' }}
                }}
            }}
        }}
    }};
    window.charts = window.charts || {{}};
    window.charts['{}'] = new Chart(ctx, config);
}})();
"#,
                    bench_id,
                    serde_json::to_string(&chart_labels).unwrap_or_default(),
                    chart_values,
                    serde_json::to_string(&chart_colors.iter().map(|c| format!("{}CC", c)).collect::<Vec<_>>()).unwrap_or_default(),
                    serde_json::to_string(&chart_colors).unwrap_or_default(),
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
window.chartOriginalConfigs = {};

function highlightChart(benchId, targetName) {
    const chart = window.charts[benchId];
    if (!chart) return;

    if (!window.chartOriginalConfigs[benchId]) {
        window.chartOriginalConfigs[benchId] = {
            borderWidth: chart.data.datasets[0].borderWidth,
            backgroundColor: [...chart.data.datasets[0].backgroundColor],
            borderColor: [...chart.data.datasets[0].borderColor]
        };
    }

    const table = document.getElementById('table-' + benchId);
    const rows = table.querySelectorAll('tbody tr');
    let targetIndex = -1;

    rows.forEach((row, idx) => {
        if (row.getAttribute('data-target') === targetName) {
            targetIndex = idx;
            row.classList.remove('dimmed');
        } else {
            row.classList.add('dimmed');
        }
    });

    if (targetIndex === -1) return;

    const newBg = chart.data.datasets[0].backgroundColor.map((color, idx) => {
        return idx === targetIndex ? color : color.replace('CC', '30');
    });
    const newWidth = Array(chart.data.datasets[0].borderColor.length).fill(2);
    newWidth[targetIndex] = 4;

    chart.data.datasets[0].backgroundColor = newBg;
    chart.data.datasets[0].borderWidth = newWidth;
    chart.update('none');
}

function unhighlightChart(benchId) {
    const chart = window.charts[benchId];
    const table = document.getElementById('table-' + benchId);

    if (table) {
        const rows = table.querySelectorAll('tbody tr');
        rows.forEach(row => row.classList.remove('dimmed'));
    }

    if (!chart || !window.chartOriginalConfigs[benchId]) return;

    const original = window.chartOriginalConfigs[benchId];
    chart.data.datasets[0].backgroundColor = original.backgroundColor;
    chart.data.datasets[0].borderColor = original.borderColor;
    chart.data.datasets[0].borderWidth = original.borderWidth;
    chart.update('none');
}

window.highlightChart = highlightChart;
window.unhighlightChart = unhighlightChart;
            "#))
        }
    }
}

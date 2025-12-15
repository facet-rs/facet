#!/usr/bin/env python3
"""
Parse divan and gungraun benchmark outputs and generate HTML report with tables and graphs.
Each benchmark gets its own table and interactive chart.
"""

import re
import sys
import json
from pathlib import Path
from datetime import datetime
from typing import Dict, List, Optional
import subprocess

# Target configuration
TARGETS = {
    'facet_format_jit': {
        'emoji': '⚡',
        'name': 'facet_format_jit',
        'label': 'Format JIT',
        'color': '#FFD700',  # Yellow/Gold - our star!
    },
    'facet_format_json': {
        'emoji': '📦',
        'name': 'facet_format_json',
        'label': 'Format Interp',
        'color': '#FF6B6B',  # Red
    },
    'facet_json': {
        'emoji': '🔧',
        'name': 'facet_json',
        'label': 'JSON Interp',
        'color': '#4ECDC4',  # Teal
    },
    'facet_json_cranelift': {
        'emoji': '🚀',
        'name': 'facet_json_cranelift',
        'label': 'JSON JIT',
        'color': '#95E1D3',  # Light teal
    },
    'serde_json': {
        'emoji': '🎯',
        'name': 'serde_json',
        'label': 'serde_json',
        'color': '#9B59B6',  # Purple - the baseline
    },
}

def parse_divan_output(text: str) -> Dict[str, Dict[str, Dict[str, float]]]:
    """
    Parse divan output into structured data.
    Returns: {benchmark_name: {operation: {target_name: median_ns}}}
    where operation is 'deserialize' or 'serialize'
    """
    results = {}
    current_benchmark = None

    lines = text.split('\n')
    for line in lines:
        # Match benchmark module names
        module_match = re.match(r'[├╰]─ (\w+)\s', line)
        if module_match:
            current_benchmark = module_match.group(1)
            results[current_benchmark] = {'deserialize': {}, 'serialize': {}}
            continue

        # Match result lines like "│  ├─ facet_format_jit_deserialize  1.046 µs  ..."
        if current_benchmark:
            result_match = re.match(r'│?\s*[├╰]─\s+([\w_]+)\s+([\d.]+)\s+(ns|µs|ms)', line)
            if result_match:
                target_full = result_match.group(1)
                value = float(result_match.group(2))
                unit = result_match.group(3)

                # Convert to nanoseconds
                if unit == 'µs':
                    value *= 1000
                elif unit == 'ms':
                    value *= 1_000_000

                # Determine operation (deserialize or serialize)
                operation = 'deserialize' if 'deserialize' in target_full else 'serialize'

                # Extract target name (remove _deserialize/_serialize suffix)
                target_base = target_full.replace('_deserialize', '').replace('_serialize', '')

                results[current_benchmark][operation][target_base] = value

    return results

def format_time(ns: float) -> str:
    """Format nanoseconds into readable string with proper precision"""
    if ns < 1000:
        return f"{ns:.1f} ns"
    elif ns < 1_000_000:
        return f"{ns/1000:.2f} µs"
    else:
        return f"{ns/1_000_000:.2f} ms"

def generate_benchmark_section(bench_name: str, operation: str, divan_data: Dict[str, float],
                              gungraun_data: Dict[str, Dict[str, int]], bench_id: str) -> str:
    """
    Generate HTML for one benchmark: table (with both time + instructions) + interactive chart.
    bench_id is used for unique element IDs.
    """
    if not divan_data:
        return ""

    # Find serde_json baseline and fastest
    serde_time = divan_data.get('serde_json')
    fastest_time = min(divan_data.values()) if divan_data.values() else 0

    # Sort: fastest first
    sorted_targets = sorted(divan_data.items(), key=lambda x: x[1])

    html = f"""
    <div class="benchmark-item" id="bench-{bench_id}">
        <h3>{bench_name.replace('_', ' ').title()} - {operation.title()}</h3>

        <div class="table-chart-container">
            <div class="table-wrapper">
                <table id="table-{bench_id}">
                    <thead>
                        <tr>
                            <th>Target</th>
                            <th>Median Time</th>
                            <th>Instructions</th>
                            <th>vs serde 🎯</th>
                        </tr>
                    </thead>
                    <tbody>
"""

    for target, time_ns in sorted_targets:
        if target not in TARGETS:
            continue

        config = TARGETS[target]

        # Look up instruction count for this target
        # Try to find matching gungraun benchmark
        gungraun_key = f"{bench_name}_{target}"
        instructions = gungraun_data.get(gungraun_key, {}).get('Instructions')
        instructions_str = f"{instructions:,}" if instructions else "-"

        # Calculate ratios
        vs_serde = time_ns / serde_time if serde_time and serde_time > 0 else 0

        # Determine row class
        row_class = ""
        if time_ns == fastest_time:
            row_class = "fastest"
        elif target == 'serde_json':
            row_class = "baseline"
        elif target == 'facet_format_jit':
            row_class = "jit-highlight"

        # Format speedup/slowdown vs serde
        vs_serde_str = f"{vs_serde:.2f}x" if vs_serde > 0 else "-"
        vs_serde_class = "speedup" if vs_serde <= 1.0 else ("neutral" if vs_serde <= 2.0 else "slowdown")

        html += f"""
                        <tr class="{row_class}" data-target="{target}"
                            onmouseenter="highlightChart('{bench_id}', '{target}')"
                            onmouseleave="unhighlightChart('{bench_id}')">
                            <td><span class="emoji">{config['emoji']}</span> {config['label']}</td>
                            <td class="metric">{format_time(time_ns)}</td>
                            <td class="metric">{instructions_str}</td>
                            <td class="{vs_serde_class}">{vs_serde_str}</td>
                        </tr>
"""

    html += """
                    </tbody>
                </table>
            </div>

            <div class="chart-wrapper">
                <canvas id="chart-""" + bench_id + """"></canvas>
            </div>
        </div>
    </div>

    <script>
    (function() {
        const ctx = document.getElementById('chart-""" + bench_id + """').getContext('2d');

        const chartData = {
            labels: [
"""

    # Generate chart labels and data
    for target, time_ns in sorted_targets:
        if target not in TARGETS:
            continue
        config = TARGETS[target]
        html += f"                '{config['emoji']} {config['label']}',\n"

    html += """            ],
            datasets: [{
                label: 'Time (µs)',
                data: [
"""

    for target, time_ns in sorted_targets:
        if target in TARGETS:
            html += f"                    {time_ns / 1000:.3f},\n"  # Convert to microseconds

    html += """                ],
                backgroundColor: [
"""

    for target, _ in sorted_targets:
        if target in TARGETS:
            config = TARGETS[target]
            html += f"                    '{config['color']}CC',\n"  # CC for alpha

    html += """                ],
                borderColor: [
"""

    for target, _ in sorted_targets:
        if target in TARGETS:
            config = TARGETS[target]
            html += f"                    '{config['color']}',\n"

    html += """                ],
                borderWidth: 2
            }]
        };

        const config = {
            type: 'bar',
            data: chartData,
            options: {
                responsive: true,
                maintainAspectRatio: true,
                indexAxis: 'y',  // Horizontal bars
                plugins: {
                    legend: {
                        display: false
                    },
                    tooltip: {
                        callbacks: {
                            label: function(context) {
                                return context.parsed.x.toFixed(3) + ' µs';
                            }
                        }
                    }
                },
                scales: {
                    x: {
                        beginAtZero: true,
                        title: {
                            display: true,
                            text: 'Time (microseconds)'
                        }
                    }
                }
            }
        };

        window.charts = window.charts || {};
        window.charts['""" + bench_id + """'] = new Chart(ctx, config);
    })();
    </script>
"""

    return html

def generate_html_report(divan_data: Dict, git_info: Dict) -> str:
    """Generate complete HTML report"""

    html_head = f"""<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Facet JIT Benchmark Report</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.0/dist/chart.umd.min.js"></script>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}

        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #f8f9fa;
            color: #212529;
            line-height: 1.6;
        }}

        .container {{
            max-width: 1800px;
            margin: 0 auto;
            padding: 20px;
        }}

        header {{
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 40px;
            border-radius: 12px;
            margin-bottom: 30px;
            box-shadow: 0 4px 6px rgba(0,0,0,0.1);
        }}

        h1 {{ font-size: 2.5em; margin-bottom: 10px; }}
        h2 {{
            color: #495057;
            margin: 40px 0 20px 0;
            padding-bottom: 10px;
            border-bottom: 3px solid #dee2e6;
        }}
        h3 {{
            color: #6c757d;
            margin: 25px 0 15px 0;
            font-size: 1.3em;
        }}

        .meta {{ opacity: 0.95; font-size: 0.95em; margin-top: 10px; }}
        .meta-item {{ display: inline-block; margin-right: 30px; }}

        .legend {{
            background: white;
            padding: 25px;
            border-radius: 12px;
            margin: 20px 0;
            box-shadow: 0 2px 8px rgba(0,0,0,0.08);
        }}
        .legend h3 {{ margin-top: 0; color: #495057; }}
        .legend-item {{
            display: inline-block;
            margin: 10px 20px 10px 0;
            padding: 8px 15px;
            background: #f8f9fa;
            border-radius: 6px;
            border-left: 4px solid;
        }}

        .benchmark-item {{
            background: white;
            padding: 30px;
            margin: 30px 0;
            border-radius: 12px;
            box-shadow: 0 2px 8px rgba(0,0,0,0.08);
        }}

        .table-chart-container {{
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: 30px;
            margin-top: 20px;
        }}

        @media (max-width: 1200px) {{
            .table-chart-container {{
                grid-template-columns: 1fr;
            }}
        }}

        table {{
            width: 100%;
            border-collapse: collapse;
        }}

        th {{
            background: linear-gradient(to bottom, #4CAF50, #45a049);
            color: white;
            padding: 12px;
            text-align: left;
            font-weight: 600;
            position: sticky;
            top: 0;
        }}

        td {{
            padding: 12px;
            border-bottom: 1px solid #e9ecef;
        }}

        tr:hover {{
            background: #e3f2fd !important;
            cursor: pointer;
            transition: background 0.15s;
        }}

        .fastest {{
            background: #c8e6c9;
            border-left: 4px solid #2e7d32;
        }}

        .jit-highlight {{
            background: #fff9c4;
            border-left: 4px solid #f57f17;
            font-weight: 600;
        }}

        .baseline {{
            background: #e1bee7;
            border-left: 4px solid #7b1fa2;
            font-weight: 600;
        }}

        .emoji {{
            font-size: 1.2em;
            margin-right: 5px;
        }}

        .metric {{
            font-family: 'SF Mono', Monaco, 'Courier New', monospace;
            font-size: 0.95em;
        }}

        .speedup {{ color: #2e7d32; font-weight: 600; }}
        .neutral {{ color: #f57f17; }}
        .slowdown {{ color: #c62828; }}

        .chart-wrapper {{
            position: relative;
            height: 300px;
        }}

        canvas {{
            max-height: 300px;
        }}

        .section-header {{
            background: #e3f2fd;
            padding: 15px 25px;
            border-radius: 8px;
            margin: 30px 0 15px 0;
            border-left: 5px solid #1976d2;
        }}

        footer {{
            text-align: center;
            margin-top: 60px;
            padding: 30px;
            color: #6c757d;
            font-size: 0.9em;
            border-top: 2px solid #dee2e6;
        }}
    </style>
</head>
<body>
    <div class="container">
        <header>
            <h1>🚀 Facet JIT Benchmark Report</h1>
            <div class="meta">
                <span class="meta-item"><strong>Generated:</strong> {datetime.now().strftime("%Y-%m-%d %H:%M:%S")}</span>
                <span class="meta-item"><strong>Commit:</strong> {git_info.get('commit', 'unknown')}</span>
                <span class="meta-item"><strong>Branch:</strong> {git_info.get('branch', 'unknown')}</span>
            </div>
        </header>

        <div class="legend">
            <h3>The 5 Targets</h3>
            <div>
"""

    for target_key in ['facet_format_jit', 'facet_json_cranelift', 'facet_format_json', 'facet_json', 'serde_json']:
        config = TARGETS[target_key]
        html_head += f'                <span class="legend-item" style="border-color: {config["color"]};">'
        html_head += f'<span class="emoji">{config["emoji"]}</span> <strong>{config["label"]}</strong>'
        if target_key == 'facet_format_jit':
            html_head += ' - Format-agnostic JIT (our work!)'
        elif target_key == 'serde_json':
            html_head += ' - The baseline to beat'
        html_head += '</span>\n'

    html_head += """
            </div>
        </div>
"""

    return html_head

def generate_sections_html(divan_data: Dict, gungraun_data: Dict) -> str:
    """Generate sections for deserialize and serialize benchmarks"""
    html = ""

    # Separate benchmarks by type
    micro_benches = [b for b in divan_data.keys() if b in ['simple_struct', 'single_nested_struct', 'simple_with_options', 'nested_struct']]
    realistic_benches = [b for b in divan_data.keys() if b in ['twitter', 'canada', 'hashmaps', 'nested_structs']]
    array_benches = [b for b in divan_data.keys() if b in ['floats', 'integers', 'booleans', 'short_strings', 'long_strings', 'escaped_strings']]
    other_benches = [b for b in divan_data.keys() if b not in micro_benches + realistic_benches + array_benches]

    for category_name, benchmarks in [
        ("🔬 Micro Benchmarks (JIT Testing)", micro_benches),
        ("🌍 Realistic Benchmarks (Real-World Data)", realistic_benches),
        ("📊 Array Benchmarks (Vec&lt;T&gt;)", array_benches),
        ("📦 Other Benchmarks", other_benches)
    ]:
        if not benchmarks:
            continue

        html += f'<div class="section-header"><h2>{category_name}</h2></div>\n'

        for bench_name in benchmarks:
            bench_data = divan_data[bench_name]

            # Deserialize section
            if bench_data.get('deserialize'):
                bench_id = f"{bench_name}_deser"
                html += generate_benchmark_section(bench_name, 'deserialize',
                                                   bench_data['deserialize'],
                                                   gungraun_data, bench_id)

            # Serialize section
            if bench_data.get('serialize'):
                bench_id = f"{bench_name}_ser"
                html += generate_benchmark_section(bench_name, 'serialize',
                                                   bench_data['serialize'],
                                                   gungraun_data, bench_id)

    return html

def generate_interactive_js() -> str:
    """Generate JavaScript for interactive table/chart highlighting"""
    return """
    <script>
        // Store original chart configurations
        window.chartOriginalConfigs = {};

        function highlightChart(benchId, targetName) {
            const chart = window.charts[benchId];
            if (!chart) return;

            // Store original if not already stored
            if (!window.chartOriginalConfigs[benchId]) {
                window.chartOriginalConfigs[benchId] = {
                    borderWidth: chart.data.datasets[0].borderWidth,
                    backgroundColor: [...chart.data.datasets[0].backgroundColor],
                    borderColor: [...chart.data.datasets[0].borderColor]
                };
            }

            // Find index of target
            const table = document.getElementById('table-' + benchId);
            const rows = table.querySelectorAll('tbody tr');
            let targetIndex = -1;

            rows.forEach((row, idx) => {
                if (row.getAttribute('data-target') === targetName) {
                    targetIndex = idx;
                }
            });

            if (targetIndex === -1) return;

            // Dim all bars except the highlighted one
            const original = window.chartOriginalConfigs[benchId];
            const newBg = chart.data.datasets[0].backgroundColor.map((color, idx) => {
                return idx === targetIndex ? color : color.replace('CC', '40');  // More transparent
            });
            const newBorder = chart.data.datasets[0].borderColor.map((color, idx) => {
                return color;
            });
            const newWidth = Array(chart.data.datasets[0].borderColor.length).fill(2);
            newWidth[targetIndex] = 4;  // Thicker border for highlighted

            chart.data.datasets[0].backgroundColor = newBg;
            chart.data.datasets[0].borderWidth = newWidth;
            chart.update('none');  // No animation
        }

        function unhighlightChart(benchId) {
            const chart = window.charts[benchId];
            if (!chart || !window.chartOriginalConfigs[benchId]) return;

            const original = window.chartOriginalConfigs[benchId];
            chart.data.datasets[0].backgroundColor = original.backgroundColor;
            chart.data.datasets[0].borderColor = original.borderColor;
            chart.data.datasets[0].borderWidth = original.borderWidth;
            chart.update('none');
        }

        window.highlightChart = highlightChart;
        window.unhighlightChart = unhighlightChart;
    </script>
"""

def parse_gungraun_output(text: str) -> Dict[str, Dict[str, int]]:
    """
    Parse gungraun output into structured data.
    Returns: {benchmark_name: {metric: value}}
    """
    results = {}
    current_bench = None

    lines = text.split('\n')
    for line in lines:
        # Match benchmark names like "gungraun_jit::jit_benchmarks::simple_struct_facet_format_jit"
        name_match = re.match(r'gungraun_jit::[\w_]+::([\w_]+)', line)
        if name_match:
            current_bench = name_match.group(1)
            results[current_bench] = {}
            continue

        # Match metrics like "  Instructions: 6549|N/A"
        if current_bench:
            metric_match = re.match(r'\s+([\w\s]+):\s+([\d,]+)', line)
            if metric_match:
                metric = metric_match.group(1).strip()
                value_str = metric_match.group(2).replace(',', '')
                # Handle the "12345|67890" format (current|baseline)
                if '|' in value_str:
                    value_str = value_str.split('|')[0]
                try:
                    value = int(value_str)
                    results[current_bench][metric] = value
                except ValueError:
                    continue

    return results

def main():
    if len(sys.argv) < 3:
        print("Usage: parse_bench.py <divan_output.txt> <gungraun_output.txt> [output.html]")
        sys.exit(1)

    divan_file = Path(sys.argv[1])
    gungraun_file = Path(sys.argv[2])
    output_file = Path(sys.argv[3]) if len(sys.argv) > 3 else Path("bench-report.html")

    # Parse benchmark outputs
    divan_text = divan_file.read_text() if divan_file.exists() else ""
    divan_data = parse_divan_output(divan_text)

    gungraun_text = gungraun_file.read_text() if gungraun_file.exists() else ""
    gungraun_data = parse_gungraun_output(gungraun_text)

    # Get git info
    try:
        git_info = {
            'commit': subprocess.run(['git', 'rev-parse', '--short', 'HEAD'],
                                    capture_output=True, text=True, check=True).stdout.strip(),
            'branch': subprocess.run(['git', 'branch', '--show-current'],
                                    capture_output=True, text=True, check=True).stdout.strip(),
        }
    except:
        git_info = {'commit': 'unknown', 'branch': 'unknown'}

    # Generate HTML
    html = generate_html_report(divan_data, git_info)
    html += generate_sections_html(divan_data, gungraun_data)
    html += generate_interactive_js()
    html += """
        <footer>
            <p><strong>Generated by</strong> scripts/parse_bench.py</p>
            <p>Benchmark tools: divan (wall-clock) + gungraun (instruction counts)</p>
        </footer>
    </div>
</body>
</html>
"""

    # Write output
    output_file.write_text(html)

    # Count benchmarks
    deser_count = sum(1 for b in divan_data.values() if b.get('deserialize'))
    ser_count = sum(1 for b in divan_data.values() if b.get('serialize'))

    print(f"✅ Report generated: {output_file}")
    print(f"   Benchmarks: {deser_count} deserialize, {ser_count} serialize")

if __name__ == '__main__':
    main()

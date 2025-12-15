#!/usr/bin/env python3
"""
Parse divan and gungraun benchmark outputs and generate HTML report with tables and graphs.
"""

import re
import sys
import json
from pathlib import Path
from datetime import datetime
from typing import Dict, List, Tuple
import subprocess

def parse_divan_output(text: str) -> Dict[str, Dict[str, float]]:
    """
    Parse divan output into structured data.
    Returns: {benchmark_name: {target_name: median_ns}}
    """
    results = {}
    current_benchmark = None

    lines = text.split('\n')
    for line in lines:
        # Match benchmark module names like "├─ simple_struct" or "╰─ twitter"
        module_match = re.match(r'[├╰]─ (\w+)\s', line)
        if module_match:
            current_benchmark = module_match.group(1)
            results[current_benchmark] = {}
            continue

        # Match result lines like "│  ├─ facet_format_jit_deserialize  1.046 µs  ..."
        if current_benchmark:
            result_match = re.match(r'│?\s*[├╰]─\s+(\w+)\s+([\d.]+)\s+(ns|µs|ms)', line)
            if result_match:
                target = result_match.group(1)
                value = float(result_match.group(2))
                unit = result_match.group(3)

                # Convert to nanoseconds
                if unit == 'µs':
                    value *= 1000
                elif unit == 'ms':
                    value *= 1_000_000

                results[current_benchmark][target] = value

    return results

def parse_gungraun_output(text: str) -> Dict[str, Dict[str, int]]:
    """
    Parse gungraun output into structured data.
    Returns: {benchmark_name: {metric: value}}
    """
    results = {}
    current_bench = None

    lines = text.split('\n')
    for line in lines:
        # Match benchmark names
        name_match = re.match(r'gungraun_jit::[\w_]+::([\w_]+)', line)
        if name_match:
            current_bench = name_match.group(1)
            results[current_bench] = {}
            continue

        # Match metrics like "  Instructions:  6549|N/A"
        if current_bench:
            metric_match = re.match(r'\s+(\w+(?:\s+\w+)*):\s+([\d,]+)', line)
            if metric_match:
                metric = metric_match.group(1).strip()
                value = int(metric_match.group(2).replace(',', ''))
                results[current_bench][metric] = value

    return results

def format_time(ns: float) -> str:
    """Format nanoseconds into readable string"""
    if ns < 1000:
        return f"{ns:.1f} ns"
    elif ns < 1_000_000:
        return f"{ns/1000:.2f} µs"
    else:
        return f"{ns/1_000_000:.2f} ms"

def format_number(n: int) -> str:
    """Format large numbers with commas"""
    return f"{n:,}"

def generate_html_report(divan_data: Dict, gungraun_data: Dict, git_info: Dict) -> str:
    """Generate HTML report with tables and graphs"""

    # Group benchmarks by type
    micro_benchmarks = ['simple_struct', 'single_nested_struct', 'simple_with_options']
    realistic_benchmarks = ['twitter', 'canada', 'hashmaps', 'nested_structs']

    html = f"""
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Facet JIT Benchmark Report</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.0/dist/chart.umd.min.js"></script>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
            max-width: 1600px;
            margin: 0 auto;
            padding: 40px 20px;
            background: #f8f9fa;
            color: #212529;
        }}
        header {{
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 40px;
            border-radius: 12px;
            margin-bottom: 30px;
            box-shadow: 0 4px 6px rgba(0,0,0,0.1);
        }}
        h1 {{ margin: 0 0 10px 0; font-size: 2.5em; }}
        .meta {{ opacity: 0.9; font-size: 0.95em; }}
        .meta-item {{ margin: 5px 0; }}

        .legend {{
            background: #e7f5ff;
            border-left: 4px solid #1971c2;
            padding: 20px;
            margin: 30px 0;
            border-radius: 8px;
        }}
        .legend h2 {{ margin-top: 0; color: #1864ab; }}
        .legend-item {{
            margin: 8px 0;
            padding-left: 20px;
        }}

        .section {{
            background: white;
            padding: 30px;
            margin: 30px 0;
            border-radius: 12px;
            box-shadow: 0 2px 8px rgba(0,0,0,0.08);
        }}
        h2 {{
            color: #495057;
            border-bottom: 3px solid #dee2e6;
            padding-bottom: 12px;
            margin-top: 0;
        }}
        h3 {{
            color: #6c757d;
            margin: 25px 0 15px 0;
        }}

        table {{
            width: 100%;
            border-collapse: collapse;
            margin: 20px 0;
            font-size: 0.95em;
        }}
        th {{
            background: linear-gradient(to bottom, #4CAF50, #45a049);
            color: white;
            padding: 14px 12px;
            text-align: left;
            font-weight: 600;
            position: sticky;
            top: 0;
        }}
        td {{
            padding: 12px;
            border-bottom: 1px solid #e9ecef;
        }}
        tr:nth-child(even) {{
            background: #f8f9fa;
        }}
        tr:hover {{
            background: #e3f2fd;
            transition: background 0.2s;
        }}

        .fastest {{
            background: #c8e6c9 !important;
            font-weight: 700;
            color: #2e7d32;
        }}
        .jit-highlight {{
            background: #fff3cd !important;
            font-weight: 600;
        }}
        .baseline {{
            background: #f0f4f8;
            font-style: italic;
        }}

        .metric {{
            font-family: 'SF Mono', Monaco, 'Courier New', monospace;
            color: #1976d2;
            font-size: 0.9em;
        }}
        .speedup {{
            color: #2e7d32;
            font-weight: 600;
        }}
        .slowdown {{
            color: #d32f2f;
        }}

        .chart-container {{
            position: relative;
            height: 400px;
            margin: 30px 0;
        }}

        .note {{
            background: #fff9db;
            border-left: 4px solid #ffd93d;
            padding: 15px;
            margin: 20px 0;
            border-radius: 4px;
            font-size: 0.9em;
        }}

        footer {{
            text-align: center;
            margin-top: 60px;
            padding-top: 20px;
            border-top: 2px solid #dee2e6;
            color: #6c757d;
            font-size: 0.9em;
        }}
    </style>
</head>
<body>
    <header>
        <h1>🚀 Facet JIT Benchmark Report</h1>
        <div class="meta">
            <div class="meta-item"><strong>Generated:</strong> {datetime.now().strftime("%Y-%m-%d %H:%M:%S")}</div>
            <div class="meta-item"><strong>Git Commit:</strong> {git_info['commit']}</div>
            <div class="meta-item"><strong>Branch:</strong> {git_info['branch']}</div>
        </div>
    </header>

    <div class="legend">
        <h2>The 5 Targets Compared</h2>
        <div class="legend-item">1. <strong>facet_json</strong> - Legacy interpreter-based JSON deserializer</div>
        <div class="legend-item">2. <strong>facet_json_cranelift</strong> - JSON-specific JIT compiler (specialized, fast)</div>
        <div class="legend-item">3. <strong>facet_format_json</strong> - Format-agnostic event-based interpreter</div>
        <div class="legend-item">4. <strong>facet_format_jit</strong> - Format-agnostic JIT compiler ⭐ <em>(our work!)</em></div>
        <div class="legend-item">5. <strong>serde_json</strong> - Industry standard baseline 🎯 <em>(the one to beat)</em></div>
    </div>

    <div class="section">
        <h2>📊 Wall-Clock Performance (Median Times)</h2>
"""

    # Generate tables for each benchmark category
    for category, benchmarks in [("Micro Benchmarks", micro_benchmarks), ("Realistic Benchmarks", realistic_benchmarks)]:
        html += f"<h3>{category}</h3>\n"

        for bench in benchmarks:
            if bench not in divan_data:
                continue

            data = divan_data[bench]
            if not data:
                continue

            # Find fastest
            fastest = min(data.values())

            html += f"<h4>{bench.replace('_', ' ').title()}</h4>\n"
            html += "<table>\n<thead>\n<tr>\n"
            html += "<th>Target</th><th>Median Time</th><th>vs Fastest</th><th>vs serde_json</th>\n"
            html += "</tr>\n</thead>\n<tbody>\n"

            # Sort by speed (fastest first)
            sorted_targets = sorted(data.items(), key=lambda x: x[1])

            serde_time = data.get('serde_json_deserialize', data.get('serde_json_serialize', None))

            for target, time_ns in sorted_targets:
                row_class = ""
                if time_ns == fastest:
                    row_class = "fastest"
                elif 'format_jit' in target:
                    row_class = "jit-highlight"
                elif 'serde' in target:
                    row_class = "baseline"

                vs_fastest = f"{time_ns / fastest:.2f}x" if fastest > 0 else "-"
                vs_serde = f"{time_ns / serde_time:.2f}x" if serde_time and serde_time > 0 else "-"

                speedup_class = "speedup" if time_ns <= fastest * 1.1 else ("slowdown" if time_ns > fastest * 2 else "")

                html += f'<tr class="{row_class}">\n'
                html += f'  <td>{target.replace("_", " ")}</td>\n'
                html += f'  <td class="metric">{format_time(time_ns)}</td>\n'
                html += f'  <td class="{speedup_class}">{vs_fastest}</td>\n'
                html += f'  <td class="{speedup_class}">{vs_serde}</td>\n'
                html += '</tr>\n'

            html += "</tbody>\n</table>\n"

    html += "</div>\n"

    # Gungraun section
    html += """
    <div class="section">
        <h2>🔬 Instruction Counts (Gungraun - Deterministic)</h2>
        <div class="note">
            These measurements are deterministic and reproducible across different machines.
            Lower instruction counts = better performance.
        </div>
"""

    for bench_name, metrics in gungraun_data.items():
        if not metrics:
            continue

        html += f"<h3>{bench_name.replace('_', ' ').title()}</h3>\n"

        # Show key metrics
        if 'Instructions' in metrics:
            html += f"<p><strong>Instructions:</strong> <span class='metric'>{format_number(metrics['Instructions'])}</span></p>\n"
        if 'Estimated Cycles' in metrics:
            html += f"<p><strong>Estimated Cycles:</strong> <span class='metric'>{format_number(metrics['Estimated Cycles'])}</span></p>\n"

    html += "</div>\n"

    # Chart section
    html += """
    <div class="section">
        <h2>📈 Performance Comparison Charts</h2>
        <div class="chart-container">
            <canvas id="microBenchChart"></canvas>
        </div>
        <div class="chart-container">
            <canvas id="realisticBenchChart"></canvas>
        </div>
    </div>

    <script>
        // Chart.js configuration
        const chartColors = {
            facet_json: '#FF6384',
            facet_json_cranelift: '#36A2EB',
            facet_format_json: '#FFCE56',
            facet_format_jit: '#4BC0C0',
            serde_json: '#9966FF'
        };

        // Micro benchmarks chart
        const microCtx = document.getElementById('microBenchChart').getContext('2d');
        new Chart(microCtx, {
            type: 'bar',
            data: {
                labels: """ + json.dumps([b.replace('_', ' ').title() for b in micro_benchmarks if b in divan_data]) + """,
                datasets: [
"""

    # Generate dataset for each target
    targets = ['facet_format_jit_deserialize', 'facet_format_json_deserialize', 'facet_json_deserialize',
               'facet_json_cranelift_deserialize', 'serde_json_deserialize']
    target_labels = ['Format JIT ⭐', 'Format Interp', 'JSON Interp', 'JSON JIT', 'serde_json 🎯']

    for target, label in zip(targets, target_labels):
        data_points = []
        for bench in micro_benchmarks:
            if bench in divan_data and target in divan_data[bench]:
                # Convert to microseconds for chart
                data_points.append(divan_data[bench][target] / 1000)
            else:
                data_points.append(None)

        color = chartColors.get(target.split('_')[0] + '_' + target.split('_')[1], '#999')
        html += f"""                    {{
                        label: '{label}',
                        data: {json.dumps(data_points)},
                        backgroundColor: '{color}88',
                        borderColor: '{color}',
                        borderWidth: 2
                    }},
"""

    html += """                ]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                plugins: {
                    title: {
                        display: true,
                        text: 'Micro Benchmarks (Lower is Better)',
                        font: { size: 18 }
                    },
                    legend: {
                        display: true,
                        position: 'bottom'
                    }
                },
                scales: {
                    y: {
                        beginAtZero: true,
                        title: {
                            display: true,
                            text: 'Time (microseconds)'
                        }
                    }
                }
            }
        });
    </script>

    <footer>
        <p>Generated by scripts/parse-bench.py</p>
        <p>Benchmark data: divan (wall-clock) + gungraun (instruction counts)</p>
    </footer>
</body>
</html>
"""

    return html

def main():
    if len(sys.argv) < 3:
        print("Usage: parse-bench.py <divan_output.txt> <gungraun_output.txt> [output.html]")
        sys.exit(1)

    divan_file = Path(sys.argv[1])
    gungraun_file = Path(sys.argv[2])
    output_file = Path(sys.argv[3]) if len(sys.argv) > 3 else Path("bench-report.html")

    # Parse benchmark outputs
    divan_text = divan_file.read_text() if divan_file.exists() else ""
    gungraun_text = gungraun_file.read_text() if gungraun_file.exists() else ""

    divan_data = parse_divan_output(divan_text)
    gungraun_data = parse_gungraun_output(gungraun_text)

    # Get git info
    git_info = {
        'commit': subprocess.run(['git', 'rev-parse', '--short', 'HEAD'],
                                capture_output=True, text=True).stdout.strip(),
        'branch': subprocess.run(['git', 'branch', '--show-current'],
                                capture_output=True, text=True).stdout.strip(),
    }

    # Generate HTML
    html = generate_html_report(divan_data, gungraun_data, git_info)

    # Write output
    output_file.write_text(html)
    print(f"✅ Report generated: {output_file}")
    print(f"   Benchmarks parsed: {len(divan_data)} from divan, {len(gungraun_data)} from gungraun")

if __name__ == '__main__':
    main()

#!/usr/bin/env -S uv run --script --quiet
# /// script
# requires-python = ">=3.10"
# dependencies = ["matplotlib"]
# ///
"""Run `cargo bench -q rpc::codec` and parse divan's tree output to CSV/plot.

Examples:
    scripts/bench_codec.py                          # CSV to stdout
    scripts/bench_codec.py --csv out.csv
    scripts/bench_codec.py --plot out.png
    scripts/bench_codec.py --input captured.txt    # skip cargo, parse a file
    scripts/bench_codec.py --save-output raw.txt   # also keep raw divan text

CSV-only runs don't need matplotlib; the shebang installs it once via `uv run`.
"""

from __future__ import annotations

import argparse
import csv
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

UNIT_NS = {
    "ns": 1.0,
    "µs": 1_000.0,
    "us": 1_000.0,
    "ms": 1_000_000.0,
    "s": 1_000_000_000.0,
}

TIME_RE = re.compile(r"^\s*([\d.]+)\s*(ns|µs|us|ms|s)\s*$")
# Used to peel a trailing time value off the label+first-column block.
TRAILING_TIME_RE = re.compile(r"\s+([\d.]+\s*(?:ns|µs|us|ms|s))\s*$")


def parse_time(s: str):
    m = TIME_RE.match(s)
    if not m:
        return None
    return float(m.group(1)) * UNIT_NS[m.group(2)]


def split_label_and_fastest(block: str) -> tuple[str, str]:
    """In the leftmost column, the label and the `fastest` time share the
    same field separated only by whitespace. Peel a trailing time value off
    if present; otherwise the whole block is just the label (interior row).
    """
    m = TRAILING_TIME_RE.search(block)
    if not m:
        return block, ""
    return block[: m.start()], m.group(1)


def parse_int(s: str):
    s = s.strip().replace(",", "")
    if not s:
        return None
    try:
        return int(s)
    except ValueError:
        return None


def strip_tree(label: str):
    """Return (depth, name) for a divan tree label.

    Depth is 1 for top-level header rows ('rpc'), 2 for first nested branch,
    and so on. Indent is 3 columns per level (the box-drawing branch char
    sits at column (depth-2)*3).
    """
    branch_pos = None
    for j, ch in enumerate(label):
        if ch in "├╰":
            branch_pos = j
            break
        if ch not in (" ", "│"):
            return 1, label.strip()
    if branch_pos is None:
        return None
    depth = branch_pos // 3 + 2
    return depth, label[branch_pos + 2 :].strip()


def parse_output(text: str):
    rows = []
    stack: list[tuple[int, str]] = []
    in_table = False
    for ln in text.splitlines():
        if not ln.strip():
            continue
        if "fastest" in ln and "slowest" in ln and "median" in ln:
            in_table = True
            stack = []
            continue
        if not in_table:
            continue
        parts = ln.rsplit("│", 5)
        if len(parts) != 6:
            in_table = False
            continue
        label_block = parts[0]
        label, fastest_str = split_label_and_fastest(label_block)
        cols = [fastest_str] + [p.strip() for p in parts[1:]]
        sn = strip_tree(label)
        if sn is None:
            continue
        depth, name = sn
        while stack and stack[-1][0] >= depth:
            stack.pop()
        stack.append((depth, name))
        fastest = parse_time(cols[0])
        if fastest is None:
            continue
        components = [n for _, n in stack]
        rows.append(
            {
                "path": "::".join(components),
                "components": components,
                "fastest_ns": fastest,
                "slowest_ns": parse_time(cols[1]),
                "median_ns": parse_time(cols[2]),
                "mean_ns": parse_time(cols[3]),
                "samples": parse_int(cols[4]),
                "iters": parse_int(cols[5]),
            }
        )
    return rows


def run_cargo(filter_pat: str) -> str:
    # divan prints the table to stderr; merge it into stdout.
    cmd = ["cargo", "bench", "-q", filter_pat]
    sys.stderr.write(f"$ {' '.join(cmd)}\n")
    proc = subprocess.run(cmd, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    if proc.returncode != 0:
        sys.stderr.write(proc.stdout)
        sys.exit(proc.returncode)
    return proc.stdout


CSV_FIELDS = [
    "path",
    "fastest_ns",
    "slowest_ns",
    "median_ns",
    "mean_ns",
    "samples",
    "iters",
]


def write_csv(rows, out) -> None:
    w = csv.writer(out)
    w.writerow(CSV_FIELDS)
    for r in rows:
        w.writerow([r[f] for f in CSV_FIELDS])


def plot(rows, path: Path) -> None:
    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError:
        sys.stderr.write("matplotlib not available — install with `pip install matplotlib`\n")
        sys.exit(2)
    from collections import defaultdict

    # Components after the harness root: codec / <shape> / <variant> [/ <arg>]
    groups: dict[str, list[tuple[str, str | None, float]]] = defaultdict(list)
    for r in rows:
        comps = r["components"]
        # Trim leading 'rpc' if divan included it as a separate top-level row.
        if comps and comps[0] == "rpc":
            comps = comps[1:]
        if len(comps) < 3 or comps[0] != "codec":
            continue
        shape = comps[1]
        variant = comps[2]
        arg = comps[3] if len(comps) >= 4 else None
        groups[shape].append((variant, arg, r["median_ns"]))

    if not groups:
        sys.stderr.write("no plot-eligible rows\n")
        sys.exit(1)

    n = len(groups)
    cols = 2
    rows_n = (n + cols - 1) // cols
    fig, axes = plt.subplots(rows_n, cols, figsize=(11, 4 * rows_n), squeeze=False)
    flat = list(axes.flat)

    for ax, (shape, entries) in zip(flat, sorted(groups.items())):
        per_variant: dict[str, list[tuple[str | None, float]]] = defaultdict(list)
        for variant, arg, median in entries:
            per_variant[variant].append((arg, median))

        any_args = any(a is not None for _, items in per_variant.items() for a, _ in items)
        if any_args:
            for variant, pairs in sorted(per_variant.items()):
                try:
                    pairs_sorted = sorted(pairs, key=lambda x: float(x[0]) if x[0] else 0)
                except ValueError:
                    pairs_sorted = sorted(pairs, key=lambda x: x[0] or "")
                xs = [p[0] for p in pairs_sorted]
                ys = [p[1] for p in pairs_sorted]
                ax.plot(xs, ys, marker="o", label=variant)
            ax.set_xlabel("arg")
        else:
            names = sorted(per_variant.keys())
            values = [per_variant[v][0][1] for v in names]
            ax.bar(names, values)
            ax.tick_params(axis="x", labelrotation=20)

        ax.set_title(shape)
        ax.set_ylabel("median (ns)")
        ax.set_yscale("log")
        ax.grid(True, which="both", alpha=0.3)
        if any_args:
            ax.legend(fontsize=8)

    for ax in flat[len(groups) :]:
        ax.axis("off")

    fig.suptitle("rpc::codec bench (median)")
    fig.tight_layout()
    fig.savefig(path, dpi=120)
    sys.stderr.write(f"wrote {path}\n")


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--filter", default="rpc::codec", help="cargo bench filter (default: rpc::codec)")
    p.add_argument("--input", type=Path, help="read divan output from file instead of running cargo")
    p.add_argument("--save-output", type=Path, help="also write raw divan output here")
    p.add_argument("--csv", type=Path, help="write CSV here (default: stdout unless --plot is set)")
    p.add_argument("--plot", type=Path, help="write a matplotlib figure here (PNG/PDF/SVG)")
    args = p.parse_args()

    text = args.input.read_text() if args.input else run_cargo(args.filter)
    if args.save_output:
        args.save_output.write_text(text)

    rows = parse_output(text)
    if not rows:
        sys.stderr.write("no bench rows parsed — divan output may have changed\n")
        sys.exit(1)

    sys.stderr.write(f"parsed {len(rows)} rows\n")

    if args.csv:
        with args.csv.open("w", newline="") as f:
            write_csv(rows, f)
        sys.stderr.write(f"wrote {args.csv}\n")
    if args.plot:
        plot(rows, args.plot)
    if not args.csv and not args.plot:
        write_csv(rows, sys.stdout)


if __name__ == "__main__":
    main()

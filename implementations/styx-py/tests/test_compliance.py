#!/usr/bin/env python3
"""Compliance tests comparing Python parser against Rust reference."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path

import pytest

from styx.compliance import format_document, format_error
from styx.parser import Parser
from styx.types import ParseError


def find_corpus_path() -> Path:
    """Find the compliance corpus directory."""
    candidates = [
        Path(__file__).parent.parent.parent.parent / "compliance" / "corpus",
        Path(__file__).parent.parent.parent / "compliance" / "corpus",
    ]
    for c in candidates:
        if c.is_dir():
            return c
    pytest.skip("Could not find compliance corpus directory")


def find_styx_cli() -> Path:
    """Find the styx CLI binary."""
    candidates = [
        Path(__file__).parent.parent.parent.parent / "target" / "debug" / "styx",
        Path(__file__).parent.parent.parent.parent / "target" / "release" / "styx",
    ]
    for c in candidates:
        if c.exists():
            return c
    pytest.skip("styx-cli not found - run 'cargo build' first")


def get_python_output(content: str) -> str:
    """Get Python parser output as sexp."""
    try:
        parser = Parser(content)
        doc = parser.parse()
        return format_document(doc)
    except ParseError as e:
        return format_error(e)


def get_rust_output(file_path: Path, styx_cli: Path) -> str:
    """Get Rust reference output as sexp."""
    result = subprocess.run(
        [str(styx_cli), "tree", "--format", "sexp", str(file_path)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0 and result.stderr:
        # Extract error from stderr
        return extract_error_from_stderr(result.stderr)
    return result.stdout


def extract_error_from_stderr(stderr: str) -> str:
    """Extract error sexp from stderr."""
    match = re.search(r"parse error at (\d+)-(\d+): (.+)", stderr)
    if match:
        start, end, msg = match.groups()
        return f'(error [{start}, {end}] "parse error at {start}-{end}: {msg}")'
    return f'(error [-1, -1] "{stderr.strip()}")'


def normalize_output(output: str) -> str:
    """Normalize output for comparison."""
    lines = output.split("\n")
    result = []
    for line in lines:
        trimmed = line.strip()
        if trimmed.startswith("; file:"):
            continue
        if not trimmed:
            continue
        result.append(trimmed)
    return "\n".join(result)


def parse_error_span(output: str) -> tuple[tuple[int, int] | None, str]:
    """Extract error span and message from sexp output."""
    match = re.search(r'\(error \[(\d+), (\d+)\] "([^"]*)"', output)
    if match:
        start = int(match.group(1))
        end = int(match.group(2))
        msg = match.group(3)
        return (start, end), msg
    return None, ""


def annotate_span(source: str, start: int, end: int, msg: str) -> str:
    """Show source with carets under the error span, handling multi-line spans."""
    if start < 0 or end < 0 or start > len(source):
        return f"  [invalid span {start}-{end}]\n"
    if end > len(source):
        end = len(source)

    # Find all lines that overlap with the span
    lines = []
    pos = 0
    for line_text in source.split("\n"):
        line_start = pos
        line_end = pos + len(line_text)
        # Check if this line overlaps with [start, end)
        if line_end >= start and line_start < end:
            lines.append((line_text, line_start, line_end))
        pos = line_end + 1  # +1 for the newline
        if line_start >= end:
            break

    if not lines:
        return f"  [span {start}-{end} not found]\n"

    result = []
    for line_text, line_start, line_end in lines:
        result.append(f"  {line_text}\n")
        # Calculate caret positions for this line
        caret_start = max(start, line_start) - line_start
        caret_end = min(end, line_end) - line_start
        width = caret_end - caret_start
        if width < 1:
            width = 1
        result.append(f"  {' ' * caret_start}{'^' * width}\n")
    result.append(f"  {msg} ({start}-{end})\n")
    return "".join(result)


def annotate_error_diff(source: str, py_output: str, rust_output: str) -> str:
    """Show the first error span difference with source context."""
    py_span, py_msg = parse_error_span(py_output)
    rust_span, rust_msg = parse_error_span(rust_output)

    if py_span is None and rust_span is None:
        return ""  # No errors to annotate

    lines = ["\n"]

    if rust_span is not None:
        lines.append("Expected error:\n")
        lines.append(annotate_span(source, rust_span[0], rust_span[1], rust_msg))
        lines.append("\n")
    else:
        lines.append("Expected: no error\n\n")

    if py_span is not None:
        lines.append("Got error:\n")
        lines.append(annotate_span(source, py_span[0], py_span[1], py_msg))
    else:
        lines.append("Got: no error\n")

    return "".join(lines)


def collect_styx_files():
    """Collect all .styx files for parametrized testing."""
    corpus_path = find_corpus_path()
    return sorted(corpus_path.rglob("*.styx"))


# Collect files at module load time for pytest parametrization
try:
    STYX_FILES = collect_styx_files()
except Exception:
    STYX_FILES = []


@pytest.mark.parametrize(
    "styx_file",
    STYX_FILES,
    ids=lambda p: str(p.relative_to(p.parent.parent.parent)),
)
def test_compliance(styx_file: Path):
    """Test Python parser against Rust reference for a single file."""
    styx_cli = find_styx_cli()
    content = styx_file.read_text()

    py_output = get_python_output(content)
    rust_output = get_rust_output(styx_file, styx_cli)

    py_norm = normalize_output(py_output)
    rust_norm = normalize_output(rust_output)

    if py_norm != rust_norm:
        annotation = annotate_error_diff(content, py_output, rust_output)
        pytest.fail(
            f"output mismatch\n{annotation}\n"
            f"--- Python output ---\n{py_output}\n"
            f"--- Rust output ---\n{rust_output}"
        )

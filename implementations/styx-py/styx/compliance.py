#!/usr/bin/env python3
"""Compliance test runner for Styx Python parser."""

from __future__ import annotations

import sys
from pathlib import Path

from .parser import Parser
from .types import (
    Document,
    Entry,
    ParseError,
    Payload,
    Scalar,
    Sequence,
    StyxObject,
    Value,
)


def escape_string(s: str) -> str:
    """Escape a string for sexp output."""
    return (
        s.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
    )


def format_value(value: Value, indent: int) -> str:
    """Format a value as sexp."""
    prefix = "  " * indent

    # Unit value (no tag, no payload)
    if value.tag is None and value.payload is None:
        return f"(unit [{value.span.start}, {value.span.end}])"

    # Tag only (no payload)
    if value.tag is not None and value.payload is None:
        return f'(tag [{value.span.start}, {value.span.end}] "{value.tag.name}")'

    # Tag with payload
    if value.tag is not None and value.payload is not None:
        payload_str = format_payload(value.payload, indent + 1)
        return f'(tag [{value.span.start}, {value.span.end}] "{value.tag.name}"\n{prefix}  {payload_str})'

    # Just payload
    if value.payload is not None:
        return format_payload(value.payload, indent)

    return f"(unit [{value.span.start}, {value.span.end}])"


def format_payload(payload: Payload, indent: int) -> str:
    """Format a payload as sexp."""
    prefix = "  " * indent

    if isinstance(payload, Scalar):
        escaped = escape_string(payload.text)
        return (
            f'(scalar [{payload.span.start}, {payload.span.end}] {payload.kind.value} "{escaped}")'
        )

    if isinstance(payload, Sequence):
        if not payload.items:
            return f"(sequence [{payload.span.start}, {payload.span.end}])"
        items = "\n".join(f"{prefix}  {format_value(item, indent + 1)}" for item in payload.items)
        return f"(sequence [{payload.span.start}, {payload.span.end}]\n{items})"

    if isinstance(payload, StyxObject):
        if not payload.entries:
            return f"(object [{payload.span.start}, {payload.span.end}])"
        entries_str = "\n".join(format_entry(entry, indent + 1) for entry in payload.entries)
        return f"(object [{payload.span.start}, {payload.span.end}]\n{entries_str}\n{prefix})"

    return "(unknown)"


def format_entry(entry: Entry, indent: int) -> str:
    """Format an entry as sexp."""
    prefix = "  " * indent
    key_str = format_value(entry.key, indent + 1)
    value_str = format_value(entry.value, indent + 1)
    return f"{prefix}(entry\n{prefix}  {key_str}\n{prefix}  {value_str})"


def format_document(doc: Document) -> str:
    """Format a document as sexp."""
    if not doc.entries:
        return "(document [-1, -1]\n)"
    entries_str = "\n".join(format_entry(entry, 1) for entry in doc.entries)
    return f"(document [-1, -1]\n{entries_str}\n)"


def format_error(error: ParseError) -> str:
    """Format an error as sexp."""
    escaped_msg = error.message.replace("\\", "\\\\")
    return f'(error [{error.span.start}, {error.span.end}] "parse error at {error.span.start}-{error.span.end}: {escaped_msg}")'


def process_file(path: Path, corpus_root: Path) -> str:
    """Process a single styx file and return sexp output."""
    # Format: compliance/corpus/...
    relative = f"{corpus_root.parent.name}/{corpus_root.name}/{path.relative_to(corpus_root)}"
    content = path.read_text()

    try:
        parser = Parser(content)
        doc = parser.parse()
        return f"; file: {relative}\n{format_document(doc)}"
    except ParseError as e:
        return f"; file: {relative}\n{format_error(e)}"


def main() -> None:
    """Main entry point."""
    if len(sys.argv) < 2:
        print("Usage: styx-compliance <corpus-directory>", file=sys.stderr)
        sys.exit(1)

    corpus_path = Path(sys.argv[1])
    if not corpus_path.is_dir():
        print(f"Error: {corpus_path} is not a directory", file=sys.stderr)
        sys.exit(1)

    styx_files = sorted(corpus_path.rglob("*.styx"))

    results = []
    for path in styx_files:
        results.append(process_file(path, corpus_path))

    print("\n".join(results))


if __name__ == "__main__":
    main()

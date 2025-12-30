#!/usr/bin/env python3
"""
Replace all #[conformance(...)] test function bodies with a panic.
"""

import re
import os
from pathlib import Path

PANIC_BODY = 'panic!("all the old tests were garbage, we\'re remaking them all from scratch");'

def find_matching_brace(content: str, start: int) -> int:
    """Find the position of the closing brace that matches the opening brace at start."""
    depth = 0
    i = start
    in_string = False
    in_raw_string = False
    in_char = False
    in_line_comment = False
    in_block_comment = False
    raw_hashes = 0

    while i < len(content):
        # Handle line comments
        if in_line_comment:
            if content[i] == '\n':
                in_line_comment = False
            i += 1
            continue

        # Handle block comments
        if in_block_comment:
            if content[i:i+2] == '*/':
                in_block_comment = False
                i += 2
                continue
            i += 1
            continue

        # Check for comment starts
        if not in_string and not in_raw_string and not in_char:
            if content[i:i+2] == '//':
                in_line_comment = True
                i += 2
                continue
            if content[i:i+2] == '/*':
                in_block_comment = True
                i += 2
                continue

        # Handle raw strings r#"..."#
        if in_raw_string:
            if content[i] == '"':
                # Check for matching closing hashes
                j = i + 1
                closing_hashes = 0
                while j < len(content) and content[j] == '#' and closing_hashes < raw_hashes:
                    closing_hashes += 1
                    j += 1
                if closing_hashes == raw_hashes:
                    in_raw_string = False
                    i = j
                    continue
            i += 1
            continue

        # Handle regular strings
        if in_string:
            if content[i] == '\\' and i + 1 < len(content):
                i += 2
                continue
            if content[i] == '"':
                in_string = False
            i += 1
            continue

        # Handle char literals
        if in_char:
            if content[i] == '\\' and i + 1 < len(content):
                i += 2
                continue
            if content[i] == "'":
                in_char = False
            i += 1
            continue

        # Check for raw string start
        if content[i] == 'r':
            j = i + 1
            raw_hashes = 0
            while j < len(content) and content[j] == '#':
                raw_hashes += 1
                j += 1
            if j < len(content) and content[j] == '"':
                in_raw_string = True
                i = j + 1
                continue

        # Check for string start
        if content[i] == '"':
            in_string = True
            i += 1
            continue

        # Check for char start (but not lifetime 'a)
        if content[i] == "'" and i + 1 < len(content):
            # Peek ahead to see if this looks like a char literal
            # Char literals are short: 'x', '\n', '\x00', '\u{...}'
            if content[i+1] == '\\' or (i + 2 < len(content) and content[i+2] == "'"):
                in_char = True
                i += 1
                continue

        # Handle braces
        if content[i] == '{':
            depth += 1
        elif content[i] == '}':
            depth -= 1
            if depth == 0:
                return i

        i += 1

    return -1

def process_file(filepath: Path) -> tuple[int, str]:
    """Process a file and replace conformance test bodies. Returns (count, new_content)."""
    content = filepath.read_text()

    # Pattern to find #[conformance(...)] followed by pub async fn name(...) -> TestResult {
    # We need to be careful with multiline attributes
    pattern = re.compile(
        r'(#\[conformance\([^\]]*\)\]\s*pub\s+async\s+fn\s+\w+\s*\([^)]*\)\s*->\s*TestResult\s*)\{',
        re.DOTALL
    )

    count = 0
    new_content = content
    offset = 0

    for match in pattern.finditer(content):
        # Find the opening brace position in original content
        brace_start = match.end() - 1
        # Find matching closing brace in original content
        brace_end = find_matching_brace(content, brace_start)

        if brace_end == -1:
            print(f"  Warning: couldn't find matching brace in {filepath}")
            continue

        # Calculate positions in the modified content
        adj_brace_start = brace_start + offset
        adj_brace_end = brace_end + offset

        # Extract the old body (including braces)
        old_body = new_content[adj_brace_start:adj_brace_end + 1]
        new_body = f'{{\n    {PANIC_BODY}\n}}'

        # Replace
        new_content = new_content[:adj_brace_start] + new_body + new_content[adj_brace_end + 1:]

        # Update offset for next replacement
        offset += len(new_body) - len(old_body)
        count += 1

    return count, new_content

def main():
    # Find all Rust files in spec-tester/src/tests
    tests_dir = Path('spec-tester/src/tests')

    if not tests_dir.exists():
        print(f"Error: {tests_dir} not found. Run from repo root.")
        return

    total_count = 0

    for filepath in tests_dir.glob('*.rs'):
        # Check if file has conformance tests
        content = filepath.read_text()
        if '#[conformance(' not in content:
            continue

        count, new_content = process_file(filepath)

        if count > 0:
            filepath.write_text(new_content)
            print(f"  {filepath}: replaced {count} test(s)")
            total_count += count

    print(f"\nTotal: replaced {total_count} conformance test bodies")

if __name__ == '__main__':
    main()

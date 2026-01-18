#!/usr/bin/env python3
"""
Validate that all snapshot files contain valid Python code and pass type checking.
"""

import subprocess
import sys
import tempfile
from pathlib import Path


def extract_python_code(content: str) -> str:
    """Extract the Python code part from a snapshot file."""
    parts = content.split("---")
    if len(parts) >= 3:
        return parts[2].strip()
    return ""


def add_imports_if_needed(code: str) -> str:
    """Add typing imports if the code doesn't start with an import statement."""
    if not code.startswith("from typing import") and not code.startswith("import "):
        preamble = "from typing import Any, Literal, Required, TypedDict, Union\n\n"
        return preamble + code
    return code


def validate_syntax(code: str, filename: str) -> bool:
    """Try to compile the Python code to check if it's valid syntax."""
    try:
        compile(code, filename, "exec")
        return True
    except SyntaxError as e:
        print(f"  Syntax error: {e}")
        return False


def typecheck_code(code: str, filename: str) -> tuple[bool, str]:
    """Run mypy on the code to check for type errors."""
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".py", delete=False, encoding="utf-8"
    ) as f:
        f.write(code)
        temp_path = Path(f.name)

    try:
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "mypy",
                "--strict",
                "--no-error-summary",
                str(temp_path),
            ],
            capture_output=True,
            text=True,
        )
        output = result.stdout + result.stderr
        # Replace temp file path with original filename for readability
        output = output.replace(str(temp_path), filename)
        return result.returncode == 0, output.strip()
    except FileNotFoundError:
        return False, "mypy not found. Install with: pip install mypy"
    finally:
        temp_path.unlink(missing_ok=True)


def main():
    # Get the directory where this script is located
    script_dir = Path(__file__).parent
    snap_files = sorted(script_dir.glob("*.snap"))

    if not snap_files:
        print("No .snap files found!")
        return 1

    print(f"Found {len(snap_files)} snapshot files\n")

    syntax_failed = []
    typecheck_failed = []
    passed = []

    for snap_file in snap_files:
        print(f"Checking: {snap_file.name}")

        content = snap_file.read_text(encoding="utf-8")
        python_code = extract_python_code(content)

        if not python_code:
            print(f"  Warning: No Python code found in {snap_file.name}")
            continue

        # Add imports if needed
        python_code = add_imports_if_needed(python_code)

        # First check syntax
        if not validate_syntax(python_code, snap_file.name):
            print("  ✗ Syntax error")
            print(f"  Code:\n{python_code}\n")
            syntax_failed.append(snap_file.name)
            continue

        # Then run type checking
        typecheck_ok, typecheck_output = typecheck_code(python_code, snap_file.name)
        if typecheck_ok:
            print("  ✓ Valid Python code and passes type checking")
            passed.append(snap_file.name)
        else:
            print("  ✗ Type checking failed:")
            for line in typecheck_output.splitlines():
                print(f"    {line}")
            typecheck_failed.append(snap_file.name)

    print("\n" + "=" * 50)
    print(
        f"Results: {len(passed)} passed, {len(syntax_failed)} syntax errors, {len(typecheck_failed)} type errors"
    )

    if syntax_failed:
        print("\nFiles with syntax errors:")
        for f in syntax_failed:
            print(f"  - {f}")

    if typecheck_failed:
        print("\nFiles with type errors:")
        for f in typecheck_failed:
            print(f"  - {f}")

    if syntax_failed or typecheck_failed:
        return 1

    print("\nAll snapshot files contain valid, well-typed Python code!")
    return 0


if __name__ == "__main__":
    sys.exit(main())

# styx-py

Native Python parser for the [Styx configuration language](https://github.com/bearcove/styx).

## Installation

```bash
uv add styx
```

## Usage

```python
from styx import parse

doc = parse("""
name "My App"
version "1.0.0"
server {
    host localhost
    port 8080
}
""")

for entry in doc.entries:
    print(f"{entry.key} = {entry.value}")
```

## Development

```bash
# Install dev dependencies
uv sync --dev

# Run tests
uv run pytest

# Run linter
uv run ruff check .

# Run type checker
uv run ty check styx
```

## Publishing to PyPI

To publish a new version:

```bash
# Build the package
uv build

# Upload to PyPI (requires PYPI_API_TOKEN)
uv publish --token $PYPI_API_TOKEN
```

For CI/CD, add `PYPI_API_TOKEN` as a repository secret and create a workflow that triggers on version tags.

## License

MIT

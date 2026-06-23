+++
title = "Integrate with Python"
weight = 3
slug = "integrate-python"
insert_anchor_links = "heading"
+++

Add Styx to your Python application. The Python implementation is a native parser with no dependencies.

## Requirements

Python 3.12 or later.

## Installation

The package is not yet published to PyPI. Install from the repository:

```bash
# Clone and install locally
git clone https://github.com/bearcove/styx.git
cd styx/implementations/styx-py
pip install -e .
```

## Basic usage

```python
from styx import parse

doc = parse("""
    host localhost
    port 8080
    debug true
""")

# doc is a Document with entries
for entry in doc.entries:
    print(f"{entry.key.payload.text} = {entry.value.payload.text}")
```

## Working with the AST

The parser returns a typed AST:

```python
from styx import parse, Document, Entry, Value, Scalar, Sequence, StyxObject

doc = parse("""
    name "Alice"
    tags (admin user)
    settings {
        theme dark
    }
""")

for entry in doc.entries:
    key = entry.key.payload.text  # Scalar text
    value = entry.value

    if isinstance(value.payload, Scalar):
        print(f"{key} = {value.payload.text}")
    elif isinstance(value.payload, Sequence):
        items = [v.payload.text for v in value.payload.items]
        print(f"{key} = {items}")
    elif isinstance(value.payload, StyxObject):
        print(f"{key} = <object with {len(value.payload.entries)} entries>")
```

## Error handling

Parse errors include source location information:

```python
from styx import parse, ParseError

try:
    doc = parse('key "unterminated string')
except ParseError as e:
    print(f"Error: {e.message}")
    print(f"Location: bytes {e.span.start}-{e.span.end}")
```

## Source

[implementations/styx-py](https://github.com/bearcove/styx/tree/main/implementations/styx-py)

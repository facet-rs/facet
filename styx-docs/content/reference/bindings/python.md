+++
title = "Python"
weight = 2
slug = "python"
insert_anchor_links = "heading"
+++

Native Python implementation using modern Python 3.12+ features.

## Installation

```bash
pip install styx
# or with uv
uv add styx
```

## Usage

```python
from styx import parse

doc = parse('name "Alice"\nage 30')
```

## Requirements

Python 3.12+

## Source

[implementations/styx-py](https://github.com/bearcove/styx/tree/main/implementations/styx-py)

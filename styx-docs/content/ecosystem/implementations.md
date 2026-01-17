+++
title = "Implementations"
weight = 1
+++

Native Styx parsers are available in multiple languages.

## Rust

The reference implementation. Available as a crate on crates.io.

```bash
cargo add styx-parse
```

```rust
use styx_parse::parse;

let doc = parse("name \"Alice\"\nage 30")?;
```

## Python

Native Python implementation using modern Python 3.12+ features.

```bash
pip install styx
# or with uv
uv add styx
```

```python
from styx import parse

doc = parse('name "Alice"\nage 30')
```

**Requirements:** Python 3.12+

**Source:** [implementations/styx-py](https://github.com/bearcove/styx/tree/main/implementations/styx-py)

## Go

Native Go implementation.

```bash
go get github.com/bearcove/styx/implementations/styx-go
```

```go
import styx "github.com/bearcove/styx/implementations/styx-go"

doc, err := styx.Parse(`name "Alice"
age 30`)
```

**Requirements:** Go 1.22+

**Source:** [implementations/styx-go](https://github.com/bearcove/styx/tree/main/implementations/styx-go)

## JavaScript / TypeScript

Native TypeScript implementation with full type definitions.

```bash
npm install @aspect/styx
```

```typescript
import { parse } from '@aspect/styx';

const doc = parse('name "Alice"\nage 30');
```

**Source:** [implementations/styx-js](https://github.com/bearcove/styx/tree/main/implementations/styx-js)

## Compliance Testing

All implementations are tested against a shared compliance corpus to ensure consistent behavior. The test suite covers:

- Scalars (bare, quoted, raw, heredoc)
- Objects and sequences
- Tags and unit values
- Dotted paths
- Attribute syntax
- Error cases

See the [compliance corpus](https://github.com/bearcove/styx/tree/main/compliance/corpus) for examples.

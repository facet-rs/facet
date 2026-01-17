# Language Implementations

**Status:** TODO  
**Priority:** Medium

## Goal

Native styx implementations in other languages.

## Python

**styx-py** - Pure Python implementation

```python
import styx

config = styx.load("config.styx")
config = styx.parse(source_string)
styx.dump(value)  # → styx string
styx.validate(config, schema)
```

Implementation:
- Hand-written recursive descent parser (simple!)
- Dict/list output (like json module)
- Schema validation optional

The format is simple enough that a native impl is straightforward:
- Tokenizer: strings, bare scalars, tags, braces, parens
- Parser: recursive descent, ~500 lines probably

## JavaScript/TypeScript

**styx-js** - Pure JS/TS implementation

```typescript
import * as styx from 'styx';

const config = styx.parse(source);
const source = styx.stringify(value);
```

Implementation:
- TypeScript with good types
- Works in Node, Deno, Bun, browsers
- No native deps, no wasm
- Tree-sitter grammar already exists for editors (separate concern)

## Benefits of Native Implementations

- No build complexity (no Rust toolchain needed)
- Smaller install size
- Easier to debug/contribute
- Platform independent
- The format is simple - leverage that!

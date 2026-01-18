+++
title = "Integrate with JavaScript"
weight = 4
slug = "integrate-javascript"
insert_anchor_links = "heading"
+++

Add Styx to your JavaScript or TypeScript application. The implementation is written in TypeScript with full type definitions.

## Requirements

Node.js 18 or later.

## Installation

The package is not yet published to npm. Install from the repository:

```bash
# Clone and install locally
git clone https://github.com/bearcove/styx.git
cd styx/implementations/styx-js
npm install
npm run build
```

Or link it into your project:

```bash
cd styx/implementations/styx-js
npm link

# In your project
npm link @bearcove/styx
```

## Basic usage

```typescript
import { parse } from "@bearcove/styx";

const doc = parse(`
    host localhost
    port 8080
    debug true
`);

// doc is a Document with entries
for (const entry of doc.entries) {
    const key = entry.key.payload;
    const value = entry.value.payload;
    if (key?.kind === "scalar" && value?.kind === "scalar") {
        console.log(`${key.text} = ${value.text}`);
    }
}
```

## Working with the AST

The parser returns a typed AST:

```typescript
import { parse, Document, Scalar, Sequence, StyxObject } from "@bearcove/styx";

const doc = parse(`
    name "Alice"
    tags (admin user)
    settings {
        theme dark
    }
`);

for (const entry of doc.entries) {
    const key = entry.key.payload as Scalar;
    const value = entry.value.payload;

    if (!value) {
        console.log(`${key.text} = <unit>`);
    } else if (value.kind === "scalar") {
        console.log(`${key.text} = ${value.text}`);
    } else if (value.kind === "sequence") {
        const items = value.items.map(v => (v.payload as Scalar).text);
        console.log(`${key.text} = [${items.join(", ")}]`);
    } else if (value.kind === "object") {
        console.log(`${key.text} = <object with ${value.entries.length} entries>`);
    }
}
```

## Error handling

Parse errors include source location information:

```typescript
import { parse, ParseError } from "@bearcove/styx";

try {
    const doc = parse('key "unterminated string');
} catch (e) {
    if (e instanceof ParseError) {
        console.log(`Error: ${e.message}`);
        console.log(`Location: bytes ${e.span.start}-${e.span.end}`);
    }
}
```

## TypeScript support

Full TypeScript definitions are included. The AST types are fully typed:

```typescript
import type { Document, Value, Scalar, Sequence, StyxObject, Entry } from "@bearcove/styx";
```

## Source

[implementations/styx-js](https://github.com/bearcove/styx/tree/main/implementations/styx-js)

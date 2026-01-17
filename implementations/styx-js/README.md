# @bearcove/styx

A JavaScript/TypeScript parser for the [Styx configuration language](https://github.com/bearcove/styx).

## Installation

```bash
npm install @bearcove/styx
```

## Usage

### Parsing a document

```typescript
import { parse } from "@bearcove/styx";

const doc = parse(`
  name "My Application"
  port 8080
  
  server {
    host localhost
    tls true
  }
  
  routes (
    @route{path "/", handler index}
    @route{path "/api", handler api}
  )
`);

// doc.entries is an array of { key, value } pairs
for (const entry of doc.entries) {
  console.log("Key:", entry.key);
  console.log("Value:", entry.value);
}
```

### Accessing values

```typescript
import { parse, Value, Scalar, StyxObject, Sequence } from "@bearcove/styx";

const doc = parse(`name "hello"`);
const entry = doc.entries[0];

// Check the key
if (entry.key.payload?.type === "scalar") {
  console.log("Key:", entry.key.payload.text); // "name"
}

// Check the value
if (entry.value.payload?.type === "scalar") {
  console.log("Value:", entry.value.payload.text); // "hello"
  console.log("Kind:", entry.value.payload.kind); // "quoted"
}
```

### Working with tags

```typescript
import { parse } from "@bearcove/styx";

const doc = parse(`status @ok`);
const entry = doc.entries[0];

if (entry.value.tag) {
  console.log("Tag name:", entry.value.tag.name); // "ok"
}

// Tags can have payloads
const doc2 = parse(`color @rgb(255 128 0)`);
const color = doc2.entries[0].value;

if (color.tag && color.payload?.type === "sequence") {
  console.log("Tag:", color.tag.name); // "rgb"
  console.log("R:", color.payload.items[0].payload?.text); // "255"
}
```

### Handling errors

```typescript
import { parse, ParseError } from "@bearcove/styx";

try {
  const doc = parse(`obj { unclosed`);
} catch (e) {
  if (e instanceof ParseError) {
    console.log("Parse error:", e.message);
    console.log("At position:", e.span.start, "-", e.span.end);
  }
}
```

## API Reference

### `parse(source: string): Document`

Parses a Styx source string and returns a `Document`.

### Types

```typescript
interface Document {
  entries: Entry[];
  span: Span;
}

interface Entry {
  key: Value;
  value: Value;
}

interface Value {
  tag?: Tag;
  payload?: Scalar | Sequence | StyxObject;
  span: Span;
}

interface Tag {
  name: string;
  span: Span;
}

interface Scalar {
  type: "scalar";
  text: string;
  kind: "bare" | "quoted" | "raw" | "heredoc";
  span: Span;
}

interface Sequence {
  type: "sequence";
  items: Value[];
  span: Span;
}

interface StyxObject {
  type: "object";
  entries: Entry[];
  separator: "newline" | "comma";
  span: Span;
}

interface Span {
  start: number; // UTF-8 byte offset
  end: number;   // UTF-8 byte offset (exclusive)
}
```

### `ParseError`

Thrown when parsing fails. Has `message` and `span` properties.

## License

MIT OR Apache-2.0

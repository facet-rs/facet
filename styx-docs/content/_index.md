+++
title = "Styx"
insert_anchor_links = "heading"
+++

<div class="hero-intro">
<h1>Styx</h1>
<p class="tagline">A document language for mortals.</p>
</div>

```styx
styx-is (a document language)
with-features {
    that make 
    it {easy to-love} // also comments
} 
```

<div class="features">

<section class="feature">
<div class="feature-text">

## Minimal punctuation

Everything is space-separated, except inline object form:

</div>
<div class="feature-code">

```styx
sequences (look super clean)
an-object {
    can be
    multi line
}
or {inline style, with commas}
```

</div>
</section>

<section class="feature">
<div class="feature-text">

## Minimal quoting

Of course, you can have spaces and special chars:

</div>
<div class="feature-code">

```styx
one bare-scalar
two "double-quoted"
raw r#"raw quoted a-la Rust"#
finally <<HEREDOCS
    they work!
    HEREDOCS
```

</div>
</section>

<section class="feature">
<div class="feature-text">

## Minimal typing

Objects and sequences contain scalar key-values. Scalars are just opaque text at
this stage.

</div>
<div class="feature-code">

```yaml
country: NO   # boolean false
version: 3.10 # 3.1
comment: "This is a string for sure"
```

```styx
country NO   // opaque scalar "NO"
version 3.10 // opaque scalar "3.10"
comment "This is a string for sure" // nope, an opaque scalar
```

</div>
</section>

<style>
.layers-section {
  background: #dc143c;
  margin: 0;
  margin-left: calc(-50vw + 50%);
  margin-right: calc(-50vw + 50%);
  padding: 4rem 2rem;

  .section-header {
    text-align: center;
    margin-bottom: 2.5rem;
    max-width: 600px;
    margin-left: auto;
    margin-right: auto;
  }

  .section-title {
    font-family: var(--font-heading);
    font-size: 3.5rem;
    font-weight: 400;
    letter-spacing: -0.02em;
    text-transform: uppercase;
    color: #fff;
    margin: 0 0 -0.25rem 0;
    line-height: 1.1;
  }

  .section-subtitle {
    font-family: "Lato", sans-serif;
    font-size: 1.75rem;
    font-weight: 200;
    color: rgba(255,255,255,0.95);
    margin: 0 0 3.25rem 0;
  }

  .section-desc {
    font-size: 1rem;
    line-height: 1.6;
    color: rgba(255,255,255,0.8);
    margin: 0;

    em {
      color: #fff;
      font-style: normal;
      font-weight: 700;
    }

    a {
      color: #fff;
      text-decoration: underline;
    }
  }
}

.tooling-demos {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  gap: 1.5rem;
  max-width: 900px;
  margin: 0 auto;
}

.tooling-demo {
  background: rgba(0, 0, 0, 0.2);
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 12px;
  padding: 1.75rem;
  transition: transform 0.2s, background 0.2s;

  &:hover {
    background: rgba(0, 0, 0, 0.3);
    transform: translateY(-2px);
  }

  h3 {
    font-family: var(--font-heading);
    font-size: 1.25rem;
    font-weight: 400;
    text-transform: uppercase;
    letter-spacing: 0.02em;
    color: #fff;
    margin: 0 0 0.75rem 0;
  }

  p {
    font-size: 0.95rem;
    line-height: 1.5;
    color: rgba(255, 255, 255, 0.8);
    margin: 0 0 1.25rem 0;
  }

  a {
    display: inline-block;
    font-size: 0.85rem;
    font-weight: 600;
    color: #fff;
    text-decoration: none;
    border-bottom: 1px solid rgba(255, 255, 255, 0.5);
    transition: border-color 0.2s;

    &:hover {
      border-color: #fff;
    }
  }
}

.hero-intro h1 {
  font-family: var(--font-heading);
  font-weight: 400;
  text-transform: uppercase;
}

.feature-text h2 {
  font-family: var(--font-heading);
  font-weight: 400;
  text-transform: uppercase;
}

.layers-diagram {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0;
  max-width: 650px;
  margin: 0 auto;

  .code-block {
    min-width: 300px;
    margin-bottom: 0;
  }
}

.layer-box {
  background: var(--bg-code);
  border: 1px solid var(--border);
  border-radius: 8px;
  max-width: 400px;
  width: 100%;
  overflow: hidden;

  .layer-title {
    display: block;
    font-size: 0.7rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
    padding: 0.6rem 1rem;
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
  }

  pre {
    margin: 0;
    padding: 0.75rem 1rem;
    background: none;
    border: none;
    border-radius: 0;
  }

  code {
    font-family: var(--font-mono);
    font-size: 0.85rem;
    background: none;
    border: none;
    padding: 0;
    white-space: pre;
  }

  .code-header {
    display: none;
  }
}

.layers-diagram-horizontal {
  display: flex;
  flex-direction: row;
  align-items: stretch;
  gap: 1rem;
  max-width: 750px;
  margin: 0 auto;

  & + & {
    margin-top: 1.5rem;
  }

  .code-block {
    flex: 1;
    min-width: 280px;
    margin-bottom: 0;
  }

  @media (max-width: 600px) {
    flex-direction: column;
  }
}

.layer-arrow-horizontal {
  display: flex;
  align-items: center;
  margin: 0 -0.5rem;

  .arrow-svg-h {
    width: 80px;
    height: 40px;

    @media (max-width: 600px) {
      transform: rotate(90deg);
    }
  }
}

.layer-arrow {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 0.25rem 0;

  .arrow-svg {
    width: 200px;
    height: 105px;

    .arrow-label {
      font-family: "Lato", sans-serif;
      font-size: 10px;
      font-weight: 400;
      fill: rgba(255,255,255,0.85);
      text-transform: uppercase;
      letter-spacing: 0.1em;
      dominant-baseline: middle;
    }
  }
}

.svg-defs {
  position: absolute;
  width: 0;
  height: 0;
  overflow: hidden;
}

.cta-section {
  text-align: center;
  padding: 4rem 2rem;
  margin-top: 2rem;
}

.cta-header {
  font-family: var(--font-heading);
  font-size: 2rem;
  font-weight: 400;
  text-transform: uppercase;
  letter-spacing: -0.01em;
  color: var(--text);
  margin: 0 0 2rem 0;
}

.cta-buttons {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 1.5rem;
}

.cta-button {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-width: 220px;
  padding: 1.5rem 2.5rem;
  border-radius: 12px;
  text-decoration: none;
  transition: transform 0.2s, box-shadow 0.2s;

  &:hover {
    transform: translateY(-3px);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.15);
  }
}

.cta-primary {
  background: #dc143c;
  color: #fff;

  .cta-title {
    font-family: var(--font-heading);
    font-size: 1.4rem;
    font-weight: 400;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }

  .cta-desc {
    font-size: 0.9rem;
    opacity: 0.9;
    margin-top: 0.25rem;
  }
}

.cta-secondary {
  background: var(--bg-code);
  border: 2px solid var(--border);
  color: var(--text);

  .cta-title {
    font-family: var(--font-heading);
    font-size: 1.4rem;
    font-weight: 400;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }

  .cta-desc {
    font-size: 0.9rem;
    color: var(--text-muted);
    margin-top: 0.25rem;
  }
}
</style>

<svg class="svg-defs" aria-hidden="true">
  <defs>
    <path id="arrow-line-top" d="M100 0 L100 30" stroke="rgba(255,255,255,0.9)" stroke-width="3" fill="none"/>
    <path id="arrow-line-bottom" d="M100 62 L100 91" stroke="rgba(255,255,255,0.9)" stroke-width="3" fill="none"/>
    <path id="arrow-head" d="M100 105 L89 86 Q100 91 111 86 Z" fill="rgba(255,255,255,0.9)"/>
    <path id="arrow-head-left" d="M0 20 L19 9 Q14 20 19 31 Z" fill="rgba(255,255,255,0.9)"/>
    <path id="arrow-head-right" d="M80 20 L61 9 Q66 20 61 31 Z" fill="rgba(255,255,255,0.9)"/>
    <path id="arrow-line-h" d="M14 20 L66 20" stroke="rgba(255,255,255,0.9)" stroke-width="3" fill="none"/>
  </defs>
</svg>

<div class="layers-section">
<div class="section-header">
<p class="section-title">Get those types out of your document</p>
<p class="section-subtitle">and into your schema.</p>
<p class="section-desc">Styx schemas aren't just for objects and arrays — they're for <em>every scalar</em>.</p>
</div>

<div class="layers-diagram">

```styx
// input
host localhost
port 8080
```

  <div class="layer-arrow">
    <svg viewBox="0 0 200 105" class="arrow-svg">
      <use href="#arrow-line-top"/>
      <text x="100" y="46" text-anchor="middle" class="arrow-label">validated by</text>
      <use href="#arrow-line-bottom"/>
      <use href="#arrow-head"/>
    </svg>
  </div>

```styx
// schema
host @string
port @int
```

  <div class="layer-arrow">
    <svg viewBox="0 0 200 105" class="arrow-svg">
      <use href="#arrow-line-top"/>
      <text x="100" y="46" text-anchor="middle" class="arrow-label">deserialized into</text>
      <use href="#arrow-line-bottom"/>
      <use href="#arrow-head"/>
    </svg>
  </div>

```rust
Server {
    host: "localhost",
    port: 8080,
}
```

</div>
</div>

<section class="feature">
<div class="feature-text">

## It starts with a tree

At this point, it's all still objects, sequences, and opaque scalars.

</div>
<div class="feature-code">

```
server
├─ host: "localhost"
├─ port: "8080"          ← still text
└─ tls
   ├─ cert: "/path/cert.pem"
   └─ key: "/path/key.pem"
```

</div>
</section>

<section class="feature">
<div class="feature-text">

## Meaning on tap

Interpret scalars as typed values when you need them.

Durations, integers, dates — the rules are [in the spec](/reference/spec/scalars), not implementation-defined.

</div>
<div class="feature-code">

```rust
let port: u16 = doc["server"]["port"].get()?;
let timeout: Duration = doc["timeout"].get()?;
let created: DateTime = doc["created"].get()?;
```

```javascript
let port = doc.server.port.asInt()
let timeout = doc.timeout.asDuration()
let created = doc.created.asDateTime()
```

```python
port = doc["server"]["port"].as_int()
timeout = doc["timeout"].as_duration()
created = doc["created"].as_datetime()
```

</div>
</section>

<div class="layers-section">
<div class="section-header">
<p class="section-title">Standardized interpretation</p>
<p class="section-subtitle">not implementation-defined.</p>
<p class="section-desc">Durations like <em>30s</em> or <em>1h30m</em>. Integers like <em>0xff</em> or <em>1_000_000</em>. RFC 3339 dates. It's all <a href="/reference/spec/scalars">in the spec</a>.</p>
</div>

<div class="layers-diagram-horizontal">

```styx
timeout 30s
retry 1h30m
poll 500ms
ttl 7d
```

  <div class="layer-arrow-horizontal">
    <svg viewBox="0 0 80 40" class="arrow-svg-h">
      <use href="#arrow-head-left"/>
      <use href="#arrow-line-h"/>
      <use href="#arrow-head-right"/>
    </svg>
  </div>

```rust
Duration::from_secs(30)
Duration::from_secs(5400)
Duration::from_millis(500)
Duration::from_secs(604800)
```

</div>

<div class="layers-diagram-horizontal">

```styx
count 1_000_000
color 0xff5500
mask 0b1111_0000
mode 0o755
```

  <div class="layer-arrow-horizontal">
    <svg viewBox="0 0 80 40" class="arrow-svg-h">
      <use href="#arrow-head-left"/>
      <use href="#arrow-line-h"/>
      <use href="#arrow-head-right"/>
    </svg>
  </div>

```rust
1000000_i64
16733440_u32
240_u8
493_u32
```

</div>

<div class="layers-diagram-horizontal">

```styx
pi 3.141_592_653
avogadro 6.022e23
small 1.5e-10
max inf
```

  <div class="layer-arrow-horizontal">
    <svg viewBox="0 0 80 40" class="arrow-svg-h">
      <use href="#arrow-head-left"/>
      <use href="#arrow-line-h"/>
      <use href="#arrow-head-right"/>
    </svg>
  </div>

```rust
3.141592653_f64
6.022e23_f64
1.5e-10_f64
f64::INFINITY
```

</div>

<div class="layers-diagram-horizontal">

```styx
created 2024-03-15T14:30:00Z
enabled true
debug false
```

  <div class="layer-arrow-horizontal">
    <svg viewBox="0 0 80 40" class="arrow-svg-h">
      <use href="#arrow-head-left"/>
      <use href="#arrow-line-h"/>
      <use href="#arrow-head-right"/>
    </svg>
  </div>

```rust
DateTime(2024, 3, 15, 14, 30, 0, UTC)
true
false
```

</div>
</div>

<section class="feature">
<div class="feature-text">

## Skip the schema

Using Rust? Derive `Facet` on your types and deserialize directly.

No schema files, no code generation — your types are the schema.

</div>
<div class="feature-code">

```rust
#[derive(Facet)]
struct Server {
    host: String,
    port: u16,
    tls: Option<bool>,
}

let server: Server = facet_styx::from_str(input)?;
```

</div>
</section>

<section class="feature">
<div class="feature-text">

## Live the schema

Generate schemas from Rust types or write them by hand.

Doc comments become hover text in your editor and show up in error messages.

</div>
<div class="feature-code">

```styx
/// A server configuration.
Server @object {
  /// Hostname or IP address to bind to.
  host @default(localhost @string)

  /// Port number (1-65535).
  port @default(8080 @int{ min 1, max 65535 })

  /// Enable TLS. Defaults to false.
  tls @default(false @bool)
}
```

</div>
</section>

<section class="feature">
<div class="feature-text">

## Love the schema

Dynamically typed languages like JavaScript can get a fully-typed object 
through the schema:

</div>
<div class="feature-code">

```typescript
import { parse } from "@bearcove/styx";

const config = parse(input, schema);
console.log(config);
```

```bash
$ node index.ts
{
  "host": "localhost",
  "port": 8080,
  "created": 2024-03-15T14:30:00.000Z  // a Date!
}
```

</div>
</section>

<div class="layers-section tooling-section">
<div class="section-header">
<p class="section-title">Offensively nice tooling</p>
<p class="section-subtitle">because you deserve better.</p>
<p class="section-desc">Errors that actually help. Autocomplete that actually works. From your editor to your CI pipeline.</p>
</div>

<div class="tooling-demos">

<!-- TODO: Editor integration screenshot/gif -->
<div class="tooling-demo">
<h3>Editor Integration</h3>
<p>Hover docs, autocomplete, inline errors — powered by LSP.</p>
<a href="/tools/editor">Set up your editor →</a>
</div>

<!-- TODO: Terminal recording (asciinema or gif) -->
<div class="tooling-demo">
<h3>CLI Validation</h3>
<p>Validate in CI. Get actionable errors with "did you mean?" suggestions.</p>
<a href="/tools/cli">See the CLI →</a>
</div>

<!-- Web playground links -->
<div class="tooling-demo">
<h3>Web Playground</h3>
<p>Try Styx in your browser with full syntax highlighting and validation.</p>
<a href="/tools/playgrounds/codemirror">CodeMirror</a> | <a href="/tools/playgrounds/monaco">Monaco</a>
</div>

</div>
</div>

<div class="cta-section">
<p class="cta-header">What are you waiting for?</p>
<div class="cta-buttons">
<a href="/guide/primer" class="cta-button cta-primary">
<span class="cta-title">Learn Styx</span>
<span class="cta-desc">The language in 5 minutes</span>
</a>
<a href="/guide/integrate-rust" class="cta-button cta-primary">
<span class="cta-title">Use Styx</span>
<span class="cta-desc">In your app or CLI</span>
</a>
</div>
</div>

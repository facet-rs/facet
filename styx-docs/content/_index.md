+++
title = "Styx"
insert_anchor_links = "heading"
+++

<div class="hero-intro">
<h1>Styx</h1>
<p class="tagline">A document language for mortals.</p>
</div>

```styx
@schema ./server.schema.styx

server {
  host localhost
  port 8080
  tls cert=/etc/ssl/cert.pem
}

routes (
  @redirect{from /old, to /new}
  @proxy{path /api, upstream localhost:9000}
)
```

<div class="features">

<section class="feature">
<div class="feature-text">
<h2>Mortal-first</h2>
<p>No quotes for simple values. URLs, paths, and identifiers just work.</p>
</div>
<div class="feature-code">

```styx
host localhost
port 8080
url https://example.com/path
```

</div>
</section>

<section class="feature">
<div class="feature-text">
<h2>Key chains</h2>
<p>Nested structure without nesting. Keys chain to build deep paths.</p>
</div>
<div class="feature-code">

```styx
server host localhost
server port 8080

// expands to:
server {
  host localhost
  port 8080
}
```

</div>
</section>

<section class="feature">
<div class="feature-text">
<h2>Attribute syntax</h2>
<p>Inline key-value pairs for compact configuration.</p>
</div>
<div class="feature-code">

```styx
tls cert=/etc/ssl/cert.pem key=/etc/ssl/key.pem

// expands to:
tls { cert /etc/ssl/cert.pem, key /etc/ssl/key.pem }
```

</div>
</section>

<section class="feature">
<div class="feature-text">
<h2>Schema-driven</h2>
<p>Define types once. Get validation, autocomplete, and documentation.</p>
</div>
<div class="feature-code">

```styx
schema {
  @ @object{
    host @string
    port @int{min 1, max 65535}
    tls @optional(@TlsConfig)
  }

  TlsConfig @object{
    cert @string
    key @string
  }
}
```

</div>
</section>

<section class="feature">
<div class="feature-text">
<h2>Comments</h2>
<p>Line comments, inline comments, and doc comments that attach to entries.</p>
</div>
<div class="feature-code">

```styx
// line comment
host localhost  // inline comment

/// doc comment (attaches to next entry)
port 8080
```

</div>
</section>

</div>

<div class="hero-links">

[Learn Styx](/learn/primer) — a 5-minute primer

[Install](/tools/cli) — get the CLI

[Reference](/reference) — the spec

</div>

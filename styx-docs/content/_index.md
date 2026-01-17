+++
title = "Styx"
insert_anchor_links = "heading"
+++

# Styx

A document language for mortals.

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

<div class="hero-cards">

<div class="hero-card">
<h3>Mortal-first</h3>
<div class="carousel" data-carousel="mortal">
<div class="carousel-slides">
<div class="carousel-slide active">
<div class="slide-label">Bare scalars</div>

```styx
host localhost
port 8080
url https://example.com/path
```

</div>
<div class="carousel-slide">
<div class="slide-label">Key chains</div>

```styx
server host localhost
// expands to:
server { host localhost }
```

</div>
<div class="carousel-slide">
<div class="slide-label">Attribute syntax</div>

```styx
tls cert=/etc/ssl/cert.pem key=/etc/ssl/key.pem
// expands to:
tls { cert /etc/ssl/cert.pem, key /etc/ssl/key.pem }
```

</div>
<div class="carousel-slide">
<div class="slide-label">Comments</div>

```styx
// line comment
host localhost  // inline comment

/// doc comment (attaches to next entry)
port 8080
```

</div>
</div>
<div class="carousel-dots"></div>
</div>
</div>

<div class="hero-card">
<h3>Schema-driven</h3>
<div class="carousel" data-carousel="schema">
<div class="carousel-slides">
<div class="carousel-slide active">
<div class="slide-label">Schema</div>

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
<div class="carousel-slide">
<img src="https://placehold.co/400x200/f6f6f6/333?text=CLI+validation" alt="CLI validation">
</div>
<div class="carousel-slide">
<img src="https://placehold.co/400x200/f6f6f6/333?text=Zed+autocomplete" alt="Zed autocomplete">
</div>
</div>
<div class="carousel-dots"></div>
</div>
</div>

<div class="hero-card">
<h3>Tooling</h3>
<div class="carousel" data-carousel="tooling">
<div class="carousel-slides">
<div class="carousel-slide active">
<img src="https://placehold.co/400x200/f6f6f6/333?text=CLI+usage" alt="CLI usage">
</div>
<div class="carousel-slide">
<img src="https://placehold.co/400x200/f6f6f6/333?text=styx+fmt" alt="styx fmt">
</div>
<div class="carousel-slide">
<img src="https://placehold.co/400x200/f6f6f6/333?text=tree-sitter" alt="tree-sitter">
</div>
</div>
<div class="carousel-dots"></div>
</div>
</div>

</div>

<div class="hero-links">

[Learn Styx](/learn/primer) — a 5-minute primer

[Install](/tools/cli) — get the CLI

[Reference](/reference) — the spec

</div>

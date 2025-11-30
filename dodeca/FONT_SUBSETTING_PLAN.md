# Font Subsetting & Cache Busting

## Goal

1. Subset fonts to only include glyphs actually used on the site
2. Cache-bust all static assets for optimal browser caching

## Architecture

### Query-based design (Salsa)

```rust
// Font subsetting: depends on (font_file_content, chars_used)
#[salsa::tracked]
pub fn subset_font<'db>(
    db: &'db dyn Db,
    font_file: StaticFile,
    chars: CharSet<'db>,
) -> Option<Vec<u8>>

// Cache busting: content hash embedded in filename
// main.css → main.a1b2c3d4.css
// fonts/Inter.woff2 → fonts/Inter.deadbeef.woff2
```

### Build Pipeline

1. **Render HTML** (Salsa queries)
2. **Compile CSS** (Salsa query)
3. **Analyze fonts** - parse @font-face, collect chars per font-family
4. **Process static files** - subset fonts, hash all content
5. **Rewrite CSS** - update static URLs, hash CSS filename
6. **Rewrite HTML** - update CSS and static URLs

### Dependencies

```toml
# HTML + CSS parsing
scraper = "0.24"

# Font subsetting (vendored fontations from fontcull)
fontcull-klippa = "0.1"
fontcull-skrifa = "0.39"
fontcull-read-fonts = "0.36"
fontcull-write-fonts = "0.44"
woff = "0.6"  # WOFF2 compression

# Already had
rapidhash = "4"  # Content hashing
```

## Implementation Status ✅ Complete

### Font Subsetting
- [x] Parse CSS `@font-face` rules
- [x] Collect chars per font-family from HTML
- [x] Subset fonts via Salsa query
- [x] Compress to WOFF2

### Cache Busting
- [x] Hash static file content → embed in filename
- [x] Rewrite URLs in CSS
- [x] Hash CSS content → embed in filename
- [x] Rewrite URLs in HTML (CSS + static assets)

## Output Example

```
fonts/Inter.woff2      → fonts/Inter.a1b2c3d4.woff2
images/logo.png        → images/logo.deadbeef.png
main.css               → main.12345678.css

HTML: <link href="/main.12345678.css">
CSS:  url("/fonts/Inter.a1b2c3d4.woff2")
```

## Limitations

- URL rewriting uses simple string replacement (works for typical cases)

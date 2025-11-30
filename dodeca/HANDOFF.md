# Dodeca Handoff Instructions

## What is Dodeca?

A custom static site generator for facet docs, replacing Zola. Named after dodecahedron (12-faced polyhedron) to match the "facet" theme.

## Why?

- **Dogfooding**: Uses `facet-toml` for frontmatter parsing
- **No template language**: HTML generated with `maud` (type-safe, Rust)
- **Exactly what we need**: No framework fighting
- **Fast**: Incremental computation with Salsa
- **Incremental**: Only recompute what changed (powered by Salsa)

## Current State (~500 lines)

```
dodeca/
├── Cargo.toml
└── src/
    ├── main.rs      # CLI + BuildContext + tree building
    ├── db.rs        # Salsa database + SourceFile input + ParsedData
    ├── queries.rs   # Tracked parse_file() function
    ├── render.rs    # HTML generation (maud) + Sass compilation (grass)
    ├── serve.rs     # Dev server (axum)
    └── tui.rs       # Stub for ratatui progress UI
```

## Salsa Integration

Dodeca uses [Salsa](https://salsa-rs.github.io/salsa/) for incremental computation:

### Key Concepts

- **SourceFile** (`#[salsa::input]`): Represents a markdown file with its content
- **parse_file** (`#[salsa::tracked]`): Parses markdown to HTML, memoized by Salsa
- **Database**: Stores all memoized computation results

### How Incremental Rebuilds Work

1. **Initial build**: All files are loaded as `SourceFile` inputs
2. **File change**: Update the `SourceFile` with new content via `set_content()`
3. **Re-parse**: Call `parse_file()` - Salsa returns cached result if content unchanged
4. **Selective rebuild**: Only changed files get re-parsed, tree rebuilds use new data

### Example Usage

```rust
// Initial build
let mut ctx = BuildContext::new(content_dir, output_dir);
ctx.load_sources()?;
let parsed = ctx.parse_all();  // All files parsed

// File changes...
ctx.update_source(Path::new("learn/getting-started.md"))?;
let parsed = ctx.parse_all();  // Only changed file re-parsed!
```

## What's Done

- [x] Salsa database with SourceFile inputs
- [x] Tracked `parse_file()` function with `facet-toml` frontmatter
- [x] Site tree building from parsed data
- [x] HTML rendering with `maud` (sidebar, nav, layouts)
- [x] Sass compilation with `grass`
- [x] Basic serve mode with `axum`
- [x] CLI with `--address/-a` and `--port/-p` flags

## What's TODO

### 1. TUI with ratatui (Priority: High)
Show build progress in terminal:
```
┌─────────────────────────────────────────────────────────────┐
│  dodeca build                                               │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐           │
│  │ Content     │ │ Sass        │ │ Showcases   │           │
│  │ ██████████  │ │ ██████████  │ │ ████░░░░░░  │           │
│  │ 12/12 done  │ │ done        │ │ 4/8         │           │
│  └─────────────┘ └─────────────┘ └─────────────┘           │
│                                                             │
│  ✓ Build complete (1.2s)                     [serving :4000]│
└─────────────────────────────────────────────────────────────┘
```

Stub exists in `tui.rs`. Use `ratatui` + `crossterm`.

### 2. File Watching (Priority: High)
In serve mode, watch for changes and rebuild incrementally:
- Use `notify` crate (already in deps)
- On change: call `ctx.update_source()` then rebuild
- Salsa memoization means only changed files re-parse
- Integrate with TUI to show "rebuilding..."

### 3. Showcase Generation (Priority: Medium)
Port from `docs/build-website.rs`:
- Discover `*_showcase` examples in workspace
- Run them with `FACET_SHOWCASE_OUTPUT=markdown`
- Write output to `content/learn/showcases/`

### 4. Link Checking (Priority: Medium)
After HTML generation:
- Walk all `<a href="...">` in generated HTML
- Check internal links exist
- Check external links (optional, slow)
- Report broken links

### 5. SCSS/Sass Compilation (Priority: High)
The `compile_sass` function exists but needs work:
- Currently looks for `sass/main.scss` relative to content dir parent
- May need path configuration or auto-discovery
- Uses `grass` crate for compilation

### 6. Search Index (Priority: Low)
Shell out to `pagefind`:
```rust
Command::new("pagefind")
    .args(["--site", output_dir])
    .status()?;
```

## Architecture Notes

### Build Pipeline
```
Phase 1: Load               Phase 2: Parse (Salsa)      Phase 3: Tree
┌──────────────────┐       ┌────────────────────┐      ┌─────────────┐
│ Walk .md files   │       │ parse_file()       │      │ Build tree  │
│ Create SourceFile│──────▶│ (memoized!)        │─────▶│ from parsed │
│ inputs           │       │ facet-toml + md    │      │ data        │
└──────────────────┘       └────────────────────┘      └─────────────┘
                                    │
                                    ▼
                           Phase 4: Render (parallel)
                           ┌─────────────────────────┐
                           │ Render HTML with maud   │
                           │ Write to disk           │
                           └─────────────────────────┘
```

Key insight: Salsa memoizes `parse_file()` so incremental rebuilds skip unchanged files.

### Build Modes
- `BuildMode::Full` - blocks on link checking + search index (for CI)
- `BuildMode::Quick` - just HTML, async background tasks (for dev)

### Frontmatter
Uses `facet-toml` (dogfooding!):
```rust
#[derive(Facet)]
pub struct Frontmatter {
    #[facet(default)]
    pub title: String,
    #[facet(default)]
    pub weight: i32,
    // ...
}
```

## Testing

```bash
# Check compilation
cargo check -p dodeca

# Run tests
cargo nextest run -p dodeca

# Build docs (once content paths are adjusted)
cargo run -p dodeca -- build -c docs/content -o docs/public

# Serve docs
cargo run -p dodeca -- serve -c docs/content -o docs/public
```

## Migration from Zola

The current Zola setup lives in `docs/`:
- `docs/content/` - markdown files (keep as-is)
- `docs/sass/` - stylesheets (keep as-is)
- `docs/templates/` - Tera templates (replace with maud in render.rs)
- `docs/config.toml` - Zola config (not needed)

Dodeca reads from `content/` and `sass/`, outputs to `public/`.

## Dependencies

Key crates:
- `facet`, `facet-toml` - frontmatter parsing (dogfooding!)
- `salsa` - incremental computation (memoization)
- `pulldown-cmark` - markdown to HTML
- `maud` - HTML generation
- `grass` - Sass compilation
- `ignore` - file walking (respects .gitignore)
- `axum` - HTTP server
- `ratatui` - TUI (stub)
- `notify` - file watching (stub)

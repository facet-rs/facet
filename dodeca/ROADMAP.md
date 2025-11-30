# Dodeca Roadmap

## Completed

- [x] Salsa-based incremental computation
- [x] Content-addressed storage (rapidhash + canopydb)
- [x] Query stats tracking
- [x] Mini build TUI with real-time progress
- [x] Live reload in serve mode
- [x] Pagefind search indexing (integrated via library, not shell)

## In Progress

- [ ] Salsa persistence for intermediate computations (cache parsed markdown between runs)

## Planned Features

### Link Checking (built-in)

Build our own link checker:
- Extract all `<a href>` and `<img src>` from rendered HTML
- Check internal links exist in output
- Check external links (async, with caching)
- Report broken links with line numbers

### Font Subsetting (blocked)

Via [fontcull](https://github.com/fasterthanlime/fontcull):
- Waiting on fontcull to be published to crates.io as lib + bin
- Scan HTML/CSS for used characters
- Subset fonts to only include used glyphs

### Other Ideas

- Image optimization (resize, convert to webp/avif)
- CSS purging (remove unused styles)
- HTML minification
- Sitemap generation
- RSS/Atom feed generation

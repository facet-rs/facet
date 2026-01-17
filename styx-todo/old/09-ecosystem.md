# Ecosystem & Adoption

**Status:** TODO  
**Priority:** Low

## Documentation

### Website (styx.bearcove.eu)
- Tutorial / getting started
- Language specification
- Schema guide
- Tool documentation
- Playground (interactive editor)

### Examples Repository
- Real-world config examples
- Schema library (common patterns)
- Migration guides from JSON/YAML/TOML

## Community

### Package Registry
- crates.io (already there)
- PyPI (styx-py)
- npm (styx-js)

### Integrations
- Config crate (like `config-rs` but for styx)
- Framework integrations (axum, actix, etc.)
- CI/CD tools (validate configs in pipelines)

## Adoption Strategy

1. Use in bearcove projects first (dogfooding)
2. Write migration tools (JSON/YAML → styx)
3. Blog posts explaining benefits
4. Conference talks?

## Comparison Content

- "Styx vs YAML" - no indent sensitivity
- "Styx vs JSON" - comments, trailing commas, less noise
- "Styx vs TOML" - nested structures, sequences, schemas
- "Styx vs KDL" - similar vibes, different syntax choices

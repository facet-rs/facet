# Future Ideas

**Status:** Backlog  
**Priority:** When everything else is done

## Language Features

### Schema Imports
Allow schemas to import types from other schemas:
```styx
@import types from "./common.schema.styx"
```

### Schema Inheritance
Allow extending existing schemas:
```styx
@extends "./base.schema.styx"
```

### Conditional Fields
Fields that are required based on other field values:
```styx
// if mode is "tls", require cert and key
```

## Tooling

### Schema Generator
Generate schema from example document:
```
styx infer example.styx > schema.styx
```

### Migration Tool
Help migrate from JSON/YAML/TOML to Styx:
```
styx convert config.json > config.styx
```

### Web Playground
Interactive styx editor in browser with:
- Syntax highlighting
- Live validation
- Schema editing

## Integration

### Config Library
Rust library for loading styx config with:
- File watching
- Reload on change
- Environment variable interpolation

### Build Tool Integration
Plugins for:
- Buck2 (styx configs for build rules)
- Cargo (Cargo.styx anyone?)

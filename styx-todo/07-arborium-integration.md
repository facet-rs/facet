# Arborium Integration

**Status:** TODO  
**Priority:** Medium

## Goal

Integrate styx as a configuration format in [arborium](https://github.com/bearcove/arborium).

## What is Arborium?

Tree-based data viewer/editor. Currently supports JSON, YAML, TOML, etc.

## Integration Points

### File Format Support
- Register `.styx` extension
- Parse styx → arborium tree model
- Serialize arborium tree → styx

### Schema Integration
- Load schema from `@ path` declaration
- Show field types/docs in UI
- Validate on edit
- Auto-complete from schema

### Syntax Highlighting
- Use tree-sitter-styx grammar
- Or integrate styx semantic tokens

## Benefits

- Visual config editing with validation
- Schema-aware UI (shows valid fields, types)
- Convert between formats (styx ↔ JSON ↔ YAML)

# AI Tooling

**Status:** Done  
**Priority:** Medium

## Problem

Styx is a new format. AI assistants (Claude, GPT, Copilot, etc.) don't know how to write it correctly yet. They need to be taught:

1. The syntax (similar to but distinct from KDL, YAML, etc.)
2. How to validate a data file against its schema
3. Common patterns and idioms

## Solutions

### Claude Code Skill

Create a skill that teaches Claude Code about Styx:
- Syntax reference (nodes, arguments, properties, children, strings, etc.)
- Schema language overview
- How to run `styx <file>` to validate
- Common error messages and how to fix them

### MCP Server

Expose Styx functionality via MCP:
- `styx_validate` - validate a file, return diagnostics
- `styx_format` - format a file
- `styx_to_json` - convert to JSON for inspection
- `styx_schema_info` - get schema details for a file

This lets AI tools validate their output without parsing CLI output.

### CLI `--help` / `--syntax`

Add a CLI command that outputs a syntax reference:
```
styx @syntax        # print syntax cheatsheet
styx @schema-help   # print schema language reference
```

AI tools can call this to get up-to-date documentation.

### Prompt/Context File

Create a `STYX.md` or similar that projects can include to teach AI about their Styx usage:
- Link to syntax docs
- Project-specific schemas
- Validation commands

## Progress

### Done

- [x] **Claude Code skill** at `contrib/claude-skill/SKILL.md`
  - Covers syntax (scalars, objects, sequences, tags, heredocs)
  - Schema language reference
  - CLI usage for validation
  - Common mistakes section
- [x] **CLI command** `styx @skill` outputs the skill for easy installation
- [x] **Website docs** at `docs/content/guide/claude-skill.md`
- [x] **Installation README** with multiple install methods

### Future Ideas

- [ ] Consider MCP server for programmatic access (validate, format, etc.)
- [ ] Skills for other AI tools (GitHub Copilot, Cursor, etc.)

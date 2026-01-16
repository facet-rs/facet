# Claude Code Skill for Styx

This skill teaches [Claude Code](https://docs.anthropic.com/en/docs/claude-code) about Styx syntax and schemas.

## Installation

### Option 1: Copy manually

```bash
mkdir -p ~/.claude/skills/styx
cp SKILL.md ~/.claude/skills/styx/
```

### Option 2: Use the CLI

```bash
mkdir -p ~/.claude/skills/styx
styx @skill > ~/.claude/skills/styx/SKILL.md
```

### Option 3: One-liner from repo

```bash
mkdir -p ~/.claude/skills/styx && curl -sL https://raw.githubusercontent.com/bearcove/styx/main/contrib/claude-skill/SKILL.md > ~/.claude/skills/styx/SKILL.md
```

## Usage

Once installed, Claude Code will automatically have access to Styx syntax knowledge. You can also explicitly invoke it:

```
/styx
```

This teaches Claude about:
- Styx syntax (scalars, objects, sequences, tags, heredocs)
- Schema language and type constraints
- CLI commands for validation and formatting
- Common mistakes to avoid

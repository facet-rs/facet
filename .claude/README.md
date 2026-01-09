# .claude/ Directory

This directory contains Claude Code project configuration and guidance.

## Structure

- **CLAUDE.md** - Main project guidelines (**START HERE**)
- **skills/** - Specialized how-to guides for specific tasks
- **commands/** - Custom slash commands
- **hooks/** - Event hooks for tool calls
- **settings.local.json** - Local configuration

## When to Check Each File

### CLAUDE.md (Read First)
- Git workflow rules (NEVER merge to main)
- File editing rules (no sed!)
- Architecture/migration info (facet-format vs facet-json)
- Testing commands (cargo nextest, valgrind)
- Benchmarking quick commands
- Problem handling philosophy

### skills/ Directory (Check Before Starting Work)

| Skill | When to Read |
|-------|-------------|
| `benchmarking.md` | Before running/modifying benchmarks |
| `debug-with-valgrind.md` | When encountering crashes/SIGSEGV |
| `profiling.md` | For performance analysis |
| `use-facet-crates.md` | When using facet crates |

**Rule of thumb:** If you're about to search for "how to..." information, check skills/ first.

## For Claude Code Agents

1. **Read CLAUDE.md** at session start
2. **Check skills/** before domain-specific work
3. **Don't duplicate** information - reference existing docs
4. **When stuck**, check if guidance exists before asking user

## Common Workflows

### Before Running Benchmarks
1. Check CLAUDE.md "Benchmarking" section for quick commands
2. Read `.claude/skills/benchmarking.md` for full documentation
3. Remember: benchmarks are defined in `benchmarks.json`, not `.rs` files

### Before Debugging Crashes
1. Check `.claude/skills/debug-with-valgrind.md`
2. Use `cargo nextest run --profile valgrind <filters>`
3. Generated benchmark tests can be run under valgrind

### Before Performance Work
1. Check `.claude/skills/profiling.md`
2. Check `.claude/skills/benchmarking.md` for perf analysis
3. Review `bench-reports/perf/RESULTS.md` for current performance

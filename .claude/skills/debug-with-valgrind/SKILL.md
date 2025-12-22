---
name: debug-with-valgrind
description: Debug crashes, segfaults, and memory errors using valgrind integration with nextest through pre-configured profiles
---

# Debugging with Valgrind + Nextest

When you encounter crashes, segfaults, or memory errors in tests, use valgrind for proper debugging.

## Quick Usage

```bash
# Run a specific test under valgrind
cargo nextest run --profile valgrind -p PACKAGE --test TEST_FILE TEST_NAME

# Example
cargo nextest run --profile valgrind -p facet-format-json --test jit_deserialize test_jit_option_some
```

## How It Works

The project has valgrind pre-configured in `.config/nextest.toml`:

1. **Wrapper script**: Defined as `[scripts.wrapper.valgrind]`
   - Command: `valgrind --leak-check=full --show-leak-kinds=all ...`
   - Automatically wraps test execution

2. **Profile**: `[profile.valgrind]` applies the wrapper to all tests on Linux

3. **Running**: Use `--profile valgrind` flag with nextest

## Benefits

- ✅ **Automatic setup** - no manual valgrind commands needed
- ✅ **Proper configuration** - leak checking, error codes pre-configured
- ✅ **Integrated** - works with nextest filtering and test selection
- ✅ **Clean output** - nextest captures and formats valgrind output

## Don't Do This

❌ Running valgrind manually: `valgrind ./target/debug/deps/test-binary`
❌ Using raw `--no-run` + valgrind commands
❌ Playing hunt-the-segfault without proper tools

## Do This Instead

✅ Use the pre-configured profile: `cargo nextest run --profile valgrind <filters>`
✅ Let nextest handle the wrapper script integration
✅ Get clean, actionable valgrind output immediately

## Debugging Workflow

1. Test crashes with SIGSEGV
2. Run: `cargo nextest run --profile valgrind -p PACKAGE --test FILE TEST_NAME`
3. Valgrind shows exact line where invalid read/write occurs
4. Fix the bug
5. Verify with regular tests

## See Also

- Nextest wrapper scripts docs: https://nexte.st/docs/configuration/wrapper-scripts/
- Project nextest config: `.config/nextest.toml`

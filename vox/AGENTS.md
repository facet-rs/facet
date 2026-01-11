# Instructions for AI Agents Working on This Repository

## The Hierarchy

```
Specification (docs/content/)
      ↓
Implementation (codegen, runtime libraries)
      ↓
Tests (verify the implementation)
```

**The spec is the source of truth.** Everything else serves it.

## The Cardinal Rule: NO SHORTCUTS

When you encounter a gap—something the codegen doesn't generate, something the runtime doesn't support—you have two choices:

1. **CORRECT**: Fix the implementation (codegen, runtime, etc.) to support what's needed
2. **WRONG**: Write manual/ad-hoc code to work around the gap

**You MUST choose option 1. Always.**

## Why This Matters

Tests exist to verify that the *generated code* works. If you write manual code to make a test pass, you've tested nothing. You've wasted time creating an illusion of progress.

Examples of cheating (DO NOT DO THESE):

- Writing a manual `Dispatcher` struct instead of using/fixing codegen to generate one
- Manually encoding/decoding RPC messages instead of using/fixing generated client code
- Implementing protocol logic by hand instead of using the runtime library
- Any code that duplicates what codegen should produce

## When You Find a Gap

If the codegen doesn't generate something you need:

1. **STOP** - Do not work around it
2. **IDENTIFY** - What should the codegen produce that it doesn't?
3. **FIX** - Modify the codegen to produce the correct output
4. **THEN** - Write the test using the generated code

If you're unsure whether something should be generated or hand-written, **ask the user**.

## The Test Pyramid for This Repo

- **Spec compliance tests**: Verify implementations follow the spec
- **Codegen tests**: Verify generated code is correct
- **Integration tests**: Verify different language implementations interoperate

All of these test THE IMPLEMENTATION. None of them should contain ad-hoc reimplementations of what the codegen should produce.

## Red Flags You're About to Cheat

If you find yourself:

- Writing `impl ServiceDispatcher for MyManualDispatcher` — STOP
- Manually encoding message bytes that codegen should handle — STOP
- Copying patterns from one file because "that's how the other one does it" without checking if it's the right way — STOP
- Thinking "I'll just make this work for now" — STOP

## Architecture Reminder

**Rust:**
- `#[service]` proc macro → Generates everything: trait, Dispatcher, Client, method IDs, `service_detail()`

**Other languages (TypeScript, Go, Swift, etc.):**
- `roam-codegen` → Generates client/server code from `ServiceDetail`

If any codegen is missing functionality, FIX THE CODEGEN.

## Final Note

Your job is to build a correct, complete implementation. Not to make tests green. A green test that tests manual code is worthless. A failing test that reveals missing codegen functionality is valuable—it shows you what to fix next.


All the code you write must be in service of the spec. Use your tracey skill
and the tracey MCP to ensure that the spec and code stay in-sync.

## TypeScript

**NEVER use `npx tsc`** - it doesn't work properly in this repo.

To type-check TypeScript code, use:
```bash
cd typescript && pnpm check
```

This runs `tsgo` from `@typescript/native-preview`.

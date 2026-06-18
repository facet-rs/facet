
All the code you write must be in service of the spec. Use your tracey skill
and the tracey MCP to ensure that the spec and code stay in-sync.

## TypeScript

**NEVER use `npx tsc`** - it doesn't work properly in this repo.

To type-check TypeScript code, use:
```bash
cd typescript && pnpm check
```

This runs `tsgo` from `@typescript/native-preview`.

## CRITICAL: File Editing on Windows

### ⚠️ MANDATORY: Always Use Backslashes on Windows for File Paths

**When using Edit or MultiEdit tools on Windows, you MUST use backslashes (`\`) in file paths, NOT forward slashes (`/`).**

#### ❌ WRONG - Will cause errors:
```
Edit(file_path: "D:/repos/project/file.tsx", ...)
MultiEdit(file_path: "D:/repos/project/file.tsx", ...)
```

#### ✅ CORRECT - Always works:
```
Edit(file_path: "D:\repos\project\file.tsx", ...)
MultiEdit(file_path: "D:\repos\project\file.tsx", ...)
```

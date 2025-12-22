# Debugging JIT Crashes on Windows with WinDbg/CDB

## Finding the Test Binary

Use `cargo nextest list` to find the binary path for a specific test:

```bash
# List all tests with their binary paths
cargo nextest list

# Filter to find a specific test's binary
cargo nextest list | grep -B5 test_twitter_benchmark_types
```

Output shows binary paths like:
```
facet-json::jit_nested_correctness:
  bin: C:\Users\faste\facet\target\debug\deps\jit_nested_correctness-8c63ef12f848539f.exe
  ...
    test_twitter_benchmark_types
```

## Running CDB

```bash
# Run specific test under debugger
cdb ./target/debug/deps/<test_binary>.exe <test_name> --nocapture

# Example:
cdb ./target/debug/deps/jit_nested_correctness-8c63ef12f848539f.exe test_twitter_benchmark_types --nocapture
```

## Essential CDB Commands

### Running and Breakpoints
- `g` - Go (run until crash/breakpoint)
- `sxe av` - Break on access violation (0xc0000005) - **use this for memory corruption**
- `bp <address>` - Set breakpoint
- `bl` - List breakpoints
- `bc *` - Clear all breakpoints

### After a Crash
- `.lastevent` - Show the exception that occurred
- `r` - Show all registers (look for R13 clobbering, bad pointers)
- `k` - Stack trace
- `kv` - Stack trace with arguments
- `kp` - Stack trace with full parameters

### Disassembly
- `u` - Disassemble at current instruction
- `u <address>` - Disassemble at address
- `ub <address>` - Disassemble backwards from address
- `uf <function>` - Disassemble entire function

### Memory Inspection
- `dd <address>` - Display DWORDs at address
- `dq <address>` - Display QWORDs at address
- `da <address>` - Display ASCII string
- `db <address>` - Display bytes

### Registers of Interest (Windows x64 ABI)
- **RCX, RDX, R8, R9** - First 4 arguments
- **RAX** - Return value
- **R12-R15, RBX, RBP, RSI, RDI** - Callee-saved (preserved across calls)
- If R13/R14/R15 are corrupted after a call, suspect wrong calling convention

## Debugging JIT ABI Issues

### Symptoms of Calling Convention Mismatch
- R12-R15 get clobbered across `call` instructions
- Crash happens after returning from a helper function
- Stack misalignment

### Checking the Problem
1. Set breakpoint before suspicious call: `bp <address>`
2. Note callee-saved registers: `r r12 r13 r14 r15`
3. Step over the call: `p`
4. Check if they changed: `r r12 r13 r14 r15`

### Finding JIT Code vs Rust Helpers
- JIT code addresses are typically in a different range (dynamically allocated)
- Rust helper functions have symbols: `facet_format!jit_write_string`
- Use `ln <address>` to find nearest symbol

## Page Heap (for heap corruption)

Before running, enable page heap for more detailed heap corruption detection:

```bash
gflags /p /enable <exe_name>
```

Then run under CDB. Disable when done:
```bash
gflags /p /disable <exe_name>
```

## Quick Workflow

```bash
# 1. Find the binary
cargo nextest list | grep -B5 <test_name>

# 2. Run under debugger
cdb <binary_path> <test_name> --nocapture

# 3. In CDB:
sxe av          # Break on access violation
g               # Run until crash
r               # Check registers
k               # Stack trace
u               # Disassemble crash location
```

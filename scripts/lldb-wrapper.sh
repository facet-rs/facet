#!/bin/bash
# Nextest wrapper script to run tests under lldb
# This automatically captures backtraces when tests crash (SIGABRT, SIGSEGV, etc.)

# Create a temp script that conditionally shows backtrace
CMDS=$(mktemp)
trap "rm -f $CMDS" EXIT

cat > "$CMDS" << 'EOF'
# Configure signal handling - stop on crashes but don't pass to process
process handle SIGABRT --stop true --pass false --notify true
process handle SIGSEGV --stop true --pass false --notify true
process handle SIGBUS --stop true --pass false --notify true
process handle SIGILL --stop true --pass false --notify true
process handle SIGTRAP --stop true --pass false --notify true

# Run the test
run

# Use Python to check if we should show backtrace
script
import lldb
target = lldb.debugger.GetSelectedTarget()
process = target.GetProcess()
state = process.GetState()

# Only show backtrace if process is stopped (crashed)
if state == lldb.eStateStopped:
    print("\n" + "="*60)
    print("CRASH DETECTED")
    print("="*60 + "\n")
    lldb.debugger.HandleCommand("thread backtrace all")
    exit_code = 1
else:
    # Process exited normally
    exit_code = process.GetExitStatus()

# Store exit code for later
lldb.debugger.HandleCommand(f"script lldb.test_exit_code = {exit_code}")
EOF

# Add quit command with exit code
echo 'script import sys; sys.exit(lldb.test_exit_code)' >> "$CMDS"

# Run lldb with the command script
exec lldb -b -s "$CMDS" -- "$@"

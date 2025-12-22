#!/usr/bin/env python3
"""
LLDB script to run tests and capture backtraces only on crashes.
Usage: lldb -s lldb-crash-handler.py -- <test-binary> <args>
"""

import lldb
import sys

def run_with_crash_detection(debugger, command, result, internal_dict):
    """Run the target and show backtrace only if it crashes."""
    target = debugger.GetSelectedTarget()
    process = target.GetProcess()

    # Launch the process
    error = lldb.SBError()
    process = target.Launch(debugger.GetListener(), None, None, None, None, None, None, None, False, error)

    if error.Fail():
        print(f"Failed to launch: {error}", file=sys.stderr)
        return

    # Wait for process to finish
    state = process.GetState()
    while state != lldb.eStateExited and state != lldb.eStateDetached:
        process.WaitForStateChangedEventsForRestartedProcess(10)
        state = process.GetState()

        # If stopped due to signal/crash
        if state == lldb.eStateStopped:
            thread = process.GetSelectedThread()
            stop_reason = thread.GetStopReason()

            # Check if stopped due to signal (crash)
            if stop_reason == lldb.eStopReasonSignal:
                print("\n" + "="*60)
                print(f"CRASH DETECTED - Stop reason: {thread.GetStopDescription(100)}")
                print("="*60 + "\n")

                # Show backtraces for all threads
                for thread in process:
                    print(f"\n{'─'*60}")
                    print(f"Thread #{thread.GetIndexID()}: {thread.GetName()}")
                    print(f"{'─'*60}")
                    for frame in thread:
                        print(f"  {frame}")
                    print()

                # Kill the crashed process
                process.Kill()
                sys.exit(1)
            else:
                # Continue if stopped for other reasons (breakpoints, etc.)
                process.Continue()

    # Get exit code and propagate it
    exit_code = process.GetExitStatus()
    if exit_code != 0:
        print(f"\nTest exited with code: {exit_code}", file=sys.stderr)
    sys.exit(exit_code)

# Register the command
def __lldb_init_module(debugger, internal_dict):
    debugger.HandleCommand('command script add -f lldb_crash_handler.run_with_crash_detection run_and_detect')

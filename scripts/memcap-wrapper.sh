#!/usr/bin/env bash
# memcap-wrapper: run a test process under a memory watchdog.
#
# Kills the wrapped process with SIGKILL if its RSS exceeds MEMCAP_MB
# (default 6144 MB). Exists because a runaway test once allocated 22GB in
# four seconds and took the whole machine down; nextest has timeouts but no
# memory limits, and macOS has no cgroups. Sampling beats nothing.
#
# The wrapper MUST exec the test binary rather than spawn it: nextest
# delivers timeout SIGTERM/SIGKILL to the wrapper's pid, and SIGKILL cannot
# be forwarded. A spawned child survives as an orphan burning CPU forever
# (observed: four fixture binaries at 95%+ CPU for over an hour after their
# nextest runs timed out). With exec, the test IS the wrapper's pid, so
# every signal lands on the test directly. The watchdog is spawned before
# the exec, monitoring $$ — the same pid before and after.
#
#   MEMCAP_MB        cap in megabytes (default 6144)
#   MEMCAP_INTERVAL  sampling interval in seconds (default 0.1)
set -u

MEMCAP_MB="${MEMCAP_MB:-6144}"
INTERVAL="${MEMCAP_INTERVAL:-0.1}"
CAP_KB=$((MEMCAP_MB * 1024))

pid=$$
(
  while kill -0 "$pid" 2>/dev/null; do
    rss_kb=$(ps -o rss= -p "$pid" 2>/dev/null | tr -d ' ')
    if [ -n "${rss_kb:-}" ] && [ "$rss_kb" -gt "$CAP_KB" ]; then
      echo "MEMCAP EXCEEDED: killing pid $pid (rss ${rss_kb}KB > cap ${MEMCAP_MB}MB): $*" >&2
      kill -9 "$pid" 2>/dev/null
      exit 0
    fi
    sleep "$INTERVAL"
  done
) &

exec "$@"

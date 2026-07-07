#!/usr/bin/env bash
# memcap-wrapper: run a test process under a memory watchdog.
#
# Kills the wrapped process with SIGKILL if its RSS exceeds MEMCAP_MB
# (default 6144 MB). Exists because a runaway test once allocated 22GB in
# four seconds and took the whole machine down; nextest has timeouts but no
# memory limits, and macOS has no cgroups. Sampling beats nothing.
#
#   MEMCAP_MB        cap in megabytes (default 6144)
#   MEMCAP_INTERVAL  sampling interval in seconds (default 0.1)
set -u

MEMCAP_MB="${MEMCAP_MB:-6144}"
INTERVAL="${MEMCAP_INTERVAL:-0.1}"
CAP_KB=$((MEMCAP_MB * 1024))

"$@" &
pid=$!

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
watchdog=$!

wait "$pid"
status=$?
kill "$watchdog" 2>/dev/null
wait "$watchdog" 2>/dev/null
exit "$status"

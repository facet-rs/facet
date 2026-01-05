#!/bin/sh
set -eu

JAVA_BIN="${JAVA_BIN:-}"
if [ -z "$JAVA_BIN" ]; then
  if [ -x /opt/homebrew/opt/openjdk/bin/java ]; then
    JAVA_BIN="/opt/homebrew/opt/openjdk/bin/java"
  elif command -v java >/dev/null 2>&1; then
    JAVA_BIN="java"
  else
    echo "java not found. Install OpenJDK 25 (e.g. 'brew install openjdk') and ensure it's available." >&2
    exit 1
  fi
fi

if ! "$JAVA_BIN" -version >/dev/null 2>&1; then
  echo "java at '$JAVA_BIN' is not functional. Prefer Homebrew OpenJDK 25 (e.g. 'brew install openjdk')." >&2
  exit 1
fi

exec "$JAVA_BIN" -cp java/subject/out SubjectJava

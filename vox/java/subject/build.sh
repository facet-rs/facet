#!/bin/sh
set -eu

JAVAC_BIN="${JAVAC_BIN:-}"
if [ -z "$JAVAC_BIN" ]; then
  if [ -x /opt/homebrew/opt/openjdk/bin/javac ]; then
    JAVAC_BIN="/opt/homebrew/opt/openjdk/bin/javac"
  elif command -v javac >/dev/null 2>&1; then
    JAVAC_BIN="javac"
  else
    echo "javac not found. Install OpenJDK 25 (e.g. 'brew install openjdk') and ensure it's available." >&2
    exit 1
  fi
fi

if ! "$JAVAC_BIN" -version >/dev/null 2>&1; then
  echo "javac at '$JAVAC_BIN' is not functional. Prefer Homebrew OpenJDK 25 (e.g. 'brew install openjdk')." >&2
  exit 1
fi

mkdir -p java/subject/out
"$JAVAC_BIN" -encoding UTF-8 -d java/subject/out java/subject/src/SubjectJava.java

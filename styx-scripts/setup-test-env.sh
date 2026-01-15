#!/usr/bin/env bash

# Exit with 1 if NEXTEST_ENV isn't defined.
if [ -z "$NEXTEST_ENV" ]; then
    exit 1
fi

# Disable colors for consistent snapshot tests
echo "NO_COLOR=1" >> "$NEXTEST_ENV"

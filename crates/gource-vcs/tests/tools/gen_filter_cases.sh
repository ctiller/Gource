#!/usr/bin/env bash
set -euo pipefail
export TZ=America/New_York

GOURCE_REF="${GOURCE_REF:?set GOURCE_REF to a C++ gource binary built from src/}"
PARITY_DIR="crates/gource-vcs/tests/data/parity/filters"
mkdir -p "$PARITY_DIR"

INPUT_LOG="crates/gource-vcs/tests/data/parity/custom/standard.log"
cp "$INPUT_LOG" "$PARITY_DIR/input.log"

# Case 1: file-filter
"$GOURCE_REF" --log-format custom --file-filter '\.md$' --output-custom-log "$PARITY_DIR/file_filter.expected" "$INPUT_LOG"

# Case 2: file-show-filter
"$GOURCE_REF" --log-format custom --file-show-filter '\.rs$' --output-custom-log "$PARITY_DIR/file_show_filter.expected" "$INPUT_LOG"

# Case 3: user-filter
"$GOURCE_REF" --log-format custom --user-filter '^Bob$' --output-custom-log "$PARITY_DIR/user_filter.expected" "$INPUT_LOG"

# Case 4: user-show-filter
"$GOURCE_REF" --log-format custom --user-show-filter '^Alice$' --output-custom-log "$PARITY_DIR/user_show_filter.expected" "$INPUT_LOG"

# Case 5: start-date / stop-date
"$GOURCE_REF" --log-format custom --start-date '2020-01-01 00:01:00' --output-custom-log "$PARITY_DIR/start_date.expected" "$INPUT_LOG"

echo "Filter cases generated successfully."

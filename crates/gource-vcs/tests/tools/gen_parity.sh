#!/usr/bin/env bash
set -euo pipefail

# Script to regenerate parity expected output files using the C++ reference binary.
# Fixed timezone for reproducible timestamps across mktime / localtime calls:
export TZ=America/New_York

GOURCE_REF="${GOURCE_REF:?set GOURCE_REF to a C++ gource binary built from src/}"
PARITY_DIR="crates/gource-vcs/tests/data/parity"

echo "Generating parity expected outputs using $GOURCE_REF with TZ=$TZ..."

formats=("git" "gitraw" "svn" "hg" "bzr" "cvs2cl" "cvs_exp" "apache" "custom")

for fmt in "${formats[@]}"; do
    dir="$PARITY_DIR/$fmt"
    if [ ! -d "$dir" ]; then
        continue
    fi
    for logfile in "$dir"/*.log; do
        if [ ! -f "$logfile" ]; then
            continue
        fi
        base="${logfile%.log}"
        expected_out="$base.expected"
        expected_err="$base.expected_err"
        expected_exit="$base.expected_exit"
        
        echo "Processing [$fmt]: $logfile"
        set +e
        if [ "$fmt" = "cvs_exp" ]; then
            "$GOURCE_REF" --output-custom-log "$expected_out" "$logfile" 2> "$expected_err"
        else
            "$GOURCE_REF" --log-format "$fmt" --output-custom-log "$expected_out" "$logfile" 2> "$expected_err"
        fi
        rc=$?
        set -e
        echo "$rc" > "$expected_exit"
    done
done

echo "Done regenerating parity files."

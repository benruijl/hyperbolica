#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
max_lines=${MAX_RUST_SOURCE_LINES:-600}

[[ "$max_lines" =~ ^[1-9][0-9]*$ ]] || {
    echo "module-size gate: MAX_RUST_SOURCE_LINES must be a positive integer" >&2
    exit 2
}

violations=0
while IFS= read -r -d '' source_file; do
    line_count=$(wc -l <"$source_file")
    if ((line_count > max_lines)); then
        relative_path=${source_file#"$repo_root/"}
        printf '%s: %d lines (limit %d)\n' "$relative_path" "$line_count" "$max_lines" >&2
        violations=$((violations + 1))
    fi
done < <(find "$repo_root/src" -type f -name '*.rs' -print0)

if ((violations > 0)); then
    echo "module-size gate: FAIL ($violations oversized source file(s))" >&2
    exit 1
fi

echo "module-size gate: every Rust source file is <= ${max_lines} lines"

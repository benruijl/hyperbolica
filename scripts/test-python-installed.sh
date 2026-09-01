#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ -n "${PYTHON:-}" ]]; then
    python_bin=$PYTHON
elif command -v python3 >/dev/null 2>&1; then
    python_bin=python3
else
    echo "test-python-installed: set PYTHON or install python3" >&2
    exit 1
fi

"$python_bin" tests/python_smoke.py

pickle_path=$(mktemp)
trap 'rm -f -- "$pickle_path"' EXIT
"$python_bin" tests/python_pickle_roundtrip.py write "$pickle_path"
"$python_bin" tests/python_pickle_roundtrip.py read "$pickle_path"
"$python_bin" tests/python_pickle_roundtrip.py read-perturbed "$pickle_path"

"$python_bin" tests/python_licensed.py

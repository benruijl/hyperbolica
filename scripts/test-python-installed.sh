#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
community=${SYMBOLICA_COMMUNITY_DIR:-"$repo_root/../symbolica-community/main"}
python_bin=${PYTHON:-python3}
cd "$community"
# Each test owns its kernel process, including subprocess citation checks.
# This works in community-license mode without competing for its runtime port.
mapfile -t tests < <("$python_bin" -m pytest tests/test_integration.py --collect-only -q | sed -n '/::/p')
[[ ${#tests[@]} -gt 0 ]]
for test in "${tests[@]}"; do
    "$python_bin" -m pytest "$test" -q
done

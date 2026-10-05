#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
python_bin=${PYTHON:-python3}
"$python_bin" -m unittest tests.test_benchmark_evidence tests.test_local_stage
"$python_bin" - <<'PYTHON'
import ast
from pathlib import Path
ast.parse(Path("src/python/integration_extras.pyi").read_text())
PYTHON
community=${SYMBOLICA_COMMUNITY_DIR:-"$repo_root/../symbolica-community/main"}
"${TY:-ty}" check --python "$python_bin" "$community/tests/typing/integration.py"

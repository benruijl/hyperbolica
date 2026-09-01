#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ -n "${PYTHON:-}" ]]; then
    python_bin=$PYTHON
elif command -v python3 >/dev/null 2>&1; then
    python_bin=python3
else
    echo "check-python-static: set PYTHON or install python3" >&2
    exit 1
fi

"$python_bin" - hyperbolica.pyi tests/python_smoke.py tests/python_licensed.py tests/python_pickle_roundtrip.py tests/python_typing.py <<'PY'
import ast
from pathlib import Path
import sys

for argument in sys.argv[1:]:
    path = Path(argument)
    source = path.read_text(encoding="utf-8")
    ast.parse(source, filename=str(path), type_comments=True)
    compile(source, str(path), "exec", dont_inherit=True)
PY

"$python_bin" - <<'PY'
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10, the oldest supported interpreter.
    source = Path("pyproject.toml").read_text(encoding="utf-8")
    assert 'module-name = "hyperbolica"' in source
    assert 'features = ["python-extension"]' in source
    assert '"Typing :: Typed"' in source
else:
    with Path("pyproject.toml").open("rb") as handle:
        config = tomllib.load(handle)

    assert config["tool"]["maturin"]["module-name"] == "hyperbolica"
    assert "python-extension" in config["tool"]["maturin"]["features"]
    assert "Typing :: Typed" in config["project"]["classifiers"]
PY

if [[ -n "${PYRIGHT:-}" ]]; then
    pyright_bin=$PYRIGHT
elif command -v pyright >/dev/null 2>&1; then
    pyright_bin=pyright
else
    echo "check-python-static: set PYRIGHT or install pyright" >&2
    exit 1
fi

"$pyright_bin" --pythonversion 3.10 --pythonpath "$python_bin" tests/python_typing.py
echo "Python syntax, typing stub, and maturin configuration are consistent"

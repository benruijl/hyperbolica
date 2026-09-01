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

"$python_bin" - hyperbolica.pyi tests/python_smoke.py tests/python_licensed.py tests/python_pickle_roundtrip.py tests/python_typing.py tests/test_benchmark_evidence.py <<'PY'
import ast
from pathlib import Path
import sys

for argument in sys.argv[1:]:
    path = Path(argument)
    source = path.read_text(encoding="utf-8")
    ast.parse(source, filename=str(path), type_comments=True)
    compile(source, str(path), "exec", dont_inherit=True)
PY

# This suite is CAS- and license-independent. Keep the qualification driver,
# process measurement, statistics, artifact hashing, and policy verifier wired
# into the ordinary repository gate rather than relying on implicit discovery.
"$python_bin" -m unittest tests.test_benchmark_evidence

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

mzv_source = Path("src/reduce/mzv_reduce.rs").read_text(encoding="utf-8")
bridge_source = Path("src/bridge/mzv_data.rs").read_text(encoding="utf-8")
options_source = Path("src/python/options.rs").read_text(encoding="utf-8")
narrow_source = Path("src/bridge/narrow.rs").read_text(encoding="utf-8")
assert 'include_bytes!("../../data/mzv_reductions.json")' in mzv_source
assert "OnceLock<MzvReductionTable>" in mzv_source
assert 'var_os("HYPERFLINT_DATA_DIR")' in bridge_source
assert "CARGO_MANIFEST_DIR" not in bridge_source
assert "if self.inner.mzv_reductions.is_embedded_standard()" in options_source
assert "Omitted MZV arguments reconstruct the embedded standard table" in options_source
assert "fn payload_strings" in narrow_source
assert "Value::Array(values)" in narrow_source
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
echo "Python syntax, benchmark evidence, typing stub, and maturin configuration are consistent"

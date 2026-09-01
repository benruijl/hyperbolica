#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ -n "${PYTHON:-}" ]]; then
    python_bin=$PYTHON
elif command -v python3 >/dev/null 2>&1; then
    python_bin=python3
else
    echo "check-python-package: set PYTHON or install python3" >&2
    exit 1
fi

if [[ -n "${MATURIN:-}" ]]; then
    maturin_bin=$MATURIN
elif command -v maturin >/dev/null 2>&1; then
    maturin_bin=maturin
else
    echo "check-python-package: set MATURIN or install maturin" >&2
    exit 1
fi

package_dir=$(mktemp -d)
trap 'rm -rf -- "$package_dir"' EXIT

"$maturin_bin" build \
    --locked \
    --interpreter "$python_bin" \
    --out "$package_dir"

"$python_bin" - "$package_dir" <<'PY'
from pathlib import Path
import sys
import zipfile

directory = Path(sys.argv[1])
wheels = sorted(directory.glob("hyperbolica-*.whl"))
assert len(wheels) == 1, f"expected one Hyperbolica wheel, found {wheels}"

with zipfile.ZipFile(wheels[0]) as archive:
    names = set(archive.namelist())
    metadata_files = [name for name in names if name.endswith(".dist-info/METADATA")]
    assert len(metadata_files) == 1, f"expected one METADATA file, found {metadata_files}"
    metadata_path = metadata_files[0]
    metadata = archive.read(metadata_path).decode("utf-8")

required = {
    "hyperbolica/__init__.py",
    "hyperbolica/__init__.pyi",
    "hyperbolica/py.typed",
}
missing = required - names
assert not missing, f"wheel is missing PEP 561/package files: {sorted(missing)}"

dist_info = metadata_path.rsplit("/", 1)[0]
required_licenses = {
    f"{dist_info}/licenses/DISTRIBUTION-LICENSE.md",
    f"{dist_info}/licenses/LICENSE",
}
missing_licenses = required_licenses - names
assert not missing_licenses, (
    f"wheel is missing required license files: {sorted(missing_licenses)}"
)
for license_file in ("DISTRIBUTION-LICENSE.md", "LICENSE"):
    assert f"License-File: {license_file}" in metadata, (
        f"METADATA does not declare {license_file}"
    )

native = [
    name
    for name in names
    if name.startswith("hyperbolica/hyperbolica.")
    and name.endswith((".so", ".pyd", ".dylib"))
]
assert len(native) == 1, f"expected one package-local native extension, found {native}"
loose_mzv_data = [name for name in names if name.endswith("/mzv_reductions.json")]
assert not loose_mzv_data, (
    "standard MZV data must be embedded in the native extension, not loaded "
    f"as a loose runtime file: {loose_mzv_data}"
)
print(
    f"validated {wheels[0].name}: typed package, native extension, and licenses are present"
)
PY

#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cargo_bin=${CARGO:-cargo}
library_path=${LIBHYPERBOLICA:-${1:-}}
cc_bin=${CC:-cc}
cxx_bin=${CXX:-c++}
run_smoke=${ABI_RUN_SMOKE:-1}

[[ "$run_smoke" =~ ^[01]$ ]] || {
    echo "check-abi: ABI_RUN_SMOKE must be 0 or 1" >&2
    exit 2
}

if [[ -z "$library_path" ]]; then
    (
        cd "$repo_root"
        "$cargo_bin" build --release --lib
    )
    case $(uname -s) in
        Darwin) library_path="$repo_root/target/release/libhyperbolica.dylib" ;;
        Linux) library_path="$repo_root/target/release/libhyperbolica.so" ;;
        *)
            echo "check-abi: unsupported host; set LIBHYPERBOLICA explicitly" >&2
            exit 2
            ;;
    esac
fi

[[ -f "$library_path" ]] || {
    echo "check-abi: library not found: $library_path" >&2
    exit 2
}
for tool in nm awk sed sort diff rg "$cc_bin" "$cxx_bin"; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "check-abi: required tool '$tool' is unavailable" >&2
        exit 2
    }
done

scratch=$(mktemp -d "${TMPDIR:-/tmp}/hyperbolica-abi.XXXXXXXX")
cleanup() {
    rm -rf -- "$scratch"
}
trap cleanup EXIT

case $(uname -s) in
    Darwin) nm -gU "$library_path" >"$scratch/nm.txt" ;;
    *) nm -D --defined-only "$library_path" >"$scratch/nm.txt" ;;
esac

awk '{ print $NF }' "$scratch/nm.txt" | sed 's/^_//' | rg '^hf_' | sort -u \
    >"$scratch/actual.txt"

golden="$repo_root/tests/abi/symbols_golden.txt"
if ! diff -u "$golden" "$scratch/actual.txt"; then
    echo "check-abi: exported hf_* symbol set differs from the whitelist" >&2
    exit 1
fi

rg -o 'hf_[a-z0-9_]+\(' "$repo_root/include/hyperbolica/c_abi.h" \
    | tr -d '(' | sort -u >"$scratch/header.txt"
if ! diff -u "$golden" "$scratch/header.txt"; then
    echo "check-abi: C header declarations differ from the export whitelist" >&2
    exit 1
fi

include_dir="$repo_root/include"
c_smoke="$repo_root/tests/abi/c_smoke.c"
cpp_smoke="$repo_root/tests/abi/header_cpp_smoke.cpp"
library_dir=$(cd -- "$(dirname -- "$library_path")" && pwd)

"$cc_bin" -std=c11 -Wall -Wextra -Werror -pedantic \
    -I"$include_dir" "$c_smoke" "$library_path" \
    -Wl,-rpath,"$library_dir" -o "$scratch/c-smoke"
"$cxx_bin" -std=c++17 -Wall -Wextra -Werror -pedantic -fsyntax-only \
    -I"$include_dir" "$cpp_smoke"

if [[ "$run_smoke" == 1 ]]; then
    case $(uname -s) in
        Darwin)
            DYLD_LIBRARY_PATH="$library_dir${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}" \
                "$scratch/c-smoke"
            ;;
        *)
            LD_LIBRARY_PATH="$library_dir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
                "$scratch/c-smoke"
            ;;
    esac
    echo "ABI gate: exact 8-symbol surface, C/C++ headers, link, and ownership smoke PASS"
else
    echo "check-abi: C smoke linked but was not run (ABI_RUN_SMOKE=0)"
    echo "ABI gate: exact 8-symbol surface, C/C++ headers, and link PASS"
fi

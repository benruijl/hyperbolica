#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cargo_bin=${CARGO:-cargo}
cc_bin=${CC:-cc}
cxx_bin=${CXX:-c++}
backend=${LIBHYPERBOLICA:-}
adapter=${HYPERBOLICA_LIBRARYLINK_LIBRARY:-}

if [[ -z "$backend" ]]; then
    (
        cd "$repo_root"
        "$cargo_bin" build --release --lib
    )
    case $(uname -s) in
        Darwin) backend="$repo_root/target/release/libhyperbolica.dylib" ;;
        Linux) backend="$repo_root/target/release/libhyperbolica.so" ;;
        *)
            echo "check-librarylink: this native gate currently supports macOS and Linux" >&2
            exit 2
            ;;
    esac
fi

if [[ -z "$adapter" ]]; then
    "$cargo_bin" build --release \
        --manifest-path "$repo_root/librarylink/Cargo.toml"
    case $(uname -s) in
        Darwin) adapter="$repo_root/librarylink/target/release/libhyperflint_librarylink.dylib" ;;
        Linux) adapter="$repo_root/librarylink/target/release/libhyperflint_librarylink.so" ;;
        *)
            echo "check-librarylink: this native gate currently supports macOS and Linux" >&2
            exit 2
            ;;
    esac
fi

[[ -f "$backend" ]] || {
    echo "check-librarylink: backend not found: $backend" >&2
    exit 2
}
[[ -f "$adapter" ]] || {
    echo "check-librarylink: adapter not found: $adapter" >&2
    exit 2
}

for tool in nm awk sed sort diff rg "$cc_bin" "$cxx_bin"; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "check-librarylink: required tool '$tool' is unavailable" >&2
        exit 2
    }
done

scratch=$(mktemp -d "${TMPDIR:-/tmp}/hyperbolica-librarylink.XXXXXXXX")
cleanup() {
    rm -rf -- "$scratch"
}
trap cleanup EXIT

case $(uname -s) in
    Darwin) nm -gU "$adapter" >"$scratch/nm.txt" ;;
    *) nm -D --defined-only "$adapter" >"$scratch/nm.txt" ;;
esac

awk '{ print $NF }' "$scratch/nm.txt" | sed 's/^_//' \
    | rg '^(WolframLibrary_|hf_)' | LC_ALL=C sort -u >"$scratch/actual.txt"
if ! diff -u "$repo_root/librarylink/tests/abi/symbols_golden.txt" \
        "$scratch/actual.txt"; then
    echo "check-librarylink: exported LibraryLink symbol set differs from the whitelist" >&2
    exit 1
fi

include_dir="$repo_root/librarylink/include"
smoke_source="$repo_root/librarylink/tests/abi/librarylink_smoke.c"
cpp_source="$repo_root/librarylink/tests/abi/header_cpp_smoke.cpp"
adapter_dir=$(cd -- "$(dirname -- "$adapter")" && pwd)

"$cc_bin" -std=c11 -Wall -Wextra -Werror -pedantic \
    -I"$include_dir" "$smoke_source" "$adapter" \
    -Wl,-rpath,"$adapter_dir" -o "$scratch/librarylink-smoke"
"$cxx_bin" -std=c++17 -Wall -Wextra -Werror -pedantic -fsyntax-only \
    -I"$include_dir" "$cpp_source"

HYPERBOLICA_LIBRARY_PATH="$backend" "$scratch/librarylink-smoke"

# Verify the relocatable production layout independently of the environment
# override: both libraries are co-located and the adapter finds its sibling
# from its own loaded-module path.
case $(uname -s) in
    Darwin)
        backend_name=libhyperbolica.dylib
        adapter_name=libhyperflint_librarylink.dylib
        ;;
    Linux)
        backend_name=libhyperbolica.so
        adapter_name=libhyperflint_librarylink.so
        ;;
esac
stage="$scratch/stage"
mkdir -p "$stage"
cp "$backend" "$stage/$backend_name"
cp "$adapter" "$stage/$adapter_name"
"$cc_bin" -std=c11 -Wall -Wextra -Werror -pedantic \
    -I"$include_dir" "$smoke_source" "$stage/$adapter_name" \
    -Wl,-rpath,"$stage" -o "$scratch/librarylink-staged-smoke"
env -u HYPERBOLICA_LIBRARY_PATH \
    -u HYPERBOLICA_LIBRARYLINK_LICENSED_TEST \
    "$scratch/librarylink-staged-smoke"

HYPERBOLICA_LIBRARY_PATH="$backend" \
    "$cargo_bin" test --manifest-path "$repo_root/librarylink/Cargo.toml"

echo "LibraryLink gate: exact 9-symbol surface, C/C++ ABI, lifecycle, marshalling, and delegation PASS"

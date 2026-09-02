#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
# shellcheck source=scripts/lib/response-comparison.sh
source "$repo_root/scripts/lib/response-comparison.sh"
fixtures=${DIFFERENTIAL_FIXTURES:-"$repo_root/tests/fixtures/differential.jsonl"}
rust_bin=${HYPERFLINT_RUST:-"$repo_root/target/release/hyperflint"}
cpp_bin=${HYPERFLINT_CPP:-/tmp/hyperflint-cpp-build/hyperflint}
cargo_bin=${CARGO:-cargo}
build_rust=${BUILD_RUST:-1}
timeout_seconds=${TIMEOUT_SECONDS:-120}

for tool in jq cmp mktemp; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "differential: required tool '$tool' is unavailable" >&2
        exit 2
    }
done

[[ -f "$fixtures" ]] || {
    echo "differential: fixture file not found: $fixtures" >&2
    exit 2
}
DIFFERENTIAL_FIXTURES="$fixtures" "$repo_root/scripts/lint-fixtures.sh"

if [[ "$build_rust" == 1 ]]; then
    "$cargo_bin" build --release --bin hyperflint
fi
for executable in "$rust_bin" "$cpp_bin"; do
    [[ -x "$executable" ]] || {
        echo "differential: executable not found: $executable" >&2
        exit 2
    }
done

scratch=$(mktemp -d "${TMPDIR:-/tmp}/hyperbolica-diff.XXXXXXXX")
cleanup() {
    rm -rf -- "$scratch"
}
trap cleanup EXIT

run_backend() {
    local backend=$1
    local executable=$2
    local request=$3
    local stdout_path=$4
    local stderr_path=$5
    local -a command=("$executable" eval-json)
    local -a environment=(env)
    if [[ "$backend" == cpp ]]; then
        environment+=(-u SYMBOLICA_LICENSE -u SYMBOLICA_LICENSE_SERVER)
    fi
    environment+=(SYMBOLICA_HIDE_BANNER=1)
    if command -v timeout >/dev/null 2>&1; then
        command=(timeout "$timeout_seconds" "${command[@]}")
    fi
    printf '%s\n' "$request" |
        "${environment[@]}" "${command[@]}" >"$stdout_path" 2>"$stderr_path"
}

passed=0
failed=0
not_byte_comparable=0
line_number=0
while IFS= read -r fixture || [[ -n "$fixture" ]]; do
    line_number=$((line_number + 1))
    [[ -z "$fixture" ]] && continue
    if ! jq -e . >/dev/null 2>&1 <<<"$fixture"; then
        echo "[FAIL] invalid fixture JSON on line $line_number" >&2
        failed=$((failed + 1))
        continue
    fi
    name=$(jq -r '.name' <<<"$fixture")
    compare=$(jq -r '.compare // "byte"' <<<"$fixture")
    request=$(jq -c '.request' <<<"$fixture")
    ignored=$(jq -c '.ignore // []' <<<"$fixture")
    ignored_recursive=$(jq -c '.ignore_recursive // []' <<<"$fixture")
    reason=$(jq -r '.reason // ""' <<<"$fixture")
    semantic_fields=$(jq -c '.semantic_fields // []' <<<"$fixture")
    semantic_variables=$(jq -c '.request.vars // []' <<<"$fixture")
    permutation_field=$(jq -r '.permutation_field // ""' <<<"$fixture")
    permutation_values=$(jq -c '.permutation_values // []' <<<"$fixture")
    rust_out="$scratch/${line_number}.rust.json"
    cpp_out="$scratch/${line_number}.cpp.json"
    rust_err="$scratch/${line_number}.rust.stderr"
    cpp_err="$scratch/${line_number}.cpp.stderr"

    if ! run_backend rust "$rust_bin" "$request" "$rust_out" "$rust_err"; then
        echo "[FAIL] $name: Rust backend failed" >&2
        sed -n '1,20p' "$rust_err" >&2
        failed=$((failed + 1))
        continue
    fi
    if ! run_backend cpp "$cpp_bin" "$request" "$cpp_out" "$cpp_err"; then
        echo "[FAIL] $name: C++ oracle failed" >&2
        sed -n '1,20p' "$cpp_err" >&2
        failed=$((failed + 1))
        continue
    fi
    if ! jq -e . "$rust_out" >/dev/null 2>&1 || ! jq -e . "$cpp_out" >/dev/null 2>&1; then
        echo "[FAIL] $name: a backend returned non-JSON output" >&2
        failed=$((failed + 1))
        continue
    fi
    if ! hf_validate_permutation "$rust_out" "$permutation_field" "$permutation_values" ||
        ! hf_validate_permutation "$cpp_out" "$permutation_field" "$permutation_values"; then
        echo "[FAIL] $name: '$permutation_field' is not the required permutation" >&2
        failed=$((failed + 1))
        continue
    fi

    case "$compare" in
        byte)
            if cmp -s "$rust_out" "$cpp_out"; then
                echo "[PASS byte] $name"
                passed=$((passed + 1))
            else
                echo "[FAIL byte] $name" >&2
                diff -u <(jq -S . "$cpp_out") <(jq -S . "$rust_out") >&2 || true
                failed=$((failed + 1))
            fi
            ;;
        normalized)
            not_byte_comparable=$((not_byte_comparable + 1))
            echo "[INFO] $name is not byte-comparable: $reason"
            rust_compare="$scratch/${line_number}.rust.normalized.json"
            cpp_compare="$scratch/${line_number}.cpp.normalized.json"
            if ! hf_normalize_response "$rust_out" "$ignored" "$ignored_recursive" \
                    >"$rust_compare" ||
                ! hf_normalize_response "$cpp_out" "$ignored" "$ignored_recursive" \
                    >"$cpp_compare"; then
                echo "[FAIL normalized] $name: response normalization failed" >&2
                failed=$((failed + 1))
            elif cmp -s "$rust_compare" "$cpp_compare"; then
                echo "[PASS normalized] $name"
                passed=$((passed + 1))
            else
                echo "[FAIL normalized] $name" >&2
                diff -u <(jq . "$cpp_compare") <(jq . "$rust_compare") >&2 || true
                failed=$((failed + 1))
            fi
            ;;
        semantic)
            not_byte_comparable=$((not_byte_comparable + 1))
            echo "[INFO] $name uses semantic Atom comparison: $reason"
            rust_compare="$scratch/${line_number}.rust.semantic.json"
            cpp_compare="$scratch/${line_number}.cpp.semantic.json"
            if ! hf_semantic_response "$rust_out" "$ignored" "$ignored_recursive" \
                    "$semantic_fields" "$semantic_variables" "$rust_bin" \
                    >"$rust_compare" ||
                ! hf_semantic_response "$cpp_out" "$ignored" "$ignored_recursive" \
                    "$semantic_fields" "$semantic_variables" "$rust_bin" \
                    >"$cpp_compare"; then
                echo "[FAIL semantic] $name: Atom canonicalization failed" >&2
                failed=$((failed + 1))
            elif cmp -s "$rust_compare" "$cpp_compare"; then
                echo "[PASS semantic] $name"
                passed=$((passed + 1))
            else
                echo "[FAIL semantic] $name" >&2
                diff -u <(jq . "$cpp_compare") <(jq . "$rust_compare") >&2 || true
                failed=$((failed + 1))
            fi
            ;;
        *)
            echo "[FAIL] $name: unknown comparison mode '$compare'" >&2
            failed=$((failed + 1))
            ;;
    esac
done <"$fixtures"

echo "differential summary: passed=$passed failed=$failed normalized_only=$not_byte_comparable"
((failed == 0))

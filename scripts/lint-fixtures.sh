#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
differential=${DIFFERENTIAL_FIXTURES:-${1:-"$repo_root/tests/fixtures/differential.jsonl"}}
benchmark=${BENCH_WORKLOADS:-${2:-"$repo_root/tests/fixtures/benchmark.jsonl"}}

for tool in jq sort uniq mktemp; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "lint-fixtures: required tool '$tool' is unavailable" >&2
        exit 2
    }
done
for fixture_file in "$differential" "$benchmark"; do
    [[ -f "$fixture_file" ]] || {
        echo "lint-fixtures: fixture file not found: $fixture_file" >&2
        exit 2
    }
done

scratch=$(mktemp -d "${TMPDIR:-/tmp}/hyperbolica-fixtures.XXXXXXXX")
cleanup() {
    rm -rf -- "$scratch"
}
trap cleanup EXIT

failures=0

report_failure() {
    local file=$1
    local line=$2
    local message=$3
    printf '%s:%s: %s\n' "$file" "$line" "$message" >&2
    failures=$((failures + 1))
}

lint_common() {
    local file=$1
    local kind=$2
    local names_file="$scratch/$kind.names"
    local requests_file="$scratch/$kind.requests"
    local count=0
    local line_number=0
    local line canonical name request

    : >"$names_file"
    : >"$requests_file"
    while IFS= read -r line || [[ -n "$line" ]]; do
        line_number=$((line_number + 1))
        if [[ -z "$line" ]]; then
            report_failure "$file" "$line_number" "blank lines are not valid JSONL fixtures"
            continue
        fi
        if ! jq -e . >/dev/null 2>&1 <<<"$line"; then
            report_failure "$file" "$line_number" "invalid JSON"
            continue
        fi
        canonical=$(jq -c . <<<"$line")
        if [[ "$line" != "$canonical" ]]; then
            report_failure "$file" "$line_number" \
                "fixture must be canonical compact JSON (run: jq -c .)"
        fi
        if ! jq -e '
            type == "object"
            and (.name | type == "string" and test("^[A-Za-z0-9][A-Za-z0-9_.-]*$"))
            and (.request | type == "object")
            and (.request.op | type == "string" and length > 0)
            and ((.request.schema_version_min? // 1) |
                type == "number" and floor == . and . >= 1 and . <= 2)
        ' >/dev/null <<<"$line"; then
            report_failure "$file" "$line_number" \
                "name/request/op/schema_version_min violates the common fixture schema"
            continue
        fi

        name=$(jq -r '.name' <<<"$line")
        request=$(jq -S -c '.request' <<<"$line")
        printf '%s\n' "$name" >>"$names_file"
        printf '%s\n' "$request" >>"$requests_file"
        count=$((count + 1))
    done <"$file"

    if ((count == 0)); then
        report_failure "$file" 0 "fixture corpus is empty"
    fi
    while IFS= read -r duplicate; do
        [[ -z "$duplicate" ]] || report_failure "$file" 0 "duplicate fixture name: $duplicate"
    done < <(sort "$names_file" | uniq -d)
    while IFS= read -r duplicate; do
        [[ -z "$duplicate" ]] || report_failure "$file" 0 "duplicate canonical request: $duplicate"
    done < <(sort "$requests_file" | uniq -d)
}

lint_differential() {
    local file=$1
    local line_number=0
    local line

    lint_common "$file" differential
    while IFS= read -r line || [[ -n "$line" ]]; do
        line_number=$((line_number + 1))
        [[ -n "$line" ]] || continue
        jq -e . >/dev/null 2>&1 <<<"$line" || continue
        if ! jq -e '
            def allowed_keys:
                ["name", "compare", "request", "ignore", "reason",
                 "ignore_recursive", "permutation_field", "permutation_values"];
            def strings_unique:
                type == "array" and all(.[]; type == "string" and length > 0)
                and (length == (unique | length));
            def allowed_ignored:
                if .request.op == "find_lr_orders" then
                    ["hf_version", "timing_compute_s", "score", "best_order"]
                elif .request.op == "apply_mzv_reductions" then ["vars"]
                elif .request.op == "integration_step" then ["vars"]
                elif .request.op == "hyperflint" then
                    ["timing_compute_s", "vars", "algebraic_letters"]
                else [] end;
            def allowed_recursive:
                if .request.op == "factor_table" then
                    ["t_build_s", "trial_s", "fallback_s"]
                else [] end;
            ((keys_unsorted - allowed_keys) | length == 0)
            and (.compare == "byte" or .compare == "normalized")
            and (
                if .compare == "byte" then
                    (has("ignore") | not)
                    and (has("ignore_recursive") | not)
                    and (has("reason") | not)
                    and (has("permutation_field") | not)
                    and (has("permutation_values") | not)
                else
                    (.ignore | strings_unique and length > 0)
                    and ((.ignore_recursive // []) | strings_unique)
                    and (.reason | type == "string" and length > 0)
                    and ([.ignore[] as $key | allowed_ignored | index($key)] |
                         all(.[]; . != null))
                    and ([((.ignore_recursive // [])[]) as $key |
                          allowed_recursive | index($key)] | all(.[]; . != null))
                    and ((has("permutation_field") and has("permutation_values"))
                         or ((has("permutation_field") | not)
                             and (has("permutation_values") | not)))
                    and (
                        if has("permutation_field") then
                            (.permutation_field | type == "string" and length > 0)
                            and (.permutation_values | strings_unique and length > 0)
                            and (.permutation_field as $field |
                                 .ignore | index($field) != null)
                            and ((.permutation_values | sort) == (.request.xvars | sort))
                        else true end
                    )
                end
            )
        ' >/dev/null <<<"$line"; then
            report_failure "$file" "$line_number" \
                "comparison policy violates differential.schema.json or the ignore-field allowlist"
        fi
    done <"$file"
}

lint_benchmark() {
    local file=$1
    local line_number=0
    local line

    lint_common "$file" benchmark
    while IFS= read -r line || [[ -n "$line" ]]; do
        line_number=$((line_number + 1))
        [[ -n "$line" ]] || continue
        jq -e . >/dev/null 2>&1 <<<"$line" || continue
        if ! jq -e '
            def allowed_keys:
                ["name", "compare", "request", "ignore", "ignore_recursive",
                 "reason", "permutation_field", "permutation_values", "tier",
                 "timeout_seconds", "pairs"];
            def strings_unique:
                type == "array" and all(.[]; type == "string" and length > 0)
                and (length == (unique | length));
            def allowed_ignored:
                if .request.op == "find_lr_orders" then
                    ["hf_version", "timing_compute_s", "score", "best_order"]
                elif .request.op == "find_lr_orders_scan" then
                    ["hf_version", "timing_compute_s"]
                elif .request.op == "factor_table" then ["hf_version"]
                elif .request.op == "apply_mzv_reductions" then ["vars"]
                elif .request.op == "integration_step" then ["vars"]
                elif .request.op == "hyperflint" then
                    ["timing_compute_s", "vars", "algebraic_letters"]
                else [] end;
            def allowed_recursive:
                if .request.op == "factor_table" then
                    ["t_build_s", "trial_s", "fallback_s"]
                else [] end;
            ((keys_unsorted - allowed_keys) | length == 0)
            and ((.tier // "qualification") |
                 . == "qualification" or . == "nightly" or
                 . == "heavyweight" or . == "exploratory")
            and ((.timeout_seconds // 1) |
                 type == "number" and floor == . and . >= 1)
            and ((.pairs // 1) | type == "number" and floor == . and . >= 1)
            and ((.compare // "byte") == "byte" or .compare == "normalized")
            and (
                if (.compare // "byte") == "byte" then
                    (has("ignore") | not)
                    and (has("ignore_recursive") | not)
                    and (has("reason") | not)
                    and (has("permutation_field") | not)
                    and (has("permutation_values") | not)
                else
                    (.ignore | strings_unique and length > 0)
                    and ((.ignore_recursive // []) | strings_unique)
                    and (.reason | type == "string" and length > 0)
                    and ([.ignore[] as $key | allowed_ignored | index($key)] |
                         all(.[]; . != null))
                    and ([((.ignore_recursive // [])[]) as $key |
                          allowed_recursive | index($key)] | all(.[]; . != null))
                    and ((has("permutation_field") and has("permutation_values"))
                         or ((has("permutation_field") | not)
                             and (has("permutation_values") | not)))
                    and (
                        if has("permutation_field") then
                            (.permutation_field | type == "string" and length > 0)
                            and (.permutation_values | strings_unique and length > 0)
                            and (.permutation_field as $field |
                                 .ignore | index($field) != null)
                            and ((.permutation_values | sort) == (.request.xvars | sort))
                        else true end
                    )
                end
            )
        ' \
            >/dev/null <<<"$line"; then
            report_failure "$file" "$line_number" \
                "comparison policy violates benchmark.schema.json or the ignore-field allowlist"
        fi
    done <"$file"
}

lint_differential "$differential"
lint_benchmark "$benchmark"

if ((failures > 0)); then
    echo "fixture lint: FAIL ($failures violation(s))" >&2
    exit 1
fi
echo "fixture lint: PASS (differential and benchmark JSONL schemas)"

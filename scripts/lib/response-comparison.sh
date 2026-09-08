#!/usr/bin/env bash

# Shared, fixture-controlled response comparison for differential and benchmark
# harnesses. Callers retain `set -euo pipefail`; this file deliberately changes
# no shell options.

hf_response_filter=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)/normalize-response.jq

hf_normalize_response() {
    local input=$1
    local ignored=$2
    local ignored_recursive=$3
    jq -S -c \
        --argjson ignored "$ignored" \
        --argjson ignored_recursive "$ignored_recursive" \
        -f "$hf_response_filter" \
        "$input"
}

hf_validate_permutation() {
    local input=$1
    local field=$2
    local expected=$3
    [[ -z "$field" ]] && return 0
    cmp -s \
        <(jq -c --arg field "$field" '.[$field] | sort' "$input") \
        <(jq -c 'sort' <<<"$expected")
}

hf_canonical_response() {
    local input=$1
    local compare=$2
    local ignored=$3
    local ignored_recursive=$4
    case "$compare" in
        byte)
            # Preserve exact transport bytes in differential mode. Benchmark
            # callers can still compare this captured value deterministically.
            cat -- "$input"
            ;;
        normalized | semantic)
            hf_normalize_response "$input" "$ignored" "$ignored_recursive"
            ;;
        *)
            printf 'unknown response comparison mode: %s\n' "$compare" >&2
            return 2
            ;;
    esac
}

# Canonicalize only fixture-declared algebraic string fields with Hyperbolica's
# own Symbolica-backed parser. The remaining response envelope is normalized by
# the same narrow ignore policy used for ordinary normalized comparisons.
#
# This function is intended for the cross-backend preflight only. Timed runs
# should be compared with each backend's captured transport response so parser
# startup cannot perturb an adjacent measurement pair.
hf_semantic_response() {
    local input=$1
    local ignored=$2
    local ignored_recursive=$3
    local fields=$4
    local variables=$5
    local parser=$6
    local temporary working next parsed field count index status=0

    [[ -x "$parser" ]] || {
        printf 'semantic response parser is not executable: %s\n' "$parser" >&2
        return 2
    }
    jq -e 'type == "array" and length > 0 and all(.[]; type == "string" and length > 0)' \
        <<<"$fields" >/dev/null || {
        printf 'semantic response fields must be a non-empty string array\n' >&2
        return 2
    }
    jq -e 'type == "array" and length > 0 and all(.[]; type == "string" and length > 0)' \
        <<<"$variables" >/dev/null || {
        printf 'semantic response variables must be a non-empty string array\n' >&2
        return 2
    }

    temporary=$(mktemp -d "${TMPDIR:-/tmp}/hyperbolica-semantic.XXXXXXXX") || return
    working="$temporary/response.json"
    next="$temporary/next.json"
    parsed="$temporary/parsed.json"
    if ! hf_normalize_response "$input" "$ignored" "$ignored_recursive" >"$working"; then
        status=1
    fi

    if ((status == 0)); then
        while IFS= read -r field; do
            if [[ "$field" == "result[].coef" ]]; then
                if ! count=$(jq -er '
                    .result |
                    if type == "array" and all(.[]; .coef | type == "string")
                    then length
                    else error("result is not an array of coefficient objects")
                    end
                ' "$working"); then
                    printf "semantic response field '%s' is absent or malformed\n" "$field" >&2
                    status=1
                    break
                fi
                for ((index = 0; index < count; index++)); do
                    if ! jq -c --argjson index "$index" --argjson vars "$variables" \
                        '{op:"parse_expr",expr:.result[$index].coef,vars:$vars}' \
                        "$working" |
                        env SYMBOLICA_HIDE_BANNER=1 OMP_NUM_THREADS=1 \
                            OPENBLAS_NUM_THREADS=1 RAYON_NUM_THREADS=1 \
                            "$parser" eval-json >"$parsed"; then
                        printf "semantic parser failed for response field '%s[%d]'\n" \
                            "$field" "$index" >&2
                        status=1
                        break
                    fi
                    if ! jq -e 'type == "object" and .op == "parse_expr" and
                        (.canonical | type == "string") and (has("error") | not)' \
                        "$parsed" >/dev/null; then
                        printf "semantic parser returned an invalid response for field '%s[%d]'\n" \
                            "$field" "$index" >&2
                        status=1
                        break
                    fi
                    if ! jq -S -c --argjson index "$index" --slurpfile parsed "$parsed" \
                        '.result[$index].coef = $parsed[0].canonical' \
                        "$working" >"$next" || ! mv -- "$next" "$working"; then
                        status=1
                        break
                    fi
                done
                ((status == 0)) || break
                continue
            fi
            if ! jq -e --arg field "$field" \
                'has($field) and (.[$field] | type == "string")' "$working" >/dev/null; then
                printf "semantic response field '%s' is absent or is not a string\n" "$field" >&2
                status=1
                break
            fi
            if ! jq -c --arg field "$field" --argjson vars "$variables" \
                '{op:"parse_expr",expr:.[$field],vars:$vars}' "$working" |
                env SYMBOLICA_HIDE_BANNER=1 OMP_NUM_THREADS=1 \
                    OPENBLAS_NUM_THREADS=1 RAYON_NUM_THREADS=1 \
                    "$parser" eval-json >"$parsed"; then
                printf "semantic parser failed for response field '%s'\n" "$field" >&2
                status=1
                break
            fi
            if ! jq -e 'type == "object" and .op == "parse_expr" and
                (.canonical | type == "string") and (has("error") | not)' \
                "$parsed" >/dev/null; then
                printf "semantic parser returned an invalid response for field '%s'\n" "$field" >&2
                status=1
                break
            fi
            if ! jq -S -c --arg field "$field" --slurpfile parsed "$parsed" \
                '.[$field] = $parsed[0].canonical' "$working" >"$next"; then
                status=1
                break
            fi
            if ! mv -- "$next" "$working"; then
                status=1
                break
            fi
        done < <(jq -r '.[]' <<<"$fields")
    fi

    if ((status == 0)); then
        if ! cat -- "$working"; then
            status=1
        fi
    fi
    rm -rf -- "$temporary"
    return "$status"
}

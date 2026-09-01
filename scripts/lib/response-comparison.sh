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
        normalized)
            hf_normalize_response "$input" "$ignored" "$ignored_recursive"
            ;;
        *)
            printf 'unknown response comparison mode: %s\n' "$compare" >&2
            return 2
            ;;
    esac
}

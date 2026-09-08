#!/usr/bin/env bash
set -euo pipefail

script_directory=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# Keep exploratory matrix comparisons on the same audited implementation as
# the locked harness. The caller bounds this entire process group.
source "$script_directory/lib/response-comparison.sh"
input=$1
fixture=$2
parser=$3
mode=$(jq -r '.compare // "byte"' "$fixture")
ignored=$(jq -c '.ignore // []' "$fixture")
ignored_recursive=$(jq -c '.ignore_recursive // []' "$fixture")
field=$(jq -r '.permutation_field // ""' "$fixture")
values=$(jq -c '.permutation_values // []' "$fixture")
hf_validate_permutation "$input" "$field" "$values"
if [[ "$mode" == semantic ]]; then
    fields=$(jq -c '.semantic_fields' "$fixture")
    variables=$(jq -c '.semantic_variables' "$fixture")
    hf_semantic_response "$input" "$ignored" "$ignored_recursive" \
        "$fields" "$variables" "$parser"
else
    hf_canonical_response "$input" "$mode" "$ignored" "$ignored_recursive"
fi

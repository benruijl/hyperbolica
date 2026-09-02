#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cargo_bin=${CARGO:-cargo}
cd "$repo_root"

scratch=$(mktemp -d "${TMPDIR:-/tmp}/hyperbolica-examples.XXXXXXXX")
cleanup() {
    rm -rf -- "$scratch"
}
trap cleanup EXIT

"$cargo_bin" build --locked --examples

run_example() {
    local name=$1
    shift
    # Let Cargo resolve the executable it just built. This remains correct for
    # custom target directories, configured target triples, and target runners
    # instead of guessing a host-only target/debug/examples path.
    if ! SYMBOLICA_HIDE_BANNER=1 "$cargo_bin" run --locked --quiet \
        --example "$name" -- "$@" \
        >"$scratch/$name.stdout" 2>"$scratch/$name.stderr"; then
        printf 'check-examples: %s failed\n' "$name" >&2
        sed -n '1,160p' "$scratch/$name.stdout" >&2
        sed -n '1,160p' "$scratch/$name.stderr" >&2
        return 1
    fi
}

run_example atom_hlog
run_example atom_integration
run_example atom_intervals
run_example core_algebra
run_example euler_filter
run_example integration_pipeline
run_example linear_reducibility
run_example resultant_strategies auto locked

request='{"op":"derivative","a":"x^3+y*x","var":"x","vars":["x","y"]}'
if ! printf '%s\n' "$request" | SYMBOLICA_HIDE_BANNER=1 \
    "$cargo_bin" run --locked --quiet --example profile_json -- 1 \
    >"$scratch/profile_json.stdout" 2>"$scratch/profile_json.stderr"; then
    printf 'check-examples: profile_json failed\n' >&2
    sed -n '1,160p' "$scratch/profile_json.stdout" >&2
    sed -n '1,160p' "$scratch/profile_json.stderr" >&2
    exit 1
fi

rg -Fx '1' "$scratch/atom_integration.stdout" >/dev/null
rg -F 'integral of 1 from 2 to 5 = 3' "$scratch/atom_intervals.stdout" >/dev/null
rg -F 'resultant in x: 2*y^2 + 1' "$scratch/core_algebra.stdout" >/dev/null
rg -F 'projective: true' "$scratch/linear_reducibility.stdout" >/dev/null
rg -F 'locked/Auto:' "$scratch/resultant_strategies.stdout" >/dev/null
rg -F '"result":"3*x^2 + y"' "$scratch/profile_json.stdout" >/dev/null

printf 'Example execution gate: all 9 public examples PASS\n'

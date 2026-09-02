#!/usr/bin/env bash
set -euo pipefail

# Symbolica's restricted mode ties an instance to its first calling thread.
# Rust's default test harness uses a fresh worker thread for successive tests,
# so run one exact test per process while keeping the whole sequence serial.
unset SYMBOLICA_LICENSE SYMBOLICA_LICENSE_SERVER
export SYMBOLICA_HIDE_BANNER=1
# The compatibility bridge turns even explicitly parallel full-integration
# requests into their serial form. Direct integration also consults
# LicenseManager before entering its sole Rayon branch, so restricted mode
# never transfers Symbolica work away from the calling test thread.
export HF_MAX_THREADS_PER_CALL=1
cargo_bin=${CARGO:-cargo}

run_each_test() {
  local -a cargo_target=("$@")
  local -a tests
  local listed
  listed=$(
    "$cargo_bin" test --locked "${cargo_target[@]}" -- --list 2>/dev/null |
      sed -n 's/: test$//p'
  )
  tests=()
  if [[ -n "$listed" ]]; then
    mapfile -t tests <<<"$listed"
  fi
  if ((${#tests[@]} == 0)); then
    printf 'test-unlicensed: no tests discovered for cargo target:' >&2
    printf ' %q' "${cargo_target[@]}" >&2
    printf '\n' >&2
    return 1
  fi
  for test_name in "${tests[@]}"; do
    "$cargo_bin" test --locked "${cargo_target[@]}" --quiet "$test_name" -- \
      --exact --test-threads=1
  done
}

run_each_test --lib

# Every integration test also gets its own process. `--test-threads=1` alone
# does not promise that successive tests reuse the same OS thread, which is
# the restricted Symbolica instance's requirement.
integration_target_list=$(
  rg --files tests -g '*.rs' |
    awk -F/ 'NF == 2 { sub(/\.rs$/, "", $2); print $2 }' |
    sort
)
[[ -n "$integration_target_list" ]] || {
  echo 'test-unlicensed: no top-level integration targets discovered' >&2
  exit 1
}
mapfile -t integration_targets <<<"$integration_target_list"
for target in "${integration_targets[@]}"; do
  run_each_test --test "$target"
done

printf 'Restricted-mode gate: every Rust test passed in an isolated serial process\n'

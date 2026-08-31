#!/usr/bin/env bash
set -euo pipefail

# Symbolica's restricted mode ties an instance to its first calling thread.
# Rust's default test harness uses a fresh worker thread for successive tests,
# so run one exact test per process while keeping the whole sequence serial.
export SYMBOLICA_HIDE_BANNER=1

mapfile -t tests < <(
  cargo test --lib -- --list 2>/dev/null \
    | sed -n 's/: test$//p'
)

for test_name in "${tests[@]}"; do
  cargo test --lib --quiet "$test_name" -- --exact --test-threads=1
done

# Atom-native coverage is deliberately one serial test so every case shares
# the same restricted-mode calling thread.
cargo test --test atom_api --quiet -- --test-threads=1

# The JSON/CLI compatibility suite is a single serial test that launches each
# CLI request in its own process, exercising the same restricted-mode boundary.
cargo test --test json_cli_compat --quiet -- --test-threads=1

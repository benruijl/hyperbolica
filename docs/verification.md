# Verification and performance evidence

Hyperbolica separates CAS-independent gates from checks that acquire a
Symbolica runtime. This keeps ABI and fixture regressions visible in ordinary
CI while making it impossible to mistake a successful compile for mathematical
or performance evidence.

## CAS-independent gates

- `scripts/lint-fixtures.sh` validates both JSONL corpora before a binary is
  built or run. The checked-in JSON Schemas document the formats; the executable
  lint additionally enforces unique canonical requests and an operation-specific
  allowlist of normalization fields.
- `cargo test --test c_abi_contract` exercises NULL input, malformed UTF-8 and
  JSON, caller-owned allocation, the NULL-safe deallocator, and the borrowed
  four-component version pointer. None of these requests reaches CAS dispatch.
- `scripts/check-abi.sh` enforces exact equality between the shared-library
  `hf_*` exports, the public header, and `tests/abi/symbols_golden.txt`. It also
  compiles C and C++ consumers, links a C harness, and by default runs only its
  NULL-input transport/ownership paths. Set `ABI_RUN_SMOKE=0` when cross
  compiling.

The pinned C++ baseline has a historical mismatch: its current `c_abi.h` and
implementation contain `hf_factor_table` and `hf_find_lr_orders_scan`, while
its older containment-style golden file lists only six symbols. Hyperbolica
uses the current header/implementation contract requested for this port: six
operations plus `hf_free_string` and `hf_version_string`, exactly eight public
`hf_*` exports. Extra exports fail the gate as readily as missing exports.

## Differential and benchmark runs

Both runtime scripts lint fixtures first. `scripts/differential.sh` then runs
the Rust binary and the independently built C++ executable; no C++ or FLINT code
is linked into the Rust crate.

`scripts/benchmark-compare.sh` refuses to time a workload unless the two
backends first return canonically equal successful JSON. Every warmup and timed
response must remain equal to that preflight response. Each run writes:

- `summary.csv`, retaining the original five-column CSV contract;
- `samples.csv`, one raw nanosecond sample per backend and iteration; and
- `metadata.json`, including UTC timestamps, host/CPU details, affinity,
  threads, thresholds, commands, tool versions, source revisions, binary and
  fixture SHA-256 hashes, and final status.

By default evidence goes under `target/benchmark-results/<run-id>/`. Existing
`RESULTS_FILE` users keep the same summary CSV format; sidecars go to
`$RESULTS_FILE.evidence` unless `EVIDENCE_DIR` is supplied. `RUST_REVISION` and
`CPP_REVISION` can attribute binaries built outside their default source trees.

A benchmark run is valid evidence only when its metadata status is `passed`.
`passed_with_regressions` means the gate was explicitly configured not to fail;
`regression` or `failed` is not positive performance evidence.

Runtime differential tests and timings require an available Symbolica license
instance on its configured singleton port. They must not be run concurrently
with another restricted Symbolica process, and results must never be inferred
from compile-only checks.

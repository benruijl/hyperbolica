# Upstream portable-test matrix

This inventory compares the public test tree from
`SubTropica/SubTropica/HyperFLINT` with this pure-Symbolica Rust port. It was
audited file-by-file against the upstream checkout at
`/tmp/hyperflint-upstream/HyperFLINT/test` on 2026-08-31. No C++ implementation
body was copied. Rust assertions below were independently expressed from the
observable mathematical, protocol, and determinism contracts and small public
inputs.

Status meanings:

- **Covered**: the same portable observable contract has a direct Rust test.
- **Replaced**: the observable goal is tested, but the FLINT/OpenMP-specific
  mechanism does not exist in the Symbolica/Rayon architecture.
- **Not applicable**: an allocator, probe, deprecated foreign binding, or
  implementation toggle has no corresponding production feature to test.

Runtime-heavy tests compile in ordinary CI. Restricted Symbolica installations
run them serially through `scripts/test-unlicensed.sh`; the external C++ oracle
remains confined to `scripts/differential.sh` and
`scripts/benchmark-compare.sh`.

## Mathematical and integration tests

| Upstream test | Status | Rust evidence |
| --- | --- | --- |
| `test/test_euler_chi.cpp` | Covered | `src/algebra/euler/staircase.rs::upstream_staircase_fixtures`; `src/algebra/euler/system.rs::{box_constraint_oracles,par_generic_zero_nonlinear_constraint_oracle}` |
| `test/test_euler_filter.cpp` | Covered | `src/algebra/euler/filter.rs::{lee_pomeransky_box_verdicts_and_cache,homogeneous_split_pair_auto_chart_matches_box_verdicts,uq5_intermediate_fixture}` |
| `test/test_factored_rat.cpp` | Covered | `src/core/factored_rat/tests.rs::{arithmetic_and_derivative_match_materialized_oracle,deterministic_randomized_operations_are_value_equivalent}` |
| `test/test_lf_perfpow.cpp` | Replaced | Symbolica factorization is the only kernel; reconstruction and quadratic classification are pinned in `src/algebra/linear_factors.rs` tests. |
| `test/test_lr_scan.cpp` | Covered | Unit cases in `src/integrator/lr_scan/tests.rs`; exact UQ5 strict/120-order/gauge/collaborator oracle in `tests/upstream_portable.rs`. |
| `test/test_partial_fractions_characterization.cpp` | Covered | `src/algebra/partial_fractions.rs` reconstruction tests plus the two parameter-dependent, non-unit pole systems in `tests/upstream_portable.rs`. |
| `test/test_partial_fractions_factored_den.cpp` | Covered | Repeated/scaled poles and quadratic-plus-linear factored denominators in `src/algebra/partial_fractions.rs`. |
| `test/test_rat_add_equivalence.cpp` | Replaced | There is no FLINT dual-backend dispatch. Exact Symbolica normalization and arithmetic oracles are in `src/core/rat/tests.rs`; content-path equivalence is in `tests/upstream_portable.rs`. |
| `test/test_univar_rat.cpp` | Replaced | No parallel `UnivarRat` CAS exists. Symbolica's public univariate projection is exercised through `src/core/poly/tests.rs`, `src/algebra/linear_factors.rs`, and `src/algebra/partial_fractions.rs`. |
| `test/unit/test_factor_table.cpp` | Covered | Existing pair/guard/dedup cases in `src/integrator/factor_table.rs`; all-object exactness, OOP singleton, and zero-difference cases in `tests/upstream_portable.rs`. |
| `test/unit/test_find_lr_orders_carry_discharge.cpp` | Covered | Default-off, strict/carry verdict flip, exact carried count/order, uncarried-first selection, and cubic rejection in `tests/json_cli_compat.rs`; exhaustive carry scan in `tests/upstream_portable.rs`. |
| `test/unit/test_find_lr_orders_strategy_roundtrip.cpp` | Covered | All six public strategy rows cross the real JSON/CLI boundary in `tests/json_cli_compat.rs`. |
| `test/unit/test_find_lr_orders_strategy_roundtrip_librarylink.cpp` | Not applicable | Wolfram LibraryLink is not a Rust production surface. The supported C ABI and CLI share the same bridge and are covered below. |
| `test/unit/test_mzv_expansion.cpp` | Covered | Chained/adversarial rules in `src/reduce/mzv_expansion.rs`; production 700-rule shape, divergent zeros, and six Euler/Zagier identities in `tests/upstream_portable.rs`. |
| `test/unit/test_parse_unary_precedence.cpp` | Covered | `src/convert/parse.rs::{power_binds_tighter_than_unary_minus,powers_are_right_associative}` and `tests/fixtures/differential.jsonl`. |
| `test/unit/test_period_tuples.cpp` | Covered | `src/core/period_table.rs`, `src/core/symcoef/tests.rs`, and `src/reduce/periods/tests.rs` cover dense IDs, cancellation, canonical collection, minting, and residual-period rejection. |
| `test/unit/test_rat_content_invariance.cpp` | Covered | Native content/cancellation tests in `src/core/rat/tests.rs`; the shared-content 12-variable construction-path case in `tests/upstream_portable.rs`. Upstream's legacy-vs-repswap toggle itself is absent. |
| `test/unit/test_rat_split_roundtrip.cpp` | Covered | `src/core/rat_split.rs` plus W-only denominator, mixed denominator, and high-W-exponent cases in `tests/upstream_portable.rs`. |
| `test/unit/test_rat_split_roundtrip_synthetic.cpp` | Covered | Rat-level adversarial shapes are in `tests/upstream_portable.rs`; symbolic-power and split arithmetic invariants are in `src/core/sym_coef_split.rs`. |
| `test/unit/test_step_strategy_dispatch.cpp` | Covered | Independent six-row JSON assertions in `tests/json_cli_compat.rs`; production implementation does not read the upstream truth table. |
| `test/unit/test_sym_coef_canonical_string.cpp` | Covered | `src/core/symcoef/tests.rs::{canonical_collection_and_merge_add_sub,formatting_is_deterministic_and_matches_hyperflint}` and structural collision tests in `src/symbols/word.rs`. |
| `test/unit/test_sym_coef_split_roundtrip.cpp` | Covered | `src/core/sym_coef_split.rs` round-trip, addition, multiplication, `i`, delta, and symbolic-power tests. |
| `test/unit/test_verify_order_multigroup.cpp` | Covered | `src/integrator/lr_verify.rs::multigroup_intersection_drops_the_e26_spurious_quadratic` and JSON diagnostics in `tests/json_cli_compat.rs`. |
| `test/unit/test_zw_table_basic.cpp` | Covered | `src/core/zw_table.rs` covers sentinel values, memoized arithmetic, merge, and full equality under a forced digest collision. |

## ABI, protocol, and determinism

| Upstream test | Status | Rust evidence |
| --- | --- | --- |
| `test/abi/check_abi_symbols.sh` | Covered | `scripts/check-abi.sh`, `tests/abi/symbols_golden.txt`, C link/ownership smoke, and C++ header syntax smoke. |
| `test/unit/test_c_abi_find_lr_orders_cli_snapshot.cpp` | Replaced | Shared bridge dispatch in `src/c_abi.rs`; surface/ownership tests in `tests/c_abi_contract.rs`; live CLI schema in `tests/json_cli_compat.rs`. |
| `test/unit/test_c_abi_find_lr_orders_emit_sings_cli_snapshot.cpp` | Replaced | Same shared dispatch; singularity serialization and registered-atom spellings are pinned in `src/bridge/lr/tests.rs`. |
| `test/unit/test_c_abi_find_lr_orders_scan_cli_snapshot.cpp` | Replaced | Same shared dispatch; scan schema in `src/bridge/lr/search.rs` and scan mathematics in `tests/upstream_portable.rs`. |
| `test/unit/test_c_abi_hyperflint_sym_cli_snapshot.cpp` | Replaced | Same shared dispatch; public integration result schema and serial/parallel equality in `tests/json_cli_compat.rs`. |
| `test/unit/test_c_abi_linear_factors_cli_snapshot.cpp` | Replaced | Same shared dispatch; exact factor reconstruction in `src/algebra/linear_factors.rs` and protocol fixture in `tests/fixtures/differential.jsonl`. |
| `test/unit/test_c_abi_pfrac_cli_snapshot.cpp` | Replaced | Same shared dispatch; exact decomposition/reconstruction in `src/algebra/partial_fractions.rs` and CLI schema in `tests/json_cli_compat.rs`. |
| `test/unit/test_c_abi_pfrac.cpp` | Covered | Export presence, allocation ownership, error containment, and free semantics in `tests/c_abi_contract.rs`; valid PF payload in `tests/json_cli_compat.rs`. |
| `test/integration/test_omp_e2e_determinism.py` | Replaced | Rayon serial/parallel result equality in `tests/json_cli_compat.rs`; collision-safe deterministic collection tests in `src/integrator/structural_keys.rs` and transform child modules. |
| `test/unit/test_omp_determinism.cpp` | Replaced | No OpenMP interner exists. Full structural keys, forced-collision equality, and repeated deterministic ordering are tested in `src/core/structural_digest.rs`, `src/integrator/structural_keys.rs`, `src/symbols/word.rs`, and `src/integrator/lr_search/tests.rs`. |
| `test/unit/test_hyperflint_sym_response_determinism.cpp` | Covered | Serial/parallel public result equality in `tests/json_cli_compat.rs`; external repeated-process comparison remains available in `scripts/differential.sh`. |
| `test/test_linear_factors_cache_key_thread_invariant.cpp` | Replaced | Immutable structural projections and forced-collision full equality in `src/integrator/structural_keys.rs`; deterministic linear-factor reconstruction in `src/algebra/linear_factors.rs`. |
| `test/unit/test_construction_path_dedup_rate_omp_invariance.cpp` | Replaced | Structural context/poly/Rat equality and forced-collision regressions in `src/core/{context_interner.rs,canonical_signature.rs,structural_digest.rs}`. |
| `test/test_merge_sorted_canonical.cpp` | Covered | `src/core/symcoef/tests.rs::canonical_collection_and_merge_add_sub`; deterministic regulator collection in transform tests. |
| `test/scripts/g2a_strict_byte_identity.sh` | Replaced | Default-off and explicit-false carry semantics plus unchanged strategy/order envelope are asserted in `tests/json_cli_compat.rs`; timing bytes are intentionally dynamic. |
| `test/scripts/g2b_production_impact.sh` | Replaced | Portable performance/differential execution lives in `scripts/benchmark-compare.sh` and `scripts/differential.sh`; host-specific C++ optimization toggles are not production Rust features. |

## Public data corpus

| Upstream data | Status | Rust evidence |
| --- | --- | --- |
| `test/abi/symbols_golden.txt` | Covered | Port whitelist at `tests/abi/symbols_golden.txt`, enforced by `scripts/check-abi.sh`. |
| `test/Smirnov/tst0.txt` | Covered for ingestion | Byte-identical `tests/data/smirnov/tst0.txt`; parses with variables `t1`…`t5` and maximum log weight 0 in `tests/upstream_portable.rs`. |
| `test/Smirnov/tst1.txt` | Covered for ingestion | Byte-identical `tests/data/smirnov/tst1.txt`; parses with variables `t1`…`t5` and maximum log weight 1 in `tests/upstream_portable.rs`. |
| `test/Smirnov/tst2.txt` | Covered for ingestion | Byte-identical `tests/data/smirnov/tst2.txt`; parses with variables `t1`…`t5` and maximum log weight 2 in `tests/upstream_portable.rs`. |
| `test/Smirnov/tst3.txt` | Covered for ingestion | Byte-identical `tests/data/smirnov/tst3.txt`; parses with variables `t1`…`t5` and maximum log weight 3 in `tests/upstream_portable.rs`. |
| `test/Smirnov/tst4.txt` | Covered for ingestion | Byte-identical `tests/data/smirnov/tst4.txt`; parses with variables `t1`…`t5` and maximum log weight 4 in `tests/upstream_portable.rs`. Full execution remains a licensed performance job. |
| `test/Smirnov/diagnostics/tst0_step3_failure.json` | Not applicable | Historical failure diagnostic, not a mathematical expected-output fixture. |
| `test/data/mzv_basis_reference_values.json` | Covered | Checked-in at `tests/data/mzv_basis_reference_values.json`; six identities are asserted structurally in `tests/upstream_portable.rs`. |
| `test/data/mzv_reductions_chained_test.json` | Covered | Byte-identical `tests/data/mzv_reductions_chained_test.json`; default-flatness rejection and exact opt-in expansion are asserted in `tests/upstream_portable.rs`. |
| `test/data/mzv_reductions_chained_adversarial.json` | Covered | Byte-identical `tests/data/mzv_reductions_chained_adversarial.json`; default-flatness rejection and exact nested-square substitution are asserted in `tests/upstream_portable.rs`. |
| `test/data/step_strategy_truth_table.json` | Covered | Its six behavioral rows are independently encoded at the public JSON boundary in `tests/json_cli_compat.rs`. |

## Implementation-specific upstream tests

These files are inventoried explicitly so their omission cannot be mistaken for
an unnoticed gap. They test removed C++/FLINT/OpenMP/mimalloc telemetry or
cache machinery, rather than portable mathematics or public ABI behavior.

| Upstream test | Status | Reason/evidence |
| --- | --- | --- |
| `test/test_evict_compose_with_size_cap.cpp` | Not applicable | C++ operator-memo LRU/size-cap mechanism is absent. Rust caches are request-scoped typed structures. |
| `test/test_evict_dispatch_wiring.cpp` | Not applicable | FLINT cache-registry dispatch wiring is absent. |
| `test/test_gmp_slab.cpp` | Not applicable | GMP slab/mimalloc experiment; Symbolica owns coefficient allocation. |
| `test/test_integration_node_rss_4col.cpp` | Not applicable | C++ RSS probe schema, not a public result contract. |
| `test/test_integration_section_6d_dfs_cap.cpp` | Not applicable | Upstream experimental DFS-cap probe. Rust exposes loud LR time/operand guards in `src/integrator/lr_reduction.rs`, not that probe ABI. |
| `test/test_integration_structural_sharing_probe.cpp` | Not applicable | FLINT allocation-probe counters are absent. Structural sharing is encoded by `Arc`-backed native Rat values and tested in `src/core/rat/tests.rs`. |
| `test/test_mimalloc_per_arena.cpp` | Not applicable | Allocator-specific lifecycle experiment. |
| `test/test_operator_memo.cpp` | Not applicable | C++ global operator memo, counter replay, and env toggles are absent. Collision safety of actual Rust caches is tested directly. |
| `test/test_operator_memo_tsan.cpp` | Not applicable | TSan stress for the absent C++ global memo. |
| `test/test_op_memo_evict.cpp` | Not applicable | C++ LRU eviction policy is absent. |
| `test/test_rec1_mpz_pool_probe.cpp` | Not applicable | GMP pool instrumentation is absent. |
| `test/test_transform_zw_tab_readonly_under_cap.cpp` | Not applicable | Companion probe for the absent experimental DFS cap; ordinary ZW immutability/dedup is covered in `src/core/zw_table.rs`. |
| `test/smoke/test_integration_node_rss_smoke.sh` | Not applicable | RSS trace smoke for removed C++ instrumentation. |
| `test/smoke/test_step_trace_rss_smoke.sh` | Not applicable | RSS trace smoke for removed C++ instrumentation. |
| `test/unit/test_cache_registry.cpp` | Not applicable | Parser for a C++ cache-callsite registry, not a public API. |
| `test/unit/test_dag_hashcons_probe_init.cpp` | Not applicable | GMP DAG/hash-cons diagnostic probe is absent. |
| `test/unit/test_dag_hashcons_probe_ndjson_emit.cpp` | Not applicable | Diagnostic NDJSON emitter is absent. |
| `test/unit/test_dag_hashcons_probe_off_no_op.cpp` | Not applicable | Diagnostic probe toggle is absent. |
| `test/unit/test_env_flag_registry.cpp` | Not applicable | Registry of removed C++ experiment flags. Supported Rust limits are parsed at their typed call sites. |
| `test/unit/test_gcd_dispatch.cpp` | Not applicable | OpenMP/FLINT GCD dispatch implementation is absent; Symbolica is the sole GCD kernel. |
| `test/unit/test_integration_node_rss.cpp` | Not applicable | C++ RSS sampler unit. |
| `test/unit/test_operand_sparsity.cpp` | Not applicable | FLINT operand-sparsity telemetry emitter. |
| `test/unit/test_phase_timer.cpp` | Not applicable | C++ instrumentation helper; Rust public responses still type-check timing fields in `tests/json_cli_compat.rs`. |
| `test/unit/test_poles_bucket_flush.cpp` | Not applicable | C++ threshold-flush container. Rust collection equality/determinism is tested at the actual typed collectors. |
| `test/unit/test_pool_force_collect_omp_safety.cpp` | Not applicable | mimalloc/OpenMP heap-collection experiment. |
| `test/unit/test_sharded_flat_map.cpp` | Not applicable | Standalone C++ container. Rust digest buckets are covered with forced collisions in `src/core/structural_digest.rs`. |
| `test/unit/test_step_trace_rss.cpp` | Not applicable | RSS sample/trace schema helper. |

## Remaining boundary

The matrix closes portable fixture ingestion and mathematical/ABI contracts,
but it does not claim runtime performance from compile-only CI. Full Smirnov
integration, C++ differential parity, and wall/RSS comparisons must run on a
licensed benchmark host. `scripts/benchmark-compare.sh` is the authoritative
on-par-or-better gate; external C++ is never linked into production Rust.

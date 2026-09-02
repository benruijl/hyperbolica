# Fair `dev_poly` paired benchmark, 2026-09-01

This is the tracked, sanitized record of the final 12-pair exploratory run used
for the `dev_poly` port decision. It preserves the aggregate results and the
cryptographic identities of the ignored raw evidence under `target/` without
recording the Symbolica license value.

## Compared builds

- Rust: release LTO binary from the final working tree, SHA-256
  `9d826854503f3849c60369840b79ab1b50ebaf7587a382498d6e9fa06d570f65`.
- Symbolica: official `dev_poly` revision
  `76e3eb630abcc4d597463d759a0b40fedb57b764` plus
  the then-current, unretained `symbolica-dev_poly.patch` (contemporaneously
  recorded SHA-256
  `c1453111b3bbe8eb6a0dd1d7089266382f91add605b11b6f6b5872a5080afde5`).
  Those patch bytes were not archived, so the hash identifies the historical
  build but is not a reproducible patch artifact. The selected dependency has
  since moved to `dev`; this row remains a historical build identity rather
  than current-revision qualification.
- C++ oracle: clean HyperFLINT revision
  `adfd3af3be234cb43a2322bd9ec442caa26edd74`, binary SHA-256
  `8140b11d4628defa83301a1e37790b1af78a7bce1da3dc0d5a17ec17c723d8b1`.
- C++ configuration: `Release`, `release-portable`, `-O3 -DNDEBUG`, OpenMP
  on, mimalloc 3.3.2 static on, ASan/TSan off, CLI static dependencies off;
  GCC 15.3 and FLINT 3.6.0.
  Its CMake cache SHA-256 was
  `d259fbb6aee9ad56db424ef6965b734a397e9e9f5950d6b2fc170963575e1c19`.
- Host: AMD EPYC 9754, Linux 6.18.45, performance governor, CPU 0, one
  algebra thread.

The C++ configure, build, version check, and timed processes had
`SYMBOLICA_LICENSE` removed from their environments. The Rust timed processes
selectively inherited it. Evidence records only the booleans `true` for Rust
and `false` for the C++ oracle.

## Method and verdict

The run used the locked 14-workload corpus, 12 paired interleaved cold-process
samples per workload, two warmups, deterministic seed 1729, and 10,000
stratified paired bootstrap samples. All 14 correctness preflights agreed.

The global Rust/C++ geometric time ratio was `0.438906` with upper 95%
confidence bound `0.469063`: Rust was about 2.28x faster overall. Every
per-workload upper time bound was below the locked `1.15` limit. The only
locked numerical threshold that would fail was peak RSS for the dense
resultant: `1.5491` versus the locked `1.25` limit. That memory exception was
explicitly accepted for this change and was subsequently encoded as a narrow
`1.60` dense-resultant override in policy v3; all other workloads retain 1.25.

The run remains exploratory rather than formal qualification because it reused
externally built binaries from the dirty pre-commit Rust tree and relaxed the
RSS threshold for collection. It establishes the time comparison and the
allocator-matched RSS observation; it is not labelled a qualification pass.

| Workload | C++ median ms | Rust median ms | Rust/C++ geometric ratio | Upper 95% CI |
|---|---:|---:|---:|---:|
| convergent integration step | 24.371 | 4.061 | 0.2254 | 0.3578 |
| dense parameter resultant | 1991.652 | 324.404 | 0.1647 | 0.1737 |
| dense polynomial multiply | 9.379 | 3.382 | 0.4518 | 0.6171 |
| Euler-filtered massless box | 11.292 | 4.268 | 0.4192 | 0.4724 |
| factor-table multistage | 9.647 | 4.007 | 0.4528 | 0.5059 |
| high-order Laurent series | 12.536 | 7.062 | 0.6070 | 0.8322 |
| large-multiplicity partial fractions | 44.754 | 36.049 | 0.8607 | 0.9706 |
| LR verify, three variables | 11.310 | 4.819 | 0.5451 | 0.7248 |
| multivariate GCD | 10.101 | 3.660 | 0.3963 | 0.4623 |
| production MZV reduction | 16.510 | 6.216 | 0.4471 | 0.5340 |
| shared-denominator rational sum | 10.480 | 6.288 | 0.7826 | 1.0805 |
| sparse high-degree multiply | 9.408 | 3.415 | 0.4820 | 0.5918 |
| three-variable HyperFLINT | 20.681 | 4.322 | 0.2693 | 0.3605 |
| tiny rational addition | 9.244 | 3.591 | 0.6031 | 0.8327 |

For the dense resultant, peak RSS was 70,799,360 bytes for Rust and
45,703,168 bytes for the mimalloc-on C++ oracle.

## Raw-evidence identities

The ignored source directory was
`target/benchmark-results/exploratory-fair-pinned-adfd3af-mimalloc-12pair-20260901`.
Its evidence files had these SHA-256 identities:

| Artifact | SHA-256 |
|---|---|
| raw samples JSON | `3ba5669b8d33389e78882e135990d29ecdbebebea1b20e184a280848ebb74ce2` |
| raw samples CSV | `58fe0860d0aed17bca89a80f179398a628d242ad14bb7b88e65b2cfeaf34c7d4` |
| summary CSV | `e5727f7f837ac2ea272cdb00b9e52c46e703f76eaaa2f3d50b694664ff97e94a` |
| analysis input JSON | `657081c7730a63e3be0d29a3496f2ffcdb993bccd74e2ea26f4ddf3d42a8ce9c` |
| analysis JSON | `515ab4b23f5214d040ba5f10290d2249386c6d8c5fc4f08e5449ea571db102eb` |
| correctness JSON | `e5ee9d5a4efcac367c011e04fd8ef5286a80470eed32e8c0e50ae51455982479` |
| qualification JSON | `975505658b5d3134aa3689d2d824ffe17281c1ea73071f18934ed1361c603f0d` |
| metadata JSON | `e76cd5cfac2f1da5e68b0895f6e1604973a06143943316cb2bc460cc56b6a363` |
| Rust build manifest | `0acaca8cdc3fa7799913d6bccca35c69dfc464925a3e6b6da93367f0b68d5197` |
| C++ build manifest | `a23736b60ad012992247c49bd3f501f05709aa952708e93b27d7153de5f95b12` |

The locked workload fixture SHA-256 was
`241e7627f262f51b2b83fd8fbf76b749a1ffb211edc7ad2d062c6d95d9262a13`;
the corpus manifest SHA-256 was
`e0b3eb1ffc2a3d3bc3443f26473ed76581a87bd287837eb6e3aa1dfa4ce847d3`.

The qualification replay stored with this historical record intentionally
reports threshold/provenance deviations: the exploratory analysis was
collected with RSS relaxed to 100, the then-current policy applied 1.25, and
neither binary was built by the qualification driver. Its
`analysis_not_reproducible` entry reflects that deliberate threshold mismatch,
not corruption of the raw samples. Policy v3 does not retroactively qualify
these artifacts.

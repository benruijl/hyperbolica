# `dev_poly` performance profile

This record captures the 2026-09-01 profiling pass requested for the Rust port.
It is diagnostic evidence, not a qualification verdict; the locked paired
benchmark policy in `docs/verification.md` remains the publication gate.

## Provenance and method

- Host: AMD EPYC 9754, Linux 6.18.45, performance governor.
- Symbolica: live `origin/dev_poly` revision
  `76e3eb630abcc4d597463d759a0b40fedb57b764`.
- Build: release LTO, one codegen unit, debug symbols retained; Symbolica
  `faster_alloc`, GMP integer, and MPFR float features enabled.
- Sampling: `perf` userspace `cpu-clock` samples, processes pinned to CPU 0,
  one algebra thread, no lost samples. The tables report flat/self sample
  percentages so an inlined wrapper is not counted repeatedly.

The binary dependency file contains `vendor/symbolica/` and no
`vendor/symbolica-src/` path. The previously built `target/release/hyperflint`
was rejected before profiling because its dependency file proved that it still
used the archived snapshot.

The diagnostic sections use several successive binaries as the implementation
was narrowed. Their identities are retained rather than attributing every
number to the last executable:

| Evidence | Binary SHA-256 |
|---|---|
| direct-Q/CRT strategy probe, `target/profiles/resultant_strategies` | `918ef85f836a8510f4eaa5acb6b34b98b2e40049b9676ba7472051ee57c08543` |
| temporary three-way integer-Ducos probe | `00f8716b69dfa36354ee0bfa74441b50143f523a70494d018a62057e74374d2b` |
| initial CRT paired/profile binary | `80b2dcf94861f6f9ff1caae89abcdc755900db97484c29088edfaf847bf164c2` |
| final Auto paired binary, `target/profile-final/release/hyperflint` | `9d826854503f3849c60369840b79ab1b50ebaf7587a382498d6e9fa06d570f65` |
| pinned mimalloc-on HyperFLINT oracle | `8140b11d4628defa83301a1e37790b1af78a7bce1da3dc0d5a17ec17c723d8b1` |

The locked fixture file SHA-256 is
`241e7627f262f51b2b83fd8fbf76b749a1ffb211edc7ad2d062c6d95d9262a13`.
The final complete Symbolica source diff is
`vendor/symbolica-dev_poly.patch`, SHA-256
`569b52e78f5c109332898625381563d1ab1c6542e60f33fb1000f4597ee9e82b`.
The temporary probe source was replaced by the tracked
`examples/resultant_strategies.rs`, which exercises the same public paths.
The final paired aggregate results and all raw-artifact hashes are retained in
[`benchmark-evidence/dev-poly-fair-20260901.md`](benchmark-evidence/dev-poly-fair-20260901.md).

## Main findings

The two former tails had different causes:

| Workload | Before | After | Main change |
|---|---:|---:|---|
| large repeated-pole partial fractions | 2.85 s old Rust median | 0.04 s cold wall, 20 MB RSS | local Taylor/Cauchy recurrence over the already linear factors |
| dense parameter resultant | 10.87 s cold wall, 116 MB RSS | 0.413 s cold wall, 66.5 MB RSS | clear rational content once, then use integer Ducos with a CRT size guard |

The repeated-pole output is byte-identical to the C++ oracle. The dense
resultant output remains covered by semantic Atom comparison and by bounded
all-backend equality tests.

## Resultant strategy evidence

`examples/resultant_strategies.rs` times Symbolica's public strategies after
parsing, so transport and formatting are outside the interval:

| Rational-coefficient case | direct Q Ducos | CRT | CRT speedup |
|---|---:|---:|---:|
| locked dense five-variable 7/6 | 11.257 s | 1.812 s | 6.2x |
| Symbolica dense three-variable 7/6 | 0.178 s | 0.0247 s | 7.2x |
| Symbolica lacunary three-variable 18/11 | 1.069 s | 0.166 s | 6.5x |

This first table predates the integer-associate adapter: its “direct Q Ducos”
column is the rational-field recurrence and must not be used to infer the
`Auto` crossover between integer Ducos and CRT.

The subsequent FLINT-like probe included three CPU-pinned cold processes per
strategy on the locked case:

| Strategy | Median wall | Peak RSS |
|---|---:|---:|
| direct Q Ducos | 10.873 s | 115.8 MB |
| CRT | 1.823 s | 77.9 MB |
| integer-associate Ducos | 0.413 s | 66.5 MB |

Symbolica's historical comparison favors Ducos for most *integer-polynomial*
resultants. Hyperbolica stores `Poly<Q>` instead. The direct Ducos profile
shows why that distinction matters:

| Direct Ducos self samples | Share |
|---|---:|
| integer GCD | 28.8% |
| rational-field multiplication | 18.7% |
| rational add/multiply-accumulate | 11.5% |
| rational addition | 10.2% |
| rational subtract/multiply-accumulate | 4.6% |

The first optimization moved the work to CRT, which clears denominators once
and uses optimized integer/modular kernels:

| CRT self samples | Share |
|---|---:|
| dense modular polynomial division | 35.4% |
| GMP basecase multiplication | 20.2% |
| finite-field dense multiplication | 12.9% |
| GMP add/subtract | 17.4% |

The final like-for-like probe followed FLINT's representation more closely:
it also removes global integer content, runs the latest Ducos recurrence over
`Z[parameters]`, and restores the rational scale only after the resultant.
Across three CPU-pinned cold processes it was 4.4x faster than CRT and 26.3x
faster than direct rational Ducos. A 256-sample flat profile (no lost samples)
placed 68.8% in exact dense polynomial division and 26.2% in dense integer
multiplication.

`Poly::resultant` therefore defaults to `ResultantStrategy::Auto` and delegates
the choice to Symbolica's rational-coefficient entry point. The current
internal compatibility rule uses integer-associate Ducos at a term-count
product of at most 2000 and CRT above it. `Ducos` names the optimized integer
path; `RationalDucos`, Brown, primitive PRS, and CRT remain explicit comparison
hooks.

The guard intentionally reproduces HyperFLINT's size rule. Fresh checks of the
locked, dense 7/6, and lacunary 18/11 cases all favor integer Ducos below it;
the rule above the guard remains a conservative coefficient-swell policy, not
a universally calibrated crossover model. A future selector can additionally
consider outer support and degree gaps, parameter support/degrees, coefficient
height, and a dense-workspace/output-bound estimate after integer conversion.

## FLINT comparison and paired result

For the locked fixture, FLINT represents each `fmpq_mpoly` as one rational
content times a primitive `fmpz_mpoly`. Since the input term product is 378,
HyperFLINT's 2000 guard takes FLINT's exact path:

```text
fmpq_mpoly_resultant
  -> fmpz_mpoly_to_univar
  -> fmpz_mpoly_univar_resultant
  -> mpoly_univar_resultant
  -> mpoly_univar_pseudo_gcd_ducos
```

Thus its Ducos recurrence also runs over `Z[parameters]`; rational contents are
restored once at the end. A 981-sample FLINT leaf profile put 52.3% in Johnson
multiplication, 25.9% in Monagan-Pearce exact division, and 17.0% in heap
insertion.

The final 12-pair exploratory corpus run had 14/14 equal outputs and used a
clean HyperFLINT checkout at the policy's pinned
`adfd3af3be234cb43a2322bd9ec442caa26edd74` revision. The oracle was built as
`release-portable`, `-O3 -DNDEBUG`, OpenMP on, and mimalloc 3.3.2 static on. On
the resultant, Rust took 0.324 s median versus 1.992 s for C++, a 6.14x median
speedup. The equal-weight global geometric-mean Rust/C++ time ratio was 0.439,
with a 0.469 upper 95% confidence bound: about 2.28x faster overall. All 14
per-workload upper time bounds passed the locked 1.15 limit.

This is deliberately not called qualification evidence. It used the locked
corpus, pair count, warmup, affinity, bootstrap, global-time, and tail-time
values, but reused externally built binaries from the dirty pre-commit Rust
tree and relaxed only the already accepted RSS threshold for collection. The
tracked sanitized record links every aggregate to the ignored raw-artifact
hashes. It establishes an allocator-matched time comparison and RSS
observation, not a formal qualification verdict.

## Resultant memory profile

The remaining RSS gap is understood and accepted for this change. In the fair
mimalloc-on paired run, Symbolica reached 70.8 MB and FLINT reached 45.7 MB on
the dense resultant, a 1.55 ratio versus the locked 1.25 limit. This was the
only locked numerical threshold not met. Setting
`MIMALLOC_ARENA_EAGER_COMMIT=0` reduced the Symbolica result to about 43 MB in a
separate probe with no measured slowdown, but it is retained as an optional
runtime mitigation rather than silently changing allocator policy for the
whole crate.

Massif separates allocator inflation from live algorithmic storage:
Symbolica's useful-heap peak was 32.90 MB versus FLINT's 11.53 MB. About 76% of
the Symbolica peak is dense exact-division workspace, led by one 15.07 MB,
627,900-cell `Vec<MultiPrecisionInteger>`; about 19% is adjacent-Ducos
polynomial-add storage. FLINT instead uses compact sparse coefficient/exponent
buffers. Closing that intrinsic gap requires an upstream bounded, packed, or
reused dense-division workspace, or a memory-budget fallback to heap division;
clearing rational coefficients cannot remove it.

## Partial fractions

One hundred cold CLI invocations produced about 1,500 flat samples. Remaining
time is CAS normalization rather than JSON transport:

| Self samples | Share |
|---|---:|
| polynomial quotient/remainder | 18.5% |
| GCD image sampling | 4.2% |
| dense integer multiplication | 3.4% |
| polynomial dense multiplication | 2.9% |
| GCD variable bounds / univariate GCD | 5.1% combined |
| wire polynomial formatting | 0.8% |

The next useful optimization, if this path becomes a tail again, is to feed the
already-known linear factorization directly into the recurrence and reduce
canonical rational operations. Transport tuning is not supported by this
profile.

## End-to-end integration

The locked three-variable integration fixture is now so short that cold-process
startup dominates. Across 500 invocations, dynamic-loader relocation and
symbol lookup consumed about 24% of samples. Hashing consumed about 10.6%, MZV
variable-list construction 4.3%, and polynomial GCD less than 1%. For this
fixture a persistent API or library call is more relevant than another algebra
micro-optimization.

Raw `perf.data` files are generated under `target/profiles/` and intentionally
remain untracked. `examples/profile_json.rs` provides a repeatable warm-process
driver; the locked JSON harness remains the cold-process comparison authority.

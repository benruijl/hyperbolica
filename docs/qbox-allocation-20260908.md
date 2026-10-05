# Quadruple-box allocation fix and refreshed backend comparison

> Earlier benchmark jobs and report watchers were stopped by user request.
> A fresh complete-suite run is tracked in [the new live report](full-suite-20260908.md).
> Running/queued labels below belong to the preserved earlier snapshot.

The old Symbolica-backed `qbox_one_mass` run failed on a single allocation of
**232,157,872,128 bytes (216.2 GiB)**, after 2,335.6 seconds and 115.4 GiB peak
RSS. Its address-space limit was 256 GiB.

The cause is the generic polynomial heap multiplier's exponent scratch arena.
It appended an exponent vector for every visited coefficient pair, retaining
vectors even when their monomial was already in the heap or had been consumed.
Memory therefore grew with the history of pair products, not the live heap.
This path matters for qbox's wide variable context, which does not qualify for
the small packed-exponent multiplication paths.

A diagnostic run of the previous release captured the allocation stack:

```
linear partial fractions
  → factorized-rational multiplication
  → MultivariatePolynomial::heap_mul
  → RawVec::reserve / finish_grow
  → mimalloc / mmap
```

The diagnostic stopped deliberately before an mmap of 8 GiB or larger. Its
last request was 9,810,477,056 bytes. Disassembly identifies the growing buffer
as `u16` exponent storage in the generic heap path. This run is allocation-site
evidence, not a benchmark or a second uncontrolled out-of-memory run.

The fix recycles duplicate candidate slots immediately and recycles each
consumed heap slot after its exponents have been copied to output. Keys remain
immutable while referenced by the heap or hash table. Arithmetic order and
coefficient collection are preserved; no memory limit was raised.

A 14-variable regression previously retained 65,536 scratch slots for a
256-by-256 product whose output had 511 terms. It now passes a bound of at
most 514 slots. Independent product checks cover cancellation, randomized
collisions, swapped operands and finite-field coefficients.

Validation: **639 Symbolica tests**, **471 Hyperbolica tests**, all-target and
all-feature port Clippy, and the pure-Symbolica dependency/source gate pass.
The follow-up patch reconstructs the exact pinned vendor tree after the
existing cumulative patch. See [the maintained patch record](../vendor/SYMBOLICA_PATCH.md).

The fixed release SHA-256 is
`4f4106444fe9ad0a37ef96b2a52ad26c4c61b71571c7eed0bc8cbbac7cd1892c`.
All **162 serial corpus cases** have fresh measurements from both backends:
486 timed samples per backend, plus preflights and warmups. Each backend's
outputs match its archived responses under the declared fixture exclusions;
the archived fixtures establish the cross-backend semantic comparisons.
The initial attempt to compare different rational display forms directly is
preserved as a harness rejection, then corrected to the fixture policy.

The fixed qbox rerun uses the same request, eight-worker budget, 256 GiB
address-space limit, 256 MiB stacks and 24-hour timeout. It remains running.
By 18:34 UTC it had peaked at 9.65 GiB and dropped to about 2.3 GiB RSS. These
are provisional figures, not a completed-case peak or proof of a final result.
A separate diagnostic control ran for 98 CPU-seconds without triggering the
8 GiB mapping guard; its largest observed mapping was 1.25 GiB. It was then
stopped deliberately, while the uninstrumented measured run continued.

See the [live comparison table](latest-backends-20260908.md) and its CSV links
for fresh attachment measurements and the full corpus. The earlier tst4 pair
is retained with an explicit old-build label. Its scalar MZV coefficient
matches exactly after additive-term ordering; the single-expression difference
is zero, although its raw output strings differ.

Raw evidence is retained under `target/qbox-allocation-20260908/`.

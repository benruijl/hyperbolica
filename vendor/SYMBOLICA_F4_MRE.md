# Symbolica F4 same-matrix simplification reproducer

The standalone Rust example
[`symbolica-src/examples/f4_incomplete_basis_mre.rs`](symbolica-src/examples/f4_incomplete_basis_mre.rs)
is the minimized regression for an F4 correctness failure that does not use
any Hyperbolica code. It constructs two binomials in three variables over
`GF(65521)` in GrevLex order, calls raw `GroebnerBasis::new`, and checks the
result with Symbolica's exact `GroebnerBasis::is_groebner_basis` verifier. The
vendored native F4 fix makes the example succeed; removing the pre-matrix basis
snapshot reproduces the historical failure.

From the Hyperbolica repository root, run:

```sh
export SYMBOLICA_LICENSE='<your Symbolica license>'
cargo run --manifest-path vendor/symbolica-src/Cargo.toml \
  --example f4_incomplete_basis_mre \
  --no-default-features --features integer-malachite,float-astro
```

Without the native fix, F4 fails deterministically:

```text
thread 'main' panicked at examples/f4_incomplete_basis_mre.rs:
assertion failed: GroebnerBasis::is_groebner_basis(&basis.system)
```

The key invariant is independent of the Euler-characteristic caller that
originally exposed the problem:

```rust
let basis = GroebnerBasis::new(&ideal, false);
assert!(GroebnerBasis::is_groebner_basis(&basis.system));
```

The MRE deliberately does not call the vendored `ensure_groebner_basis`
fallback: the assertion tests the native F4 algorithm itself.

The two input generators are:

```text
206*x*z + 942*y
422*x^2 + 422*x*y
```

Unfixed F4 returns only these three polynomials:

```text
65520*y^2 + x^2
y^2 + x*y
27358*y + x*z
```

The S-polynomial of the last two is
`z*(y^2+x*y) - y*(27358*y+x*z) = y^2*z + 38163*y^2` modulo 65521. No returned
leading monomial divides `y^2*z`, so its normal form is nonzero. This is a
self-contained correctness failure rather than a mismatch against an external
reference value.

Fixed F4 also returns the missing polynomial:

```text
38163*y^2 + y^2*z
```

## Introducing commit and mechanism

The defect first appears in Symbolica commit
`a49b86ac364c684281a594bc88cfc7022766746c` ("Performance improvements for
Groebner basis computation", 2023-11-08). Its parent
`c1bbc0e77b9d3d28cacffcabf2bc236db626b23d` returns a valid four-polynomial
basis for the same input. The child returns the invalid three-polynomial basis
above. The regression therefore coincides exactly with the introduction of
F4's simplification table and pivot-row reuse.

The first Macaulay matrix creates the new basis row `h=x*y+y^2`. While F4 is
still iterating over rows from that matrix, it inserts `h` into the live basis.
A later pivot row `y*(x*z+27358*y)` has leading monomial `x*y*z`. Because
`LM(h)=x*y` divides that monomial, the old code incorrectly registers the
pivot as the cached simplification of `z*h`. Those polynomials have the same
leading monomial but differ by the still-unprocessed S-polynomial
`y^2*z+38163*y^2`. The next F4 iteration therefore sees both sides as the same
cached row and loses the S-polynomial.

The native fix snapshots the basis immediately after echelonization and uses
that pre-matrix snapshot while registering simplification rules. New rows are
still added normally for future critical pairs, but cannot become reducers for
other rows produced by their own matrix.

## Verified environment

- Current Symbolica `dev`: `f9f756250201a13d2b06a5a75b383cb1e522c69f`
- Hyperbolica's Symbolica source snapshot: `61f88b381247f5f25dd7832b0286b5e362bd4148`
- Symbolica package version: `2.2.0`
- Rust: `1.89.0`
- Unpatched result on both revisions: deterministic assertion failure; the
  minimized case reproduced in 100/100 consecutive runs
- Patched result on both revisions: valid basis with the missing fourth
  polynomial; every input generator reduces to zero. Patched public `dev`
  passed 100/100 consecutive runs
- Deterministic randomized verification on the vendored snapshot: 96/96 small
  systems over `GF(3)`, `GF(5)`, `GF(7)`, `GF(11)`, `GF(101)`, and `GF(65521)`

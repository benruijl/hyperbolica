# Smirnov parser corpus

These five public mathematical input fixtures are copied byte-for-byte from
`HyperFLINT/test/Smirnov/tst0.txt` through `tst4.txt` in the upstream MIT
repository. They are test data, not implementation code. The Rust regression
only ingests them through the public parser and checks variable and logarithmic
weight invariants; it does not embed upstream algorithm bodies or expected
printer snapshots.

| Fixture | Bytes | SHA-256 |
| --- | ---: | --- |
| `tst0.txt` | 177 | `663d3d469cde7c7d657547c8b5f3296434219dff1680aa37df20a9d54420579c` |
| `tst1.txt` | 531 | `3f0c95b2a9547ec43e1f7c0561857bd1a28c62d9f0008ddc0736c21fb7602fb8` |
| `tst2.txt` | 3,232 | `b53a61c797995f40ff61f50a7e64dadd1f72a1fb5ec968782950309c6ed039e6` |
| `tst3.txt` | 14,106 | `1e5a90dacb4d4878461fcb0d75873e538efcfcd78db33d15a74ba780c151aba2` |
| `tst4.txt` | 41,327 | `bb128bce2f1983a2a31649c329de4fa64fbf08b33e0e4c58c6536117275a16ea` |

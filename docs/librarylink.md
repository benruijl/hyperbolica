# Wolfram LibraryLink adapter

Hyperbolica provides a separate `hyperbolica-librarylink` Rust crate that
matches the LibraryLink surface loaded by SubTropica at the pinned upstream
commit `adfd3af3be234cb43a2322bd9ec442caa26edd74`.

It is intentionally a separate dynamic library.  HyperFLINT uses names such as
`hf_find_lr_orders` both for its stable C ABI and for LibraryLink, but the two
functions have incompatible signatures.  Combining them in one shared object
would be an ABI collision.

## Exported contract

| Symbol | Wolfram signature | Required by the upstream loader |
| --- | --- | --- |
| `WolframLibrary_getVersion` | LibraryLink lifecycle | yes |
| `WolframLibrary_initialize` / `WolframLibrary_uninitialize` | LibraryLink lifecycle | yes |
| `hf_version` | `{} -> "UTF8String"` | yes; strict SubTropica version gate |
| `hf_clear_state` | `{} -> Integer` | optional |
| `hf_find_lr_orders` | `{"UTF8String"} -> "UTF8String"` | yes |
| `hf_factor_table` | `{"UTF8String"} -> "UTF8String"` | optional |
| `hf_find_lr_orders_scan` | `{"UTF8String"} -> "UTF8String"` | optional |
| `hf_hyperflint_sym` | `{"UTF8String"} -> "UTF8String"` | optional, hot integration path |

The adapter loads Hyperbolica's stable C-ABI library once and delegates every
JSON operation to it.  This keeps Symbolica and all mathematical code in the
Rust kernel while adding only LibraryLink pointer-slot marshalling and one
response copy, the same copy class as upstream's `set_utf8_result`.  It never
links FLINT or a Wolfram runtime.  The `wolfram-library-link-sys` crate supplies
Wolfram's pre-generated ABI definitions, so it compiles on a build host without
`WolframLibrary.h`, Mathematica, or Wolfram Engine.

Hyperbolica already gives every algebraic-letter-mutating request an exclusive,
fresh session.  Consequently the LibraryLink adapter has no persistent letter
registry to clear; `hf_clear_state[]` releases its last thread-local return
buffer and returns zero.  Inputs are disowned through
`WolframLibraryData->UTF8String_disown` after synchronous delegation.  Panics,
missing backends, null slots, and malformed backend results cannot unwind across
the C boundary.

## Build and local stage

Build the release CLI, stable C-ABI backend, and both-library LibraryLink
layout with:

```bash
scripts/stage-local.sh
```

The default output is `target/local-stage`; an optional first argument selects
another directory. The adapter is stamped for the pinned SubTropica `1.2.13`
loader by default. The stage also contains both C headers, Hyperbolica's MIT
license, the distribution warning, Symbolica's controlling license, and
`stage-metadata.json`. That metadata deliberately says `redistributable:
false`: this is a convenient local installation/test layout, not a release
package or permission to redistribute Symbolica.

The destination must be absent or empty. Artifacts are assembled in a private
directory, the destination is rechecked after both builds, and publication is
no-clobber plus byte verification; a concurrent file is preserved and makes
the staging command fail.

Override the loader version only when targeting a different known-compatible
SubTropica checkout:

```bash
HYPERBOLICA_SUBTROPICA_VERSION=1.2.14 scripts/stage-local.sh
```

`LOCAL_STAGE_OUTPUT`, `HYPERBOLICA_TARGET_DIR`, and
`HYPERBOLICA_LIBRARYLINK_TARGET_DIR` provide explicit paths for automation.
The older `scripts/build-librarylink.sh` helper remains available when only the
two shared libraries are wanted, but it does not create the unified local
layout.

Use `.dylib` on macOS and `hyperbolica.dll` /
`hyperflint_librarylink.dll` on Windows.  Same-directory discovery is the
default.  `HYPERBOLICA_LIBRARY_PATH=/absolute/path/to/libhyperbolica.so`
overrides discovery and is useful for development or a nonstandard package
layout.

Pinned SubTropica rejects a LibraryLink library unless `hf_version[]` equals
its own `$SubTropicaVersion` (`1.2.13` at the pinned commit). For a manual
adapter build, stamp that compatible version with:

```bash
HYPERBOLICA_SUBTROPICA_VERSION=1.2.13 \
  cargo build --release --manifest-path librarylink/Cargo.toml
```

When set, the adapter also replaces the response envelope's `hf_version`, so
the direct version probe and JSON responses remain consistent.  Without this
setting, both use the loaded Hyperbolica stable library's version.

The fallback header at
`librarylink/include/hyperbolica_librarylink.h` contains only the ABI subset
needed by this adapter's smoke tests.  Production Wolfram extensions should
compile against the SDK header by defining
`HYPERBOLICA_USE_SYSTEM_WOLFRAM_LIBRARY` and adding the SDK IncludeFiles/C
directory.

## Verification and Wolfram example

Run the complete compile/export/marshalling gate:

```bash
scripts/check-librarylink.sh
```

When a Symbolica license is available in `SYMBOLICA_LICENSE` or
`SYMBOLICA_LICENSE_SERVER`, the script automatically exercises successful
native calls to all four JSON-backed LibraryLink functions as well as the
license-independent malformed-input paths. The licensed LR portion mirrors all
six strategy rows from upstream's
`test_find_lr_orders_strategy_roundtrip_librarylink.cpp` (`LR_NoOpt`,
`LR_OptOrdered`, `Fubini_Lungo`, and `Fubini_Espresso`); the additional rows
cover factor-table construction, LR scanning, and a complete convergent
integration. The script does not print license values.

After staging, set `HYPERBOLICA_LIBRARYLINK` to the adapter path and run
`wolframscript -file librarylink/examples/load.wl`.  The example binds all six
upstream `hf_*` functions and evaluates a one-variable LR request.

No Wolfram runtime is installed in the current development environment, so the
automated gate covers the official ABI layout, exact exported symbols, C and
C++ header compilation, lifecycle calls, UTF8String input disown, error JSON,
version consistency, and optional licensed Symbolica delegation.  A final
`LibraryFunctionLoad` run in Wolfram Engine remains an installation-level gate.

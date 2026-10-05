#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cargo_bin=${CARGO:-cargo}

check_dependency_graph() {
    local label=$1
    shift
    local dependency_tree
    dependency_tree=$(
        cd "$repo_root"
        "$cargo_bin" tree --edges normal --prefix none "$@"
    )

    local forbidden
    for forbidden in \
        flint flint-sys flint3-sys singular symengine sympy giac maxima msolve \
        macaulay2 ginac pari-sys ntl sage; do
        if awk -v package="$forbidden" \
            '$1 == package { found = 1 } END { exit !found }' \
            <<<"$dependency_tree"; then
            echo "pure-Symbolica gate: forbidden runtime dependency '$forbidden' in $label graph" >&2
            exit 1
        fi
    done
}

check_dependency_graph default
check_dependency_graph python --features python
check_dependency_graph native-python --features python_stubgen
check_dependency_graph wasm-python --no-default-features --features wasm,python

if rg -n \
    '(extern[[:space:]]+crate[[:space:]]+.*\bflint\b|use[[:space:]]+.*\bflint\b|flint3?_sys|Command::new\([^)]*(msolve|singular|sage|macaulay2|maxima|giac))' \
    "$repo_root/src" "$repo_root/Cargo.toml"; then
    echo "pure-Symbolica gate: forbidden CAS binding or subprocess in production code" >&2
    exit 1
fi

# Library-owned indexed objects are function atoms of the fixed heads declared
# in symbols.rs. Dynamic spellings are allowed only in the dedicated
# legacy wire adapter and never in mathematical code.
if rg -n \
    '(symbol!|Symbol::parse|PolyCtx::new)[^\n]*(mzv_[m0-9]|Wm_[0-9]|Wp_[0-9]|WmOverWp_[0-9]|sqrt_disc_[0-9])' \
    "$repo_root/src" \
    --glob '!symbols/legacy.rs'; then
    echo "pure-Symbolica gate: dynamically named special atom outside symbols/legacy.rs" >&2
    exit 1
fi

mapfile -t initializer_sites < <(rg -l 'initialize!' "$repo_root/src" --glob '*.rs')
if [[ ${#initializer_sites[@]} -ne 1 \
    || "${initializer_sites[0]}" != "$repo_root/src/symbols.rs" ]]; then
    echo "pure-Symbolica gate: library-owned heads must use the single initializer in src/symbols.rs" >&2
    printf 'initializer site: %s\n' "${initializer_sites[@]:-<none>}" >&2
    exit 1
fi

if rg -n \
    'features[[:space:]]*=[[:space:]]*\[[^]]*(flint_benchmarks|flint_system_benchmarks)' \
    "$repo_root/Cargo.toml"; then
    echo "pure-Symbolica gate: a Symbolica FLINT feature is selected" >&2
    exit 1
fi

if ! rg -q \
    'symbolica[[:space:]]*=[[:space:]]*\{[^}]*version[[:space:]]*=[[:space:]]*"3.0.1"[^}]*default-features[[:space:]]*=[[:space:]]*false' \
    "$repo_root/Cargo.toml"; then
    echo "pure-Symbolica gate: the pinned, feature-controlled vendor dependency is missing" >&2
    exit 1
fi

symbolica_checkout="$repo_root/vendor/symbolica"
if [[ $(cd "$repo_root" && "$cargo_bin" tree -p symbolica --depth 0) != "symbolica v3.0.1 ($symbolica_checkout)" ]]; then
    echo "pure-Symbolica gate: development dependency does not resolve to the audited checkout" >&2
    exit 1
fi
symbolica_base=75f8350094b90254ee71dc2a391fde0d14b0204a
# Pin the complete pristine upstream tree used for development validation.
symbolica_tree=e6c84b0df8498081c2610cb8aa1d5b996b60832d

if [[ ! -d "$symbolica_checkout/.git" ]]; then
    echo "pure-Symbolica gate: vendor/symbolica is not a Git checkout" >&2
    exit 1
fi
if [[ $(git -C "$symbolica_checkout" remote get-url origin) \
        != https://github.com/symbolica-dev/symbolica.git \
    || $(git -C "$symbolica_checkout" rev-parse 'HEAD^{tree}') != "$symbolica_tree" ]]; then
    echo "pure-Symbolica gate: vendor/symbolica is not the audited upstream source tree" >&2
    exit 1
fi
mapfile -t symbolica_untracked < <(
    git -C "$symbolica_checkout" ls-files --others --exclude-standard
)
if [[ ${#symbolica_untracked[@]} -ne 0 ]]; then
    echo "pure-Symbolica gate: vendor/symbolica contains untracked files" >&2
    printf 'untracked Symbolica file: %s\n' "${symbolica_untracked[@]}" >&2
    exit 1
fi
if ! git -C "$symbolica_checkout" diff --quiet --ignore-submodules -- \
    || ! git -C "$symbolica_checkout" diff --cached --quiet --ignore-submodules --; then
    echo "pure-Symbolica gate: vendor/symbolica has tracked local changes" >&2
    exit 1
fi

if ! git -C "$symbolica_checkout" merge-base --is-ancestor "$symbolica_base" HEAD; then
    echo "pure-Symbolica gate: required upstream base $symbolica_base is absent" >&2
    exit 1
fi

echo "pure-Symbolica gate: dependency graphs, source imports, special symbols, and the pinned Symbolica checkout are clean"

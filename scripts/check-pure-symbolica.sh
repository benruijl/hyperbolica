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
check_dependency_graph all-features --all-features

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
    'symbolica[[:space:]]*=[[:space:]]*\{[^}]*path[[:space:]]*=[[:space:]]*"vendor/symbolica-src"[^}]*default-features[[:space:]]*=[[:space:]]*false' \
    "$repo_root/Cargo.toml"; then
    echo "pure-Symbolica gate: the pinned, feature-controlled vendor dependency is missing" >&2
    exit 1
fi

echo "pure-Symbolica gate: default/Python dependency graphs, source imports, and special symbols are clean"

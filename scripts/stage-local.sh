#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cargo_bin=${CARGO:-cargo}
output_dir=${1:-${LOCAL_STAGE_OUTPUT:-"$repo_root/target/local-stage"}}
compatibility_version=${HYPERBOLICA_SUBTROPICA_VERSION:-1.2.13}
root_target_dir=${HYPERBOLICA_TARGET_DIR:-"$repo_root/target"}
librarylink_target_dir=${HYPERBOLICA_LIBRARYLINK_TARGET_DIR:-"$repo_root/librarylink/target"}

[[ -n "$output_dir" && "$output_dir" != / ]] || {
    echo "stage-local: refusing unsafe output directory '$output_dir'" >&2
    exit 2
}
if [[ -e "$output_dir" && ! -d "$output_dir" ]]; then
    echo "stage-local: output path exists and is not a directory: $output_dir" >&2
    exit 2
fi
if [[ -d "$output_dir" ]]; then
    first_entry=$(find "$output_dir" -mindepth 1 -maxdepth 1 -print -quit)
    [[ -z "$first_entry" ]] || {
        echo "stage-local: refusing nonempty output directory: $output_dir" >&2
        exit 2
    }
fi
[[ "$compatibility_version" =~ ^[0-9A-Za-z][0-9A-Za-z._+-]*$ ]] || {
    echo "stage-local: invalid HYPERBOLICA_SUBTROPICA_VERSION" >&2
    exit 2
}

case $(uname -s) in
    Darwin)
        backend_name=libhyperbolica.dylib
        adapter_name=libhyperflint_librarylink.dylib
        ;;
    Linux)
        backend_name=libhyperbolica.so
        adapter_name=libhyperflint_librarylink.so
        ;;
    *)
        echo "stage-local: this helper currently supports macOS and Linux" >&2
        exit 2
        ;;
esac

# A Symbolica credential is a runtime secret. Compilation does not need it,
# so keep it out of Cargo and dependency build-script environments.
env -u SYMBOLICA_LICENSE -u SYMBOLICA_LICENSE_SERVER -- "$cargo_bin" build \
    --manifest-path "$repo_root/Cargo.toml" \
    --locked --release --bin hyperflint --lib \
    --target-dir "$root_target_dir"
env -u SYMBOLICA_LICENSE -u SYMBOLICA_LICENSE_SERVER \
    HYPERBOLICA_SUBTROPICA_VERSION="$compatibility_version" \
    "$cargo_bin" build \
    --manifest-path "$repo_root/librarylink/Cargo.toml" \
    --locked --release \
    --target-dir "$librarylink_target_dir"

cli="$root_target_dir/release/hyperflint"
backend="$root_target_dir/release/$backend_name"
adapter="$librarylink_target_dir/release/$adapter_name"
for artifact in "$cli" "$backend" "$adapter"; do
    [[ -f "$artifact" ]] || {
        echo "stage-local: expected release artifact was not produced: $artifact" >&2
        exit 1
    }
done

stage_scratch=$(mktemp -d "${TMPDIR:-/tmp}/hyperbolica-local-stage.XXXXXXXX")
cleanup() {
    rm -rf -- "$stage_scratch"
}
trap cleanup EXIT

mkdir -p -- "$stage_scratch/include/hyperbolica" "$stage_scratch/licenses"
cp -- "$cli" "$stage_scratch/hyperflint"
cp -- "$backend" "$stage_scratch/$backend_name"
cp -- "$adapter" "$stage_scratch/$adapter_name"
cp -- "$repo_root/include/hyperbolica/c_abi.h" \
    "$stage_scratch/include/hyperbolica/c_abi.h"
cp -- "$repo_root/librarylink/include/hyperbolica_librarylink.h" \
    "$stage_scratch/include/hyperbolica_librarylink.h"
cp -- "$repo_root/LICENSE" "$stage_scratch/licenses/LICENSE"
cp -- "$repo_root/DISTRIBUTION-LICENSE.md" \
    "$stage_scratch/licenses/DISTRIBUTION-LICENSE.md"
cp -- "$repo_root/vendor/symbolica/License.md" \
    "$stage_scratch/licenses/SYMBOLICA-License.md"

hf_version=$(
    env -u SYMBOLICA_LICENSE -u SYMBOLICA_LICENSE_SERVER -- \
        "$stage_scratch/hyperflint" --version |
        awk '$1 == "HF_VERSION:" { print $2; exit }'
)
[[ "$hf_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
    echo "stage-local: staged CLI reported an invalid HF_VERSION" >&2
    exit 1
}

printf '%s\n' \
    '{' \
    '  "schema": 1,' \
    '  "artifact_kind": "local-only-development-stage",' \
    "  \"hyperbolica_hf_version\": \"$hf_version\"," \
    "  \"subtropica_compatibility_version\": \"$compatibility_version\"," \
    '  "build_profile": "release-lto",' \
    '  "redistributable": false,' \
    '  "license_notice": "licenses/DISTRIBUTION-LICENSE.md"' \
    '}' >"$stage_scratch/stage-metadata.json"

# The output may have been populated while the release builds were running.
# Revalidate it immediately before publication. A newly absent path is claimed
# with one mkdir; an existing path must still be an empty directory.
if [[ ! -e "$output_dir" ]]; then
    mkdir -p -- "$(dirname -- "$output_dir")"
    if ! mkdir -- "$output_dir"; then
        echo "stage-local: could not claim output directory: $output_dir" >&2
        exit 2
    fi
fi
if [[ ! -d "$output_dir" ]]; then
    echo "stage-local: output path became a non-directory: $output_dir" >&2
    exit 2
fi
first_entry=$(find "$output_dir" -mindepth 1 -maxdepth 1 -print -quit)
[[ -z "$first_entry" ]] || {
    echo "stage-local: output directory became nonempty during the build: $output_dir" >&2
    exit 2
}

# Never overwrite a path introduced by a concurrent writer. `cp -n` skips a
# collision; the recursive diff then makes any skip, mutation, or extra file a
# hard failure while preserving the conflicting output for inspection.
cp -R -n -- "$stage_scratch/." "$output_dir/"
if ! diff -qr -- "$stage_scratch" "$output_dir" >/dev/null; then
    echo "stage-local: output changed during publication; no existing file was overwritten" >&2
    exit 2
fi
output_dir=$(cd -- "$output_dir" && pwd)

printf '%s\n' \
    "Local-only Hyperbolica stage created at $output_dir" \
    "  CLI: $output_dir/hyperflint" \
    "  backend: $output_dir/$backend_name" \
    "  LibraryLink: $output_dir/$adapter_name" \
    "  SubTropica compatibility: $compatibility_version" \
    "This is not a redistributable release package; read licenses/DISTRIBUTION-LICENSE.md."

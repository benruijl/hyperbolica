#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cargo_bin=${CARGO:-cargo}
output_dir=${1:-"$repo_root/target/librarylink-dist"}

[[ -n "$output_dir" && "$output_dir" != "/" ]] || {
    echo "build-librarylink: refusing unsafe output directory '$output_dir'" >&2
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
        echo "build-librarylink: this staging helper currently supports macOS and Linux" >&2
        exit 2
        ;;
esac

(
    cd "$repo_root"
    "$cargo_bin" build --release --lib
)
"$cargo_bin" build --release \
    --manifest-path "$repo_root/librarylink/Cargo.toml"

backend="$repo_root/target/release/$backend_name"
adapter="$repo_root/librarylink/target/release/$adapter_name"
[[ -f "$backend" && -f "$adapter" ]] || {
    echo "build-librarylink: expected release artifacts were not produced" >&2
    exit 1
}

mkdir -p "$output_dir"
cp "$backend" "$output_dir/$backend_name"
cp "$adapter" "$output_dir/$adapter_name"

echo "LibraryLink bundle staged:"
echo "  $output_dir/$backend_name"
echo "  $output_dir/$adapter_name"


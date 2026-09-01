#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
# shellcheck source=scripts/lib/response-comparison.sh
source "$repo_root/scripts/lib/response-comparison.sh"

locked_workloads="$repo_root/tests/fixtures/benchmark.jsonl"
locked_policy="$repo_root/tests/fixtures/benchmark-policy.json"
locked_corpus="$repo_root/tests/fixtures/benchmark-corpus.json"
workloads=${BENCH_WORKLOADS:-"$locked_workloads"}
policy=${BENCHMARK_POLICY:-"$locked_policy"}
corpus=${BENCHMARK_CORPUS:-"$locked_corpus"}
mode=${BENCHMARK_MODE:-exploratory}
tier=${BENCHMARK_TIER:-qualification}
rust_bin=${HYPERFLINT_RUST:-"$repo_root/target/release/hyperflint"}
cpp_bin=${HYPERFLINT_CPP:-/tmp/hyperflint-cpp-build/hyperflint}
cargo_bin=${CARGO:-cargo}
cmake_bin=${CMAKE:-cmake}
python_bin=${PYTHON:-python3}
build_rust=${BUILD_RUST:-1}
build_cpp=${BUILD_CPP:-}
default_timeout=${TIMEOUT_SECONDS:-300}
evidence_dir=${EVIDENCE_DIR:-}
results_file=${RESULTS_FILE:-}
samples_csv=${RAW_SAMPLES_FILE:-}
metadata_file=${METADATA_FILE:-}
rust_source=${HYPERBOLICA_SOURCE:-"$repo_root"}
cpp_source_hint=${HYPERFLINT_CPP_SOURCE:-}
cpp_cache=${HYPERFLINT_CPP_CMAKE_CACHE:-}
cpp_build_command=${HYPERFLINT_CPP_BUILD_COMMAND:-external-prebuilt-binary}

fail() {
    local message=$1
    local status=${2:-2}
    printf 'benchmark-compare: %s\n' "$message" >&2
    exit "$status"
}

qualification_fail() {
    fail "$1" 1
}

for tool in jq awk sort date mktemp cmp diff sha256sum git realpath hostname uname; do
    command -v "$tool" >/dev/null 2>&1 ||
        fail "required tool '$tool' is unavailable"
done
command -v "$python_bin" >/dev/null 2>&1 ||
    fail "Python interpreter '$python_bin' is unavailable"
python_bin=$(command -v "$python_bin")
cargo_bin=$(command -v "$cargo_bin") ||
    fail "Cargo executable '$cargo_bin' is unavailable"

[[ "$mode" == exploratory || "$mode" == qualification ]] ||
    fail "BENCHMARK_MODE must be 'exploratory' or 'qualification'"
[[ "$tier" == qualification || "$tier" == nightly || "$tier" == heavyweight ||
    "$tier" == exploratory || "$tier" == all ]] ||
    fail "BENCHMARK_TIER is invalid"
[[ "$build_rust" =~ ^[01]$ ]] || fail "BUILD_RUST must be 0 or 1"
[[ "$default_timeout" =~ ^[1-9][0-9]*$ ]] ||
    fail "TIMEOUT_SECONDS must be a positive integer"
for path in "$workloads" "$policy" "$corpus" "$locked_workloads" \
    "$locked_policy" "$locked_corpus"; do
    [[ -f "$path" ]] || fail "required input not found: $path"
done

BENCH_WORKLOADS="$workloads" "$repo_root/scripts/lint-fixtures.sh" ||
    fail "benchmark fixture validation failed"

policy_id=$(jq -er '.policy_id' "$policy")
policy_sha256=$(sha256sum "$policy" | awk '{print $1}')
corpus_sha256=$(sha256sum "$corpus" | awk '{print $1}')
fixture_sha256=$(sha256sum "$workloads" | awk '{print $1}')
corpus_fixture_sha256=$(jq -er '.fixture_sha256' "$corpus")
qualification_names=$(jq -c '[.workloads[] | select(.tier == "qualification") | .name] | sort' "$corpus")

require_matching_override() {
    local name=$1
    local expected=$2
    if [[ -v $name && "${!name}" != "$expected" ]]; then
        qualification_fail "$name=${!name} differs from the locked qualification value $expected"
    fi
}

reject_override() {
    local name=$1
    if [[ -v $name ]]; then
        qualification_fail "$name cannot override qualification provenance"
    fi
}

if [[ "$mode" == qualification ]]; then
    build_cpp=${build_cpp:-1}
    [[ "$policy_sha256" == "$(sha256sum "$locked_policy" | awk '{print $1}')" ]] ||
        qualification_fail "qualification mode requires the checked-in locked policy bytes"
    [[ "$corpus_sha256" == "$(sha256sum "$locked_corpus" | awk '{print $1}')" ]] ||
        qualification_fail "qualification mode requires the checked-in corpus manifest bytes"
    [[ "$fixture_sha256" == "$(sha256sum "$locked_workloads" | awk '{print $1}')" ]] ||
        qualification_fail "qualification mode requires the checked-in benchmark fixture bytes"
    [[ "$tier" == qualification ]] ||
        qualification_fail "qualification mode requires BENCHMARK_TIER=qualification"
    [[ "$(jq -r '.locked' "$policy")" == true ]] ||
        qualification_fail "qualification mode requires a locked policy"
    [[ "$fixture_sha256" == "$corpus_fixture_sha256" ]] ||
        fail "benchmark fixture bytes do not match the locked corpus manifest"

    pairs=$(jq -er '.requirements.minimum_pairs_per_workload' "$policy")
    warmup=$(jq -er '.requirements.warmup_runs' "$policy")
    threads=$(jq -er '.requirements.threads' "$policy")
    bootstrap_samples=$(jq -er '.requirements.bootstrap_samples' "$policy")
    seed=$(jq -er '.requirements.seed' "$policy")
    cpuset=$(jq -er '.requirements.cpuset' "$policy")
    thresholds=$(jq -c '.requirements.thresholds' "$policy")
    rust_profile_required=$(jq -er '.requirements.backend_profiles.rust' "$policy")
    cpp_profile_required=$(jq -er '.requirements.backend_profiles.cpp_oracle' "$policy")
    cpp_revision_required=$(jq -er '.requirements.required_cpp_revision' "$policy")
    cpp_build_type_required=$(jq -er '.requirements.required_cpp_cmake.build_type' "$policy")
    cpp_mimalloc_required=$(jq -er '.requirements.required_cpp_cmake.mimalloc' "$policy")
    cpp_openmp_required=$(jq -er '.requirements.required_cpp_cmake.openmp' "$policy")
    cpp_generator_required=$(jq -er '.requirements.required_cpp_cmake.generator' "$policy")
    cpp_cxx_flags_required=$(jq -r '.requirements.required_cpp_cmake.cxx_flags' "$policy")
    cpp_cxx_flags_release_required=$(jq -er '.requirements.required_cpp_cmake.cxx_flags_release' "$policy")
    cpp_asan_required=$(jq -er '.requirements.required_cpp_cmake.asan' "$policy")
    cpp_tsan_required=$(jq -er '.requirements.required_cpp_cmake.tsan' "$policy")
    cpp_static_deps_required=$(jq -er '.requirements.required_cpp_cmake.cli_static_deps' "$policy")
    require_matching_override PAIRS "$pairs"
    require_matching_override ITERATIONS "$pairs"
    require_matching_override WARMUP "$warmup"
    require_matching_override THREADS "$threads"
    require_matching_override BOOTSTRAP_SAMPLES "$bootstrap_samples"
    require_matching_override SEED "$seed"
    require_matching_override CPUSET "$cpuset"
    require_matching_override GLOBAL_UPPER_CI "$(jq -r '.global_upper_ci' <<<"$thresholds")"
    require_matching_override MAX_WORKLOAD_RATIO "$(jq -r '.max_workload_ratio' <<<"$thresholds")"
    require_matching_override MAX_RSS_RATIO "$(jq -r '.max_rss_ratio' <<<"$thresholds")"
    for provenance_override in RUST_REVISION CPP_REVISION HYPERBOLICA_SOURCE \
        HYPERFLINT_RUST HYPERFLINT_CPP_SOURCE HYPERFLINT_CPP_PROFILE \
        HYPERFLINT_CPP_BUILD_TYPE HYPERFLINT_CPP_MIMALLOC \
        HYPERFLINT_CPP_OPENMP; do
        reject_override "$provenance_override"
    done
    [[ "$build_rust" == 1 ]] ||
        qualification_fail "qualification mode requires BUILD_RUST=1 to bind the Rust binary"
    [[ "$build_cpp" == 1 ]] ||
        qualification_fail "qualification mode requires BUILD_CPP=1 to bind the C++ binary"

    # Cargo profiles and output paths must describe the binary this driver
    # actually builds. A fresh target directory prevents a stale executable
    # from being mistaken for this invocation's output; rejecting Rust and
    # native-toolchain build controls keeps both locked profiles truthful.
    while IFS= read -r environment_name; do
        case "$environment_name" in
            RUSTFLAGS | CARGO_ENCODED_RUSTFLAGS | CARGO_INCREMENTAL | \
                CARGO_TARGET_DIR | CARGO_PROFILE_* | CARGO_BUILD_* | \
                CARGO_TARGET_* | RUSTC | RUSTC_WRAPPER | RUSTC_WORKSPACE_WRAPPER | \
                CC | CXX | AR | RANLIB | CFLAGS | CXXFLAGS | CPPFLAGS | \
                LDFLAGS | CMAKE_TOOLCHAIN_FILE)
                qualification_fail \
                    "$environment_name cannot alter a qualification build"
                ;;
        esac
    done < <(compgen -e)

    cargo_home=${CARGO_HOME:-${HOME:-}/.cargo}
    for cargo_config in "$cargo_home/config" "$cargo_home/config.toml"; do
        [[ ! -e "$cargo_config" ]] ||
            qualification_fail \
                "qualification requires no user Cargo config: $cargo_config"
    done
    config_directory=$repo_root
    while :; do
        for cargo_config in "$config_directory/.cargo/config" \
            "$config_directory/.cargo/config.toml"; do
            [[ ! -e "$cargo_config" ]] ||
                qualification_fail \
                    "qualification requires no ambient Cargo config: $cargo_config"
        done
        [[ "$config_directory" == / ]] && break
        config_directory=$(dirname -- "$config_directory")
    done
else
    build_cpp=${build_cpp:-0}
    pairs=${PAIRS:-${ITERATIONS:-4}}
    warmup=${WARMUP:-1}
    threads=${THREADS:-1}
    bootstrap_samples=${BOOTSTRAP_SAMPLES:-2000}
    seed=${SEED:-1729}
    cpuset=${CPUSET:-}
    thresholds=$(
        jq -cn --argjson global "${GLOBAL_UPPER_CI:-1.20}" --argjson tail "${MAX_WORKLOAD_RATIO:-1.50}" --argjson rss "${MAX_RSS_RATIO:-2.00}" '{global_upper_ci:$global,max_workload_ratio:$tail,max_rss_ratio:$rss}'
    )
fi

[[ "$pairs" =~ ^[1-9][0-9]*$ ]] || fail "PAIRS must be a positive integer"
[[ "$warmup" =~ ^[0-9]+$ ]] || fail "WARMUP must be a non-negative integer"
[[ "$threads" =~ ^[1-9][0-9]*$ ]] || fail "THREADS must be a positive integer"
[[ "$bootstrap_samples" =~ ^[1-9][0-9]*$ ]] ||
    fail "BOOTSTRAP_SAMPLES must be a positive integer"
[[ "$seed" =~ ^[0-9]+$ ]] || fail "SEED must be a non-negative integer"
[[ "$build_cpp" =~ ^[01]$ ]] || fail "BUILD_CPP must be 0 or 1"
jq -e 'all(.[]; type == "number" and . > 0)' <<<"$thresholds" >/dev/null ||
    fail "performance thresholds must be positive JSON numbers"

taskset_bin=
if [[ -n "$cpuset" ]]; then
    taskset_bin=$(command -v taskset) ||
        fail "CPUSET requires taskset"
fi

rust_build_command="$cargo_bin build --locked --release --bin hyperflint"
if [[ "$build_rust" == 1 ]]; then
    if [[ "$mode" == qualification ]]; then
        mkdir -p -- "$repo_root/target"
        rust_target_dir=$(mktemp -d "$repo_root/target/benchmark-rust-build.XXXXXXXX")
        rust_build_command="$rust_build_command --target-dir $rust_target_dir --config profile.release.lto=true --config profile.release.codegen-units=1"
        "$cargo_bin" build --locked --release --bin hyperflint \
            --target-dir "$rust_target_dir" \
            --config profile.release.lto=true \
            --config profile.release.codegen-units=1
        rust_bin="$rust_target_dir/release/hyperflint"
    else
        "$cargo_bin" build --locked --release --bin hyperflint
    fi
fi

resolve_executable() {
    local selected=$1
    local resolved
    if [[ "$selected" == */* ]]; then
        [[ -x "$selected" ]] || return 1
        realpath "$selected"
    else
        resolved=$(command -v "$selected") || return 1
        realpath "$resolved"
    fi
}

rust_bin=$(resolve_executable "$rust_bin") ||
    fail "Rust executable not found or not executable: $rust_bin"
cpp_bin=$(resolve_executable "$cpp_bin") ||
    fail "C++ executable not found or not executable: $cpp_bin"
rust_source=$(realpath "$rust_source") ||
    fail "Rust source tree not found: $rust_source"

if [[ -z "$cpp_cache" ]]; then
    cpp_cache="$(dirname -- "$cpp_bin")/CMakeCache.txt"
fi
if [[ -f "$cpp_cache" ]]; then
    cpp_cache=$(realpath "$cpp_cache")
elif [[ "$mode" == qualification ]]; then
    qualification_fail "qualification mode requires the C++ CMake cache: $cpp_cache"
fi
cmake_value() {
    local name=$1
    if [[ -f "$cpp_cache" ]]; then
        awk -F= -v key="$name" '$1 ~ ("^" key "(:[^=]+)?$") {print $2; exit}' "$cpp_cache"
    fi
}
cpp_source=${cpp_source_hint:-$(cmake_value CMAKE_HOME_DIRECTORY)}
cpp_source=${cpp_source:-unknown}
if [[ "$cpp_source" != unknown && -e "$cpp_source" ]]; then
    cpp_source=$(realpath "$cpp_source")
fi
cpp_build_type=${HYPERFLINT_CPP_BUILD_TYPE:-$(cmake_value CMAKE_BUILD_TYPE)}
cpp_build_type=${cpp_build_type:-unknown}
cpp_profile=${HYPERFLINT_CPP_PROFILE:-$(cmake_value HF_BUILD_VARIANT)}
cpp_profile=${cpp_profile:-unknown}
cpp_mimalloc=${HYPERFLINT_CPP_MIMALLOC:-$(cmake_value HF_MIMALLOC)}
cpp_mimalloc=${cpp_mimalloc:-unknown}
cpp_openmp=${HYPERFLINT_CPP_OPENMP:-$(cmake_value HF_OPENMP)}
cpp_openmp=${cpp_openmp:-unknown}
cpp_generator=$(cmake_value CMAKE_GENERATOR)
cpp_generator=${cpp_generator:-unknown}
cpp_cxx_flags=$(cmake_value CMAKE_CXX_FLAGS)
cpp_cxx_flags=${cpp_cxx_flags:-unknown}
cpp_cxx_flags_release=$(cmake_value CMAKE_CXX_FLAGS_RELEASE)
cpp_cxx_flags_release=${cpp_cxx_flags_release:-unknown}
cpp_asan=$(cmake_value HF_ASAN)
cpp_asan=${cpp_asan:-unknown}
cpp_tsan=$(cmake_value HF_TSAN)
cpp_tsan=${cpp_tsan:-unknown}
cpp_static_deps=$(cmake_value HF_CLI_STATIC_DEPS)
cpp_static_deps=${cpp_static_deps:-unknown}
cpp_version=unknown

git_revision() {
    local source=$1
    local revision
    if [[ "$source" != unknown ]] &&
        git -C "$source" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        revision=$(git -C "$source" rev-parse HEAD)
        if [[ -n $(git -C "$source" status --porcelain --untracked-files=normal) ]]; then
            revision="$revision-dirty"
        fi
        printf '%s\n' "$revision"
    else
        printf 'unknown\n'
    fi
}

rust_revision=${RUST_REVISION:-$(git_revision "$rust_source")}
cpp_revision=${CPP_REVISION:-$(git_revision "$cpp_source")}

if [[ "$mode" == qualification ]]; then
    [[ "$rust_profile_required" == release-lto ]] ||
        qualification_fail "qualification policy has an unsupported Rust build profile"
    [[ "$cpp_revision" == "$cpp_revision_required" ]] ||
        qualification_fail "C++ source revision $cpp_revision differs from the locked value $cpp_revision_required"
    [[ "$rust_revision" =~ ^([0-9a-f]{40}|[0-9a-f]{64})$ ]] ||
        qualification_fail "qualification requires a clean Rust source revision"
    [[ "$cpp_revision" =~ ^([0-9a-f]{40}|[0-9a-f]{64})$ ]] ||
        qualification_fail "qualification requires a clean C++ source revision"

    cmake_bin=$(command -v "$cmake_bin") ||
        qualification_fail "qualification requires a CMake executable"
    # The selected C++ binary/cache is used only to locate the pinned clean
    # source checkout. Qualification configures a new build directory so no
    # stale object, copied executable, or unrecorded cache flag can survive.
    cpp_build_dir=$(mktemp -d "${TMPDIR:-/tmp}/hyperflint-qualification-build.XXXXXXXX")
    cpp_cache="$cpp_build_dir/CMakeCache.txt"
    cpp_bin="$cpp_build_dir/hyperflint"

    cpp_build_command="$cmake_bin -S $cpp_source -B $cpp_build_dir [locked release-portable cache values] && $cmake_bin --build $cpp_build_dir --target hyperflint-cli --clean-first"
    "$cmake_bin" -S "$cpp_source" -B "$cpp_build_dir" \
        -G "$cpp_generator_required" \
        "-DCMAKE_BUILD_TYPE=$cpp_build_type_required" \
        "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON" \
        "-DCMAKE_CXX_FLAGS=$cpp_cxx_flags_required" \
        "-DCMAKE_CXX_FLAGS_RELEASE=$cpp_cxx_flags_release_required" \
        "-DHF_BUILD_VARIANT=$cpp_profile_required" \
        "-DHF_MIMALLOC=$cpp_mimalloc_required" \
        "-DHF_OPENMP=$cpp_openmp_required" \
        "-DHF_ASAN=$cpp_asan_required" \
        "-DHF_TSAN=$cpp_tsan_required" \
        "-DHF_CLI_STATIC_DEPS=$cpp_static_deps_required"
    "$cmake_bin" --build "$cpp_build_dir" --target hyperflint-cli --clean-first
    cpp_bin=$(resolve_executable "$cpp_bin") ||
        qualification_fail "fresh C++ hyperflint-cli build output is unavailable"

    # Re-read the cache produced by that configure step; pre-existing cache
    # labels are never accepted as evidence for the newly built executable.
    cpp_source=$(realpath "$(cmake_value CMAKE_HOME_DIRECTORY)")
    cpp_build_type=$(cmake_value CMAKE_BUILD_TYPE)
    cpp_profile=$(cmake_value HF_BUILD_VARIANT)
    cpp_mimalloc=$(cmake_value HF_MIMALLOC)
    cpp_openmp=$(cmake_value HF_OPENMP)
    cpp_generator=$(cmake_value CMAKE_GENERATOR)
    cpp_cxx_flags=$(cmake_value CMAKE_CXX_FLAGS)
    cpp_cxx_flags_release=$(cmake_value CMAKE_CXX_FLAGS_RELEASE)
    cpp_asan=$(cmake_value HF_ASAN)
    cpp_tsan=$(cmake_value HF_TSAN)
    cpp_static_deps=$(cmake_value HF_CLI_STATIC_DEPS)
    [[ "$cpp_profile" == "$cpp_profile_required" ]] ||
        qualification_fail "C++ HF_BUILD_VARIANT=$cpp_profile differs from the locked value $cpp_profile_required"
    [[ "$cpp_build_type" == "$cpp_build_type_required" ]] ||
        qualification_fail "C++ CMAKE_BUILD_TYPE=$cpp_build_type differs from the locked value $cpp_build_type_required"
    [[ "$cpp_mimalloc" == "$cpp_mimalloc_required" ]] ||
        qualification_fail "C++ HF_MIMALLOC=$cpp_mimalloc differs from the locked value $cpp_mimalloc_required"
    [[ "$cpp_openmp" == "$cpp_openmp_required" ]] ||
        qualification_fail "C++ HF_OPENMP=$cpp_openmp differs from the locked value $cpp_openmp_required"
    [[ "$cpp_generator" == "$cpp_generator_required" ]] ||
        qualification_fail "C++ generator differs from the locked value $cpp_generator_required"
    [[ "$cpp_cxx_flags" == "$cpp_cxx_flags_required" ]] ||
        qualification_fail "C++ base flags differ from the locked value"
    [[ "$cpp_cxx_flags_release" == "$cpp_cxx_flags_release_required" ]] ||
        qualification_fail "C++ release flags differ from the locked value"
    [[ "$cpp_asan" == "$cpp_asan_required" && "$cpp_tsan" == "$cpp_tsan_required" ]] ||
        qualification_fail "C++ sanitizer settings differ from the locked values"
    [[ "$cpp_static_deps" == "$cpp_static_deps_required" ]] ||
        qualification_fail "C++ CLI static-dependency setting differs from the locked value"
    cpp_version=$({ "$cpp_bin" --version; } 2>&1) ||
        qualification_fail "newly built C++ binary does not report its build stamp"
    grep -F "HF_BUILD_VARIANT: $cpp_profile_required" <<<"$cpp_version" >/dev/null ||
        qualification_fail "C++ binary's compiled build-variant stamp is not release-portable"
fi

rust_sha256=$(sha256sum "$rust_bin" | awk '{print $1}')
cpp_sha256=$(sha256sum "$cpp_bin" | awk '{print $1}')
if [[ -f "$cpp_cache" ]]; then
    cpp_cache_sha256=$(sha256sum "$cpp_cache" | awk '{print $1}')
else
    cpp_cache_sha256=unknown
fi
if [[ "$mode" == qualification ]]; then
    [[ "$rust_bin" != "$cpp_bin" && "$rust_sha256" != "$cpp_sha256" ]] ||
        qualification_fail "qualification requires distinct Rust and C++ backend binaries"
fi

timestamp_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)
run_id=${RUN_ID:-$(date -u +%Y%m%dT%H%M%S)-$$}
if [[ -z "$evidence_dir" ]]; then
    if [[ -n "$results_file" ]]; then
        evidence_dir="$results_file.evidence"
    else
        evidence_dir="$repo_root/target/benchmark-results/$run_id"
    fi
fi
if [[ -e "$evidence_dir" ]]; then
    fail "evidence directory already exists: $evidence_dir"
fi
mkdir -p -- "$evidence_dir"
evidence_dir=$(realpath "$evidence_dir")
results_file=${results_file:-"$evidence_dir/summary.csv"}
samples_csv=${samples_csv:-"$evidence_dir/samples.csv"}
metadata_file=${metadata_file:-"$evidence_dir/metadata.json"}
samples_json="$evidence_dir/samples.json"
analysis_input="$evidence_dir/analysis-input.json"
analysis_file="$evidence_dir/analysis.json"
qualification_file="$evidence_dir/qualification.json"
correctness_file="$evidence_dir/correctness.json"
rust_manifest="$evidence_dir/rust-build.json"
cpp_manifest="$evidence_dir/cpp-build.json"
mkdir -p -- "$(dirname -- "$results_file")" "$(dirname -- "$samples_csv")" "$(dirname -- "$metadata_file")"

declare -a canonical_artifacts=()
for artifact in "$results_file" "$samples_csv" "$metadata_file" "$samples_json" \
    "$analysis_input" "$analysis_file" "$qualification_file" "$correctness_file" \
    "$rust_manifest" "$cpp_manifest"; do
    [[ ! -e "$artifact" && ! -L "$artifact" ]] ||
        fail "refusing to overwrite existing evidence artifact: $artifact"
    artifact_parent=$(realpath "$(dirname -- "$artifact")")
    canonical_artifact="$artifact_parent/$(basename -- "$artifact")"
    for existing_artifact in "${canonical_artifacts[@]}"; do
        [[ "$canonical_artifact" != "$existing_artifact" ]] ||
            fail "evidence artifact paths must be distinct: $artifact"
    done
    canonical_artifacts+=("$canonical_artifact")
done

rust_manifest_args=(
    --arg source "$rust_source"
    --arg revision "$rust_revision"
    --arg binary "$rust_bin"
    --arg binary_sha256 "$rust_sha256"
    --arg profile release-lto
    --arg command "$rust_build_command"
    --arg rustc "$(rustc --version 2>/dev/null || printf unavailable)"
    --arg cargo "$("$cargo_bin" --version 2>/dev/null || printf unavailable)"
    --argjson built_by_driver "$build_rust"
)
jq -n "${rust_manifest_args[@]}" '{schema:1,source:$source,source_revision:$revision,binary:$binary,
      binary_sha256:$binary_sha256,profile:$profile,command:$command,
      built_by_driver:($built_by_driver == 1),tools:{rustc:$rustc,cargo:$cargo}}' >"$rust_manifest"
cpp_manifest_args=(
    --arg source "$cpp_source"
    --arg revision "$cpp_revision"
    --arg binary "$cpp_bin"
    --arg binary_sha256 "$cpp_sha256"
    --arg profile "$cpp_profile"
    --arg command "$cpp_build_command"
    --argjson built_by_driver "$build_cpp"
    --arg cache "$cpp_cache"
    --arg cache_sha256 "$cpp_cache_sha256"
    --arg build_type "$cpp_build_type"
    --arg mimalloc "$cpp_mimalloc"
    --arg openmp "$cpp_openmp"
    --arg generator "$cpp_generator"
    --arg cxx_flags "$cpp_cxx_flags"
    --arg cxx_flags_release "$cpp_cxx_flags_release"
    --arg asan "$cpp_asan"
    --arg tsan "$cpp_tsan"
    --arg cli_static_deps "$cpp_static_deps"
    --arg binary_version "$cpp_version"
)
jq -n "${cpp_manifest_args[@]}" '{schema:1,source:$source,source_revision:$revision,binary:$binary,
      binary_sha256:$binary_sha256,profile:$profile,command:$command,
      built_by_driver:($built_by_driver == 1),binary_version:$binary_version,
      cmake:{cache:$cache,cache_sha256:$cache_sha256,build_type:$build_type,
             mimalloc:$mimalloc,openmp:$openmp,generator:$generator,
             cxx_flags:$cxx_flags,cxx_flags_release:$cxx_flags_release,
             asan:$asan,tsan:$tsan,cli_static_deps:$cli_static_deps}}' >"$cpp_manifest"
rust_manifest_sha256=$(sha256sum "$rust_manifest" | awk '{print $1}')
cpp_manifest_sha256=$(sha256sum "$cpp_manifest" | awk '{print $1}')

declare -a names=() requests=() compares=() ignored=() ignored_recursive=()
declare -a permutation_fields=() permutation_values=() timeouts=()
declare -a pair_counts=() expected_paths=()
while IFS= read -r fixture || [[ -n "$fixture" ]]; do
    [[ -z "$fixture" ]] && continue
    name=$(jq -er '.name' <<<"$fixture")
    fixture_tier=$(jq -r '.tier // "qualification"' <<<"$fixture")
    selected=0
    if [[ "$mode" == qualification ]]; then
        if jq -e --arg name "$name" 'index($name) != null' <<<"$qualification_names" >/dev/null; then
            selected=1
        fi
    elif [[ "$tier" == all || "$fixture_tier" == "$tier" ]]; then
        selected=1
    fi
    [[ "$selected" == 1 ]] || continue

    fixture_pairs=$(jq -r '.pairs // empty' <<<"$fixture")
    if [[ "$mode" == qualification && -n "$fixture_pairs" &&
        "$fixture_pairs" != "$pairs" ]]; then
        fail "fixture '$name' overrides the locked pair count"
    fi
    index=${#names[@]}
    names[index]=$name
    requests[index]=$(jq -c '.request' <<<"$fixture")
    compares[index]=$(jq -r '.compare // "byte"' <<<"$fixture")
    ignored[index]=$(jq -c '.ignore // []' <<<"$fixture")
    ignored_recursive[index]=$(jq -c '.ignore_recursive // []' <<<"$fixture")
    permutation_fields[index]=$(jq -r '.permutation_field // ""' <<<"$fixture")
    permutation_values[index]=$(jq -c '.permutation_values // []' <<<"$fixture")
    timeouts[index]=$(jq -r --argjson fallback "$default_timeout" '.timeout_seconds // $fallback' <<<"$fixture")
    pair_counts[index]=${fixture_pairs:-$pairs}
done <"$workloads"

((${#names[@]} > 0)) || fail "no benchmark workloads selected"
selected_names=$(printf '%s\n' "${names[@]}" | jq -R . | jq -sc 'sort')
if [[ "$mode" == qualification && "$selected_names" != "$qualification_names" ]]; then
    fail "selected workloads do not exactly match the qualification corpus"
fi

environment_allowlist=$(jq -c '.requirements.environment_allowlist' "$policy")
performance_environment=$(jq -cn --arg threads "$threads" '{OMP_NUM_THREADS:$threads,OPENBLAS_NUM_THREADS:$threads,
      RAYON_NUM_THREADS:$threads,SYMBOLICA_HIDE_BANNER:"1"}')
host_name=$(hostname 2>/dev/null || uname -n)
cpu_model=$(awk -F: '/^model name[[:space:]]*:/ {
    sub(/^[[:space:]]+/, "", $2); print $2; exit
}' /proc/cpuinfo 2>/dev/null || true)
cpu_model=${cpu_model:-unknown}
cpu_governor=unknown
if [[ -r /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor ]]; then
    cpu_governor=$(</sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)
fi

metadata_args=(
    --arg mode "$mode"
    --arg started_at_utc "$timestamp_utc"
    --arg run_id "$run_id"
    --arg policy_id "$policy_id"
    --arg policy_path "$policy"
    --arg policy_sha256 "$policy_sha256"
    --arg corpus_path "$corpus"
    --arg corpus_sha256 "$corpus_sha256"
    --arg fixture_path "$workloads"
    --arg fixture_sha256 "$fixture_sha256"
    --argjson workloads "$selected_names"
    --argjson pairs "$pairs"
    --argjson warmup "$warmup"
    --argjson threads "$threads"
    --argjson bootstrap "$bootstrap_samples"
    --argjson seed "$seed"
    --argjson thresholds "$thresholds"
    --arg cpuset "$cpuset"
    --argjson environment_allowlist "$environment_allowlist"
    --argjson performance_environment "$performance_environment"
    --arg host "$host_name"
    --arg os "$(uname -srm)"
    --arg cpu_model "$cpu_model"
    --arg cpu_governor "$cpu_governor"
    --arg rust_binary "$rust_bin"
    --arg rust_sha256 "$rust_sha256"
    --arg rust_source "$rust_source"
    --arg rust_revision "$rust_revision"
    --arg rust_command "$rust_build_command"
    --arg rust_manifest "$rust_manifest"
    --arg rust_manifest_sha256 "$rust_manifest_sha256"
    --argjson rust_built_by_driver "$build_rust"
    --arg cpp_binary "$cpp_bin"
    --arg cpp_sha256 "$cpp_sha256"
    --arg cpp_source "$cpp_source"
    --arg cpp_revision "$cpp_revision"
    --arg cpp_profile "$cpp_profile"
    --arg cpp_command "$cpp_build_command"
    --argjson cpp_built_by_driver "$build_cpp"
    --arg cpp_manifest "$cpp_manifest"
    --arg cpp_manifest_sha256 "$cpp_manifest_sha256"
    --arg cpp_cache "$cpp_cache"
    --arg cpp_cache_sha256 "$cpp_cache_sha256"
    --arg cpp_build_type "$cpp_build_type"
    --arg cpp_mimalloc "$cpp_mimalloc"
    --arg cpp_openmp "$cpp_openmp"
    --arg cpp_generator "$cpp_generator"
    --arg cpp_cxx_flags "$cpp_cxx_flags"
    --arg cpp_cxx_flags_release "$cpp_cxx_flags_release"
    --arg cpp_asan "$cpp_asan"
    --arg cpp_tsan "$cpp_tsan"
    --arg cpp_static_deps "$cpp_static_deps"
    --arg cpp_binary_version "$cpp_version"
    --arg summary "$results_file"
    --arg samples_csv "$samples_csv"
    --arg samples_json "$samples_json"
    --arg analysis "$analysis_file"
    --arg qualification "$qualification_file"
)
jq -n "${metadata_args[@]}" '{
      evidence_schema:2,mode:$mode,status:"running",
      started_at_utc:$started_at_utc,run_id:$run_id,
      policy:{id:$policy_id,path:$policy_path,sha256:$policy_sha256},
      corpus:{path:$corpus_path,sha256:$corpus_sha256,
              fixture_path:$fixture_path,fixture_sha256:$fixture_sha256,
              workloads:$workloads},
      configuration:{pairs_per_workload:$pairs,warmup_runs:$warmup,
                     threads:$threads,bootstrap_samples:$bootstrap,seed:$seed,
                     thresholds:$thresholds},
      measurement:{clock:"perf_counter_ns",rusage:"wait4",
                   order:"paired_interleaved",samples_include_process_startup:true,
                   affinity_scope:"measurement_helper_and_backend",
                   cpuset:$cpuset,environment_sanitized:true,
                   environment_allowlist:$environment_allowlist,
                   performance_environment:$performance_environment},
      host:{name:$host,os:$os,cpu_model:$cpu_model,cpu_governor:$cpu_governor},
      rust:{binary:$rust_binary,binary_sha256:$rust_sha256,source:$rust_source,
            revision:$rust_revision,
            build:{source_revision:$rust_revision,profile:"release-lto",
                   command:$rust_command,manifest:$rust_manifest,
                   manifest_sha256:$rust_manifest_sha256,
                   built_by_driver:($rust_built_by_driver == 1)}},
      cpp_oracle:{binary:$cpp_binary,binary_sha256:$cpp_sha256,source:$cpp_source,
                  revision:$cpp_revision,
                  build:{source_revision:$cpp_revision,profile:$cpp_profile,
                         command:$cpp_command,manifest:$cpp_manifest,
                         manifest_sha256:$cpp_manifest_sha256,
                         built_by_driver:($cpp_built_by_driver == 1),
                         binary_version:$cpp_binary_version,
                         cmake:{cache:$cpp_cache,cache_sha256:$cpp_cache_sha256,
                                build_type:$cpp_build_type,mimalloc:$cpp_mimalloc,
                                openmp:$cpp_openmp,generator:$cpp_generator,
                                cxx_flags:$cpp_cxx_flags,
                                cxx_flags_release:$cpp_cxx_flags_release,
                                asan:$cpp_asan,tsan:$cpp_tsan,
                                cli_static_deps:$cpp_static_deps}}},
      artifacts:{summary_csv:$summary,samples_csv:$samples_csv,
                 samples_json:$samples_json,analysis_json:$analysis,
                 qualification_json:$qualification},
      commands:{measurement:"benchmark_process.py --clear-env ...",
                statistics:"benchmark_stats.py",
                policy_verification:"benchmark_policy.py"}
    }' >"$metadata_file"

scratch=$(mktemp -d "${TMPDIR:-/tmp}/hyperbolica-benchmark.XXXXXXXX")
metadata_finalized=0
cleanup() {
    local status=$?
    rm -rf -- "$scratch"
    if [[ "$metadata_finalized" == 0 && -f "$metadata_file" ]]; then
        jq --arg completed "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --argjson exit_code "$status" '.status = "failed" | .completed_at_utc = $completed |
             .exit_code = $exit_code' "$metadata_file" >"$metadata_file.tmp" &&
            mv -- "$metadata_file.tmp" "$metadata_file"
    fi
}
trap cleanup EXIT

validate_success() {
    local output=$1
    jq -e -s 'length == 1 and (.[0] | type == "object" and
        (has("error") | not) and ((.failed // false) | not) and
        ((.divergent // false) | not))' "$output" >/dev/null 2>&1
}

run_serial=0
last_output=
last_error=
last_metrics=
run_backend() {
    local executable=$1
    local request=$2
    local timeout_seconds=$3
    local label=$4
    local -a command=("$executable" eval-json)
    local -a measure_args measure_command
    run_serial=$((run_serial + 1))
    last_output="$scratch/$run_serial.$label.stdout"
    last_error="$scratch/$run_serial.$label.stderr"
    last_metrics="$scratch/$run_serial.$label.metrics.json"
    measure_args=(
        --stdout "$last_output"
        --stderr "$last_error"
        --timeout-seconds "$timeout_seconds"
        --cwd "$repo_root"
        --env "OMP_NUM_THREADS=$threads"
        --env "OPENBLAS_NUM_THREADS=$threads"
        --env "RAYON_NUM_THREADS=$threads"
        --env SYMBOLICA_HIDE_BANNER=1
        --clear-env --
    )
    measure_command=(
        "$python_bin" "$repo_root/scripts/benchmark_process.py"
        "${measure_args[@]}" "${command[@]}"
    )
    if [[ -n "$cpuset" ]]; then
        # Pin the helper, not just its child: perf_counter starts around Popen,
        # so the full measured interval and the inherited backend affinity must
        # share the locked CPU set.
        measure_command=("$taskset_bin" -c "$cpuset" "${measure_command[@]}")
    fi
    if ! printf '%s\n' "$request" |
        "${measure_command[@]}" >"$last_metrics"; then
        fail "measurement helper failed for $label" 1
    fi
    if ! jq -e '.timed_out == false and .exit_code == 0' "$last_metrics" >/dev/null; then
        printf 'benchmark-compare: backend process failed for %s\n' "$label" >&2
        jq . "$last_metrics" >&2 || true
        sed -n '1,30p' "$last_error" >&2 || true
        exit 1
    fi
    if ! validate_success "$last_output"; then
        printf 'benchmark-compare: invalid/error response for %s\n' "$label" >&2
        sed -n '1,30p' "$last_error" >&2 || true
        jq . "$last_output" >&2 || true
        exit 1
    fi
}

assert_fixture_output() {
    local index=$1
    local output=$2
    local label=$3
    local canonical="$scratch/$run_serial.canonical"
    if ! hf_validate_permutation "$output" "${permutation_fields[index]}" "${permutation_values[index]}"; then
        fail "$label did not return the required permutation" 1
    fi
    hf_canonical_response "$output" "${compares[index]}" "${ignored[index]}" "${ignored_recursive[index]}" >"$canonical"
    if ! cmp -s "$canonical" "${expected_paths[index]}"; then
        printf 'benchmark-compare: nondeterministic output for %s\n' "$label" >&2
        diff -u "${expected_paths[index]}" "$canonical" >&2 || true
        exit 1
    fi
}

correctness_ndjson="$scratch/correctness.ndjson"
: >"$correctness_ndjson"
printf 'correctness preflight (%d workloads)\n' "${#names[@]}"
for index in "${!names[@]}"; do
    name=${names[index]}
    run_backend "$cpp_bin" "${requests[index]}" "${timeouts[index]}" "preflight.$name.cpp"
    cpp_output=$last_output
    run_backend "$rust_bin" "${requests[index]}" "${timeouts[index]}" "preflight.$name.rust"
    rust_output=$last_output
    for output in "$cpp_output" "$rust_output"; do
        if ! hf_validate_permutation "$output" "${permutation_fields[index]}" "${permutation_values[index]}"; then
            fail "'$name' did not return the required permutation" 1
        fi
    done
    cpp_canonical="$scratch/preflight.$index.cpp.canonical"
    rust_canonical="$scratch/preflight.$index.rust.canonical"
    hf_canonical_response "$cpp_output" "${compares[index]}" "${ignored[index]}" "${ignored_recursive[index]}" >"$cpp_canonical"
    hf_canonical_response "$rust_output" "${compares[index]}" "${ignored[index]}" "${ignored_recursive[index]}" >"$rust_canonical"
    if ! cmp -s "$cpp_canonical" "$rust_canonical"; then
        printf "benchmark-compare: refusing to time '%s': responses differ\n" "$name" >&2
        diff -u "$cpp_canonical" "$rust_canonical" >&2 || true
        exit 1
    fi
    expected_paths[index]=$cpp_canonical
    jq -cn --arg name "$name" --arg compare "${compares[index]}" --arg sha256 "$(sha256sum "$cpp_canonical" | awk '{print $1}')" '{name:$name,compare:$compare,canonical_response_sha256:$sha256}' >>"$correctness_ndjson"
    printf '  [equal] %s\n' "$name"
done
jq -s '{schema:1,status:"pass",workloads:.}' "$correctness_ndjson" >"$correctness_file"

schedule_indices() {
    local phase=$1
    local round=$2
    local index key
    for index in "${!names[@]}"; do
        key=$(printf '%s\0%s\0%s\0%s' "$seed" "$phase" "$round" "$index" |
            sha256sum | awk '{print $1}')
        printf '%s %s\n' "$key" "$index"
    done | sort -k1,1 | awk '{print $2}'
}

run_checked() {
    local index=$1
    local backend=$2
    local phase=$3
    local executable
    if [[ "$backend" == rust ]]; then
        executable=$rust_bin
    else
        executable=$cpp_bin
    fi
    run_backend "$executable" "${requests[index]}" "${timeouts[index]}" "$phase.${names[index]}.$backend"
    assert_fixture_output "$index" "$last_output" "$phase/${names[index]}/$backend"
}

for ((round = 0; round < warmup; round++)); do
    while IFS= read -r index; do
        if ((((round + index)) % 2 == 0)); then
            first=cpp
            second=rust
        else
            first=rust
            second=cpp
        fi
        run_checked "$index" "$first" "warmup.$round"
        run_checked "$index" "$second" "warmup.$round"
    done < <(schedule_indices warmup "$round")
done

printf 'workload,pair,position,backend,elapsed_ns,user_ns,sys_ns,max_rss_bytes\n' >"$samples_csv"
samples_ndjson="$scratch/samples.ndjson"
: >"$samples_ndjson"
max_pairs=0
for count in "${pair_counts[@]}"; do
    ((count > max_pairs)) && max_pairs=$count
done

printf 'paired measurement (pairs=%d warmup=%d threads=%d mode=%s)\n' "$pairs" "$warmup" "$threads" "$mode"
for ((pair = 0; pair < max_pairs; pair++)); do
    while IFS= read -r index; do
        ((pair < pair_counts[index])) || continue
        if ((((pair + index)) % 2 == 0)); then
            first=cpp
            second=rust
        else
            first=rust
            second=cpp
        fi
        position=0
        for backend in "$first" "$second"; do
            position=$((position + 1))
            run_checked "$index" "$backend" "sample.$pair.$position"
            elapsed_ns=$(jq -er '.elapsed_ns' "$last_metrics")
            user_ns=$(jq -er '.user_ns' "$last_metrics")
            sys_ns=$(jq -er '.sys_ns' "$last_metrics")
            max_rss_bytes=$(jq -er '.max_rss_bytes' "$last_metrics")
            printf '%s,%d,%d,%s,%s,%s,%s,%s\n' "${names[index]}" "$((pair + 1))" "$position" "$backend" "$elapsed_ns" "$user_ns" "$sys_ns" "$max_rss_bytes" >>"$samples_csv"
            sample_args=(
                --arg workload "${names[index]}"
                --argjson pair "$((pair + 1))"
                --argjson position "$position"
                --arg backend "$backend"
                --argjson elapsed_ns "$elapsed_ns"
                --argjson user_ns "$user_ns"
                --argjson sys_ns "$sys_ns"
                --argjson max_rss_bytes "$max_rss_bytes"
            )
            jq -cn "${sample_args[@]}" '{workload:$workload,pair:$pair,position:$position,
                  backend:$backend,elapsed_ns:$elapsed_ns,user_ns:$user_ns,
                  sys_ns:$sys_ns,max_rss_bytes:$max_rss_bytes}' >>"$samples_ndjson"
        done
    done < <(schedule_indices sample "$pair")
done

jq -s . "$samples_ndjson" >"$samples_json"
jq --argjson thresholds "$thresholds" --argjson bootstrap "$bootstrap_samples" --argjson seed "$seed" '{samples:.,thresholds:$thresholds,
      bootstrap_samples:$bootstrap,seed:$seed}' "$samples_json" >"$analysis_input"
if "$python_bin" "$repo_root/scripts/benchmark_stats.py" --input "$analysis_input" --output "$analysis_file" --pretty; then
    statistics_status=0
else
    statistics_status=$?
fi
((statistics_status <= 1)) ||
    fail "statistical analysis rejected the evidence"

printf 'workload,cpp_median_ms,rust_median_ms,rust_over_cpp,status\n' >"$results_file"
jq -r --argjson tail "$(jq '.max_workload_ratio' <<<"$thresholds")" '.workloads | to_entries[] |
     [.key, (.value.median_elapsed_ns.cpp / 1000000),
      (.value.median_elapsed_ns.rust / 1000000),
      .value.geometric_mean_ratio,
      (if .value.geometric_mean_ratio <= $tail then "ok" else "REGRESSION" end)] |
     @csv' "$analysis_file" >>"$results_file"
jq -r '.workloads | to_entries[] |
    "\(.key): cpp=\(.value.median_elapsed_ns.cpp / 1000000)ms " +
    "rust=\(.value.median_elapsed_ns.rust / 1000000)ms " +
    "ratio=\(.value.geometric_mean_ratio)"' "$analysis_file"

samples_sha256=$(sha256sum "$samples_json" | awk '{print $1}')
samples_csv_sha256=$(sha256sum "$samples_csv" | awk '{print $1}')
summary_sha256=$(sha256sum "$results_file" | awk '{print $1}')
analysis_sha256=$(sha256sum "$analysis_file" | awk '{print $1}')
correctness_sha256=$(sha256sum "$correctness_file" | awk '{print $1}')
run_status=passed
((statistics_status == 0)) || run_status=failed
final_metadata_args=(
    --slurpfile analysis "$analysis_file"
    --arg status "$run_status"
    --arg completed "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    --arg samples_sha256 "$samples_sha256"
    --arg samples_csv_sha256 "$samples_csv_sha256"
    --arg summary_sha256 "$summary_sha256"
    --arg analysis_sha256 "$analysis_sha256"
    --arg correctness "$correctness_file"
    --arg correctness_sha256 "$correctness_sha256"
)
jq "${final_metadata_args[@]}" '.status = $status | .completed_at_utc = $completed |
     .exit_code = (if $status == "passed" then 0 else 1 end) |
     .analysis = $analysis[0] |
     .artifacts.samples_sha256 = $samples_sha256 |
     .artifacts.samples_csv_sha256 = $samples_csv_sha256 |
     .artifacts.summary_sha256 = $summary_sha256 |
     .artifacts.analysis_sha256 = $analysis_sha256 |
     .artifacts.correctness_json = $correctness |
     .artifacts.correctness_sha256 = $correctness_sha256' "$metadata_file" >"$metadata_file.tmp"
mv -- "$metadata_file.tmp" "$metadata_file"
metadata_finalized=1

policy_args=(
    --metadata "$metadata_file"
    --policy "$policy"
    --corpus "$corpus"
    --analysis "$analysis_file"
    --mode "$mode"
    --output "$qualification_file"
    --pretty
)
if "$python_bin" "$repo_root/scripts/benchmark_policy.py" "${policy_args[@]}"; then
    policy_status=0
else
    policy_status=$?
fi
((policy_status <= 1)) ||
    fail "policy verifier rejected the evidence document"

printf 'benchmark evidence: %s\n' "$evidence_dir"
if [[ "$mode" == qualification ]]; then
    jq -r '"qualification: " + .status' "$qualification_file"
else
    jq -r '"qualification: exploratory only (would_qualify=" +
        (.would_qualify | tostring) + ")"' "$qualification_file"
fi

if ((statistics_status != 0 || policy_status != 0)); then
    exit 1
fi

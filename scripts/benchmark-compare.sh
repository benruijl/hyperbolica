#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
workloads=${BENCH_WORKLOADS:-"$repo_root/tests/fixtures/benchmark.jsonl"}
rust_bin=${HYPERFLINT_RUST:-"$repo_root/target/release/hyperflint"}
cpp_bin=${HYPERFLINT_CPP:-/tmp/hyperflint-cpp-build/hyperflint}
cargo_bin=${CARGO:-cargo}
build_rust=${BUILD_RUST:-1}
warmup=${WARMUP:-2}
iterations=${ITERATIONS:-7}
threads=${THREADS:-1}
max_slowdown=${MAX_SLOWDOWN:-1.20}
regression_tolerance_ms=${REGRESSION_TOLERANCE_MS:-2.0}
fail_on_regression=${FAIL_ON_REGRESSION:-1}
timeout_seconds=${TIMEOUT_SECONDS:-300}
results_file=${RESULTS_FILE:-}
cpuset=${CPUSET:-}
evidence_dir=${EVIDENCE_DIR:-}
raw_samples_file=${RAW_SAMPLES_FILE:-}
metadata_file=${METADATA_FILE:-}
rust_source=${HYPERBOLICA_SOURCE:-$repo_root}
cpp_source=${HYPERFLINT_CPP_SOURCE:-/tmp/subtropica-hyperbolica}
rust_revision_override=${RUST_REVISION:-}
cpp_revision_override=${CPP_REVISION:-}

for tool in jq awk sort date mktemp cmp diff; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "benchmark-compare: required tool '$tool' is unavailable" >&2
        exit 2
    }
done
[[ "$warmup" =~ ^[0-9]+$ && "$iterations" =~ ^[1-9][0-9]*$ ]] || {
    echo "benchmark-compare: WARMUP must be non-negative and ITERATIONS positive" >&2
    exit 2
}
[[ "$threads" =~ ^[1-9][0-9]*$ ]] || {
    echo "benchmark-compare: THREADS must be positive" >&2
    exit 2
}
[[ "$build_rust" =~ ^[01]$ && "$fail_on_regression" =~ ^[01]$ ]] || {
    echo "benchmark-compare: BUILD_RUST and FAIL_ON_REGRESSION must be 0 or 1" >&2
    exit 2
}
[[ "$max_slowdown" =~ ^[0-9]+([.][0-9]+)?$ && \
    "$regression_tolerance_ms" =~ ^[0-9]+([.][0-9]+)?$ ]] || {
    echo "benchmark-compare: regression thresholds must be non-negative numbers" >&2
    exit 2
}
[[ "$timeout_seconds" =~ ^[1-9][0-9]*$ ]] || {
    echo "benchmark-compare: TIMEOUT_SECONDS must be positive" >&2
    exit 2
}
if [[ -n "$cpuset" ]] && ! command -v taskset >/dev/null 2>&1; then
    echo "benchmark-compare: CPUSET requires taskset" >&2
    exit 2
fi

[[ -f "$workloads" ]] || {
    echo "benchmark-compare: workload file not found: $workloads" >&2
    exit 2
}
BENCH_WORKLOADS="$workloads" "$repo_root/scripts/lint-fixtures.sh"

if [[ "$build_rust" == 1 ]]; then
    "$cargo_bin" build --release --bin hyperflint
fi
for executable in "$rust_bin" "$cpp_bin"; do
    [[ -x "$executable" ]] || {
        echo "benchmark-compare: executable not found: $executable" >&2
        exit 2
    }
done

timestamp_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)
run_id=${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}
if [[ -z "$evidence_dir" ]]; then
    if [[ -n "$results_file" ]]; then
        evidence_dir="$results_file.evidence"
    else
        evidence_dir="$repo_root/target/benchmark-results/$run_id"
    fi
fi
results_file=${results_file:-$evidence_dir/summary.csv}
raw_samples_file=${raw_samples_file:-$evidence_dir/samples.csv}
metadata_file=${metadata_file:-$evidence_dir/metadata.json}
if [[ "$results_file" == "$raw_samples_file" || "$results_file" == "$metadata_file" ||
    "$raw_samples_file" == "$metadata_file" ]]; then
    echo "benchmark-compare: summary, samples, and metadata paths must be distinct" >&2
    exit 2
fi
mkdir -p -- "$evidence_dir" "$(dirname -- "$results_file")" \
    "$(dirname -- "$raw_samples_file")" "$(dirname -- "$metadata_file")"

if command -v sha256sum >/dev/null 2>&1; then
    sha256_file() {
        sha256sum "$1" | awk '{ print $1 }'
    }
elif command -v shasum >/dev/null 2>&1; then
    sha256_file() {
        shasum -a 256 "$1" | awk '{ print $1 }'
    }
else
    echo "benchmark-compare: sha256sum or shasum is required for evidence" >&2
    exit 2
fi

timer_probe=$(date +%s%N)
if [[ "$timer_probe" =~ ^[0-9]+$ ]]; then
    timer_backend=date
    now_ns() {
        date +%s%N
    }
elif command -v python3 >/dev/null 2>&1; then
    timer_backend=python3
    now_ns() {
        python3 -c 'import time; print(time.time_ns())'
    }
elif command -v perl >/dev/null 2>&1; then
    timer_backend=perl
    now_ns() {
        perl -MTime::HiRes=time -e 'printf "%.0f\n", time() * 1_000_000_000'
    }
else
    echo "benchmark-compare: this date lacks nanoseconds; python3 or perl is required" >&2
    exit 2
fi

git_revision() {
    local source=$1
    local override=$2
    local revision
    if [[ -n "$override" ]]; then
        printf '%s\n' "$override"
        return
    fi
    if [[ -d "$source" ]] && git -C "$source" rev-parse --is-inside-work-tree \
        >/dev/null 2>&1; then
        revision=$(git -C "$source" rev-parse HEAD)
        if [[ -n $(git -C "$source" status --porcelain --untracked-files=normal) ]]; then
            revision="$revision-dirty"
        fi
        printf '%s\n' "$revision"
    else
        printf 'unknown\n'
    fi
}

cpu_model=unknown
if [[ -r /proc/cpuinfo ]]; then
    cpu_model=$(awk -F: '/^model name[[:space:]]*:/ {
        sub(/^[[:space:]]+/, "", $2); print $2; exit
    }' /proc/cpuinfo)
elif command -v sysctl >/dev/null 2>&1; then
    cpu_model=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || printf 'unknown')
fi
cpu_model=${cpu_model:-unknown}
logical_cpus=$(getconf _NPROCESSORS_ONLN 2>/dev/null || printf 'unknown')
cpu_governor=unknown
if [[ -r /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor ]]; then
    cpu_governor=$(</sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)
fi
host_name=$(hostname 2>/dev/null || uname -n)
rust_revision=$(git_revision "$rust_source" "$rust_revision_override")
cpp_revision=$(git_revision "$cpp_source" "$cpp_revision_override")
rust_sha256=$(sha256_file "$rust_bin")
cpp_sha256=$(sha256_file "$cpp_bin")
workload_sha256=$(sha256_file "$workloads")
rustc_version=$(rustc --version 2>/dev/null || printf 'unavailable')
cargo_version=$($cargo_bin --version 2>/dev/null || printf 'unavailable')
timeout_tool=none
command_prefix=""
if command -v timeout >/dev/null 2>&1; then
    timeout_tool=timeout
    command_prefix="${command_prefix}timeout $timeout_seconds "
fi
if [[ -n "$cpuset" ]]; then
    command_prefix="${command_prefix}taskset -c $cpuset "
fi
backend_command_template="printf request | env SYMBOLICA_HIDE_BANNER=1 RAYON_NUM_THREADS=$threads OMP_NUM_THREADS=$threads OPENBLAS_NUM_THREADS=$threads ${command_prefix}<executable> eval-json"

metadata_tmp="$metadata_file.tmp.$$"
jq -n \
    --arg status running \
    --arg started_at_utc "$timestamp_utc" \
    --arg run_id "$run_id" \
    --arg host "$host_name" \
    --arg os "$(uname -srm)" \
    --arg cpu_model "$cpu_model" \
    --arg logical_cpus "$logical_cpus" \
    --arg cpu_governor "$cpu_governor" \
    --arg cpuset "$cpuset" \
    --arg timer_backend "$timer_backend" \
    --arg timeout_tool "$timeout_tool" \
    --arg rustc "$rustc_version" \
    --arg cargo "$cargo_version" \
    --arg rust_binary "$rust_bin" \
    --arg cpp_binary "$cpp_bin" \
    --arg rust_binary_sha256 "$rust_sha256" \
    --arg cpp_binary_sha256 "$cpp_sha256" \
    --arg rust_revision "$rust_revision" \
    --arg cpp_revision "$cpp_revision" \
    --arg rust_source "$rust_source" \
    --arg cpp_source "$cpp_source" \
    --arg workloads "$workloads" \
    --arg workload_sha256 "$workload_sha256" \
    --arg summary_csv "$results_file" \
    --arg samples_csv "$raw_samples_file" \
    --arg metadata_json "$metadata_file" \
    --arg build_command "$cargo_bin build --release --bin hyperflint" \
    --arg fixture_lint_command "BENCH_WORKLOADS=$workloads $repo_root/scripts/lint-fixtures.sh" \
    --arg backend_command_template "$backend_command_template" \
    --argjson build_rust "$build_rust" \
    --argjson warmup "$warmup" \
    --argjson iterations "$iterations" \
    --argjson threads "$threads" \
    --argjson timeout_seconds "$timeout_seconds" \
    --argjson max_slowdown "$max_slowdown" \
    --argjson regression_tolerance_ms "$regression_tolerance_ms" \
    --argjson fail_on_regression "$fail_on_regression" \
    '{
        evidence_schema: 1,
        status: $status,
        started_at_utc: $started_at_utc,
        run_id: $run_id,
        host: {
            name: $host,
            os: $os,
            cpu_model: $cpu_model,
            logical_cpus: $logical_cpus,
            cpu_governor: $cpu_governor,
            cpuset: $cpuset
        },
        tools: {
            timer_backend: $timer_backend,
            timeout_tool: $timeout_tool,
            rustc: $rustc,
            cargo: $cargo
        },
        configuration: {
            build_rust: ($build_rust == 1),
            warmup: $warmup,
            iterations: $iterations,
            threads: $threads,
            timeout_seconds: $timeout_seconds,
            max_slowdown: $max_slowdown,
            regression_tolerance_ms: $regression_tolerance_ms,
            fail_on_regression: ($fail_on_regression == 1)
        },
        rust: {
            binary: $rust_binary,
            binary_sha256: $rust_binary_sha256,
            source: $rust_source,
            revision: $rust_revision
        },
        cpp_oracle: {
            binary: $cpp_binary,
            binary_sha256: $cpp_binary_sha256,
            source: $cpp_source,
            revision: $cpp_revision
        },
        workloads: {
            path: $workloads,
            sha256: $workload_sha256
        },
        outputs: {
            summary_csv: $summary_csv,
            samples_csv: $samples_csv,
            metadata_json: $metadata_json
        },
        commands: {
            build: $build_command,
            fixture_lint: $fixture_lint_command,
            backend_template: $backend_command_template,
            correctness_preflight: "canonical JSON equality via jq -S -c before timing"
        }
    }' >"$metadata_tmp"
mv -- "$metadata_tmp" "$metadata_file"

scratch=$(mktemp -d "${TMPDIR:-/tmp}/hyperbolica-bench.XXXXXXXX")
metadata_finalized=0

update_metadata() {
    local status=$1
    local exit_code=$2
    local regression_count=${3:-null}
    local completed_at
    completed_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    jq \
        --arg status "$status" \
        --arg completed_at_utc "$completed_at" \
        --argjson exit_code "$exit_code" \
        --argjson regressions "$regression_count" \
        '.status = $status
         | .completed_at_utc = $completed_at_utc
         | .exit_code = $exit_code
         | .regressions = $regressions' \
        "$metadata_file" >"$metadata_tmp" && mv -- "$metadata_tmp" "$metadata_file"
}

cleanup() {
    local exit_code=$?
    rm -rf -- "$scratch"
    if [[ "$metadata_finalized" == 0 ]]; then
        update_metadata failed "$exit_code" null || true
    fi
}
trap cleanup EXIT

run_backend() {
    local executable=$1
    local request=$2
    local stdout_path=$3
    local stderr_path=$4
    local -a command=("$executable" eval-json)
    if [[ -n "$cpuset" ]]; then
        command=(taskset -c "$cpuset" "${command[@]}")
    fi
    if command -v timeout >/dev/null 2>&1; then
        command=(timeout "$timeout_seconds" "${command[@]}")
    fi
    printf '%s\n' "$request" |
        env \
            SYMBOLICA_HIDE_BANNER=1 \
            RAYON_NUM_THREADS="$threads" \
            OMP_NUM_THREADS="$threads" \
            OPENBLAS_NUM_THREADS="$threads" \
            "${command[@]}" >"$stdout_path" 2>"$stderr_path"
}

validate_success() {
    local output=$1
    jq -e \
        '((has("error") | not) and ((.failed // false) | not) and ((.divergent // false) | not))' \
        "$output" >/dev/null
}

median_ns() {
    sort -n | awk '
        { values[NR] = $1 }
        END {
            if (NR % 2) print values[(NR + 1) / 2];
            else print (values[NR / 2] + values[NR / 2 + 1]) / 2;
        }
    '
}

measure() {
    local executable=$1
    local label=$2
    local request=$3
    local prefix=$4
    local workload=$5
    local backend=$6
    local expected_output=$7
    local iteration start_ns end_ns
    local -a samples=()

    for ((iteration = 0; iteration < warmup; iteration++)); do
        run_backend "$executable" "$request" \
            "$scratch/$prefix.warm.stdout" "$scratch/$prefix.warm.stderr" || {
            echo "benchmark-compare: $label warmup failed" >&2
            sed -n '1,20p' "$scratch/$prefix.warm.stderr" >&2
            return 1
        }
        validate_success "$scratch/$prefix.warm.stdout" || {
            echo "benchmark-compare: $label warmup returned an error response" >&2
            jq . "$scratch/$prefix.warm.stdout" >&2
            return 1
        }
        if ! cmp -s <(jq -S -c . "$scratch/$prefix.warm.stdout") "$expected_output"; then
            echo "benchmark-compare: $label warmup output is nondeterministic" >&2
            return 1
        fi
    done

    for ((iteration = 0; iteration < iterations; iteration++)); do
        start_ns=$(now_ns)
        run_backend "$executable" "$request" \
            "$scratch/$prefix.$iteration.stdout" "$scratch/$prefix.$iteration.stderr" || {
            echo "benchmark-compare: $label measured run failed" >&2
            sed -n '1,20p' "$scratch/$prefix.$iteration.stderr" >&2
            return 1
        }
        end_ns=$(now_ns)
        validate_success "$scratch/$prefix.$iteration.stdout" || {
            echo "benchmark-compare: $label measured run returned an error response" >&2
            jq . "$scratch/$prefix.$iteration.stdout" >&2
            return 1
        }
        if ! cmp -s <(jq -S -c . "$scratch/$prefix.$iteration.stdout") "$expected_output"; then
            echo "benchmark-compare: $label measured output is nondeterministic" >&2
            return 1
        fi
        samples+=("$((end_ns - start_ns))")
        printf '%s,%s,%d,%s\n' "$workload" "$backend" "$((iteration + 1))" \
            "$((end_ns - start_ns))" >>"$raw_samples_file"
    done
    printf '%s\n' "${samples[@]}" | median_ns
}

printf 'workload,cpp_median_ms,rust_median_ms,rust_over_cpp,status\n' >"$results_file"
printf 'workload,backend,iteration,elapsed_ns\n' >"$raw_samples_file"

echo "release CLI benchmark (warmup=$warmup iterations=$iterations threads=$threads)"
echo "regression gate: ratio <= $max_slowdown or absolute delta <= ${regression_tolerance_ms}ms"
printf '%-34s %12s %12s %10s %s\n' workload cpp_ms rust_ms ratio status

regressions=0
line_number=0
while IFS= read -r fixture || [[ -n "$fixture" ]]; do
    line_number=$((line_number + 1))
    [[ -z "$fixture" ]] && continue
    if ! jq -e . >/dev/null 2>&1 <<<"$fixture"; then
        echo "benchmark-compare: invalid fixture JSON on line $line_number" >&2
        exit 2
    fi
    name=$(jq -r '.name' <<<"$fixture")
    request=$(jq -c '.request' <<<"$fixture")

    if ! run_backend "$cpp_bin" "$request" "$scratch/check.cpp" "$scratch/check.cpp.stderr"; then
        echo "benchmark-compare: C++ correctness preflight failed for '$name'" >&2
        sed -n '1,20p' "$scratch/check.cpp.stderr" >&2
        exit 1
    fi
    if ! run_backend "$rust_bin" "$request" "$scratch/check.rust" "$scratch/check.rust.stderr"; then
        echo "benchmark-compare: Rust correctness preflight failed for '$name'" >&2
        sed -n '1,20p' "$scratch/check.rust.stderr" >&2
        exit 1
    fi
    if ! validate_success "$scratch/check.cpp" || ! validate_success "$scratch/check.rust"; then
        echo "benchmark-compare: an error response was returned for '$name'" >&2
        jq . "$scratch/check.cpp" >&2
        jq . "$scratch/check.rust" >&2
        exit 1
    fi
    if ! cmp -s \
        <(jq -S -c . "$scratch/check.cpp") \
        <(jq -S -c . "$scratch/check.rust"); then
        echo "benchmark-compare: refusing to time '$name': backend outputs differ" >&2
        diff -u \
            <(jq -S . "$scratch/check.cpp") \
            <(jq -S . "$scratch/check.rust") >&2 || true
        exit 1
    fi
    jq -S -c . "$scratch/check.cpp" >"$scratch/check.cpp.canonical"
    jq -S -c . "$scratch/check.rust" >"$scratch/check.rust.canonical"

    cpp_ns=$(measure "$cpp_bin" "C++/$name" "$request" "cpp.$line_number" "$name" cpp \
        "$scratch/check.cpp.canonical")
    rust_ns=$(measure "$rust_bin" "Rust/$name" "$request" "rust.$line_number" "$name" rust \
        "$scratch/check.rust.canonical")
    if [[ ! "$cpp_ns" =~ ^[0-9]+([.][0-9]+)?$ || ! "$rust_ns" =~ ^[0-9]+([.][0-9]+)?$ ]] ||
        ! awk -v cpp="$cpp_ns" -v rust="$rust_ns" \
            'BEGIN { exit !(cpp > 0 && rust > 0) }'; then
        echo "benchmark-compare: invalid timing sample for '$name'" >&2
        exit 1
    fi
    cpp_ms=$(awk -v value="$cpp_ns" 'BEGIN { printf "%.3f", value / 1000000 }')
    rust_ms=$(awk -v value="$rust_ns" 'BEGIN { printf "%.3f", value / 1000000 }')
    ratio=$(awk -v rust="$rust_ns" -v cpp="$cpp_ns" 'BEGIN { printf "%.3f", rust / cpp }')
    delta_ms=$(awk -v rust="$rust_ns" -v cpp="$cpp_ns" \
        'BEGIN { printf "%.6f", (rust - cpp) / 1000000 }')
    status=ok
    if awk -v ratio="$ratio" -v maximum="$max_slowdown" \
        -v delta="$delta_ms" -v tolerance="$regression_tolerance_ms" \
        'BEGIN { exit !(ratio > maximum && delta > tolerance) }'; then
        status=REGRESSION
        regressions=$((regressions + 1))
    fi
    printf '%-34s %12s %12s %10s %s\n' "$name" "$cpp_ms" "$rust_ms" "$ratio" "$status"
    printf '%s,%s,%s,%s,%s\n' "$name" "$cpp_ms" "$rust_ms" "$ratio" "$status" \
        >>"$results_file"
done <"$workloads"

echo "benchmark summary: regressions=$regressions"
echo "benchmark evidence: $evidence_dir"
if [[ "$fail_on_regression" == 1 ]] && ((regressions > 0)); then
    update_metadata regression 1 "$regressions"
    metadata_finalized=1
    exit 1
fi
if ((regressions > 0)); then
    update_metadata passed_with_regressions 0 "$regressions"
else
    update_metadata passed 0 0
fi
metadata_finalized=1

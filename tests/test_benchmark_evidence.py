"""License-free tests for benchmark measurement, statistics, and policy gates."""

from __future__ import annotations

import copy
import csv
import hashlib
import io
import json
import math
import os
import shlex
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from typing import Any, Sequence

REPOSITORY = Path(__file__).resolve().parents[1]
DRIVER_SCRIPT = REPOSITORY / "scripts" / "benchmark-compare.sh"
PROCESS_SCRIPT = REPOSITORY / "scripts" / "benchmark_process.py"
STATS_SCRIPT = REPOSITORY / "scripts" / "benchmark_stats.py"
POLICY_SCRIPT = REPOSITORY / "scripts" / "benchmark_policy.py"
RESPONSE_COMPARISON_SCRIPT = REPOSITORY / "scripts" / "lib" / "response-comparison.sh"
SAMPLE_FIELDS = (
    "workload",
    "pair",
    "position",
    "backend",
    "elapsed_ns",
    "user_ns",
    "sys_ns",
    "max_rss_bytes",
)


def compact_json(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")


def run_script(
    script: Path,
    arguments: Sequence[str],
    *,
    input_value: object | None = None,
    timeout: float = 5.0,
) -> tuple[subprocess.CompletedProcess[bytes], dict[str, Any]]:
    input_bytes = compact_json(input_value) if input_value is not None else None
    completed = subprocess.run(
        [sys.executable, str(script), *arguments],
        input=input_bytes,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    try:
        output = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise AssertionError(
            f"{script.name} did not emit JSON; stderr={completed.stderr!r}"
        ) from error
    if not isinstance(output, dict):
        raise AssertionError(f"{script.name} emitted a non-object JSON value")
    return completed, output


def paired_samples(
    ratios: dict[str, float], *, pairs: int = 4, rust_rss_ratio: float = 1.0
) -> list[dict[str, object]]:
    samples: list[dict[str, object]] = []
    cpp_ns = 1_000_000
    cpp_rss = 10_000_000
    for workload, ratio in ratios.items():
        for pair in range(pairs):
            rust = {
                "workload": workload,
                "pair": pair + 1,
                "backend": "rust",
                "elapsed_ns": round(cpp_ns * ratio),
                "max_rss_bytes": round(cpp_rss * rust_rss_ratio),
            }
            cpp = {
                "workload": workload,
                "pair": pair + 1,
                "backend": "cpp",
                "elapsed_ns": cpp_ns,
                "max_rss_bytes": cpp_rss,
            }
            first, second = (cpp, rust) if pair % 2 == 0 else (rust, cpp)
            samples.append({**first, "position": 1})
            samples.append({**second, "position": 2})
    return samples


def stats_input(
    samples: list[dict[str, object]],
    *,
    global_upper_ci: float = 1.10,
    max_workload_ratio: float = 1.15,
    max_rss_ratio: float = 1.25,
) -> dict[str, object]:
    return {
        "samples": samples,
        "thresholds": {
            "global_upper_ci": global_upper_ci,
            "max_workload_ratio": max_workload_ratio,
            "max_rss_ratio": max_rss_ratio,
        },
        "bootstrap_samples": 500,
        "seed": 1729,
    }


class ProcessMeasurementTests(unittest.TestCase):
    def test_request_environment_and_wait4_metrics(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            stdout_path = directory / "child.stdout"
            stderr_path = directory / "child.stderr"
            child = (
                "import os,sys; data=sys.stdin.buffer.read(); "
                "sys.stdout.buffer.write(data); "
                "sys.stderr.write(os.environ['BENCHMARK_TEST_ENV'] + ':' + "
                "str('HOME' in os.environ))"
            )
            completed, result = run_script(
                PROCESS_SCRIPT,
                [
                    "--request",
                    "request-body",
                    "--stdout",
                    str(stdout_path),
                    "--stderr",
                    str(stderr_path),
                    "--timeout-seconds",
                    "2",
                    "--env",
                    "BENCHMARK_TEST_ENV=isolated",
                    "--clear-env",
                    "--",
                    sys.executable,
                    "-c",
                    child,
                ],
            )
            self.assertEqual(completed.returncode, 0, completed.stderr.decode())
            self.assertEqual(result["exit_code"], 0)
            self.assertIs(result["timed_out"], False)
            for field in ("elapsed_ns", "user_ns", "sys_ns", "max_rss_bytes"):
                self.assertIsInstance(result[field], int)
                self.assertGreaterEqual(result[field], 0)
            self.assertGreater(result["elapsed_ns"], 0)
            self.assertEqual(stdout_path.read_bytes(), b"request-body\n")
            self.assertEqual(stderr_path.read_text(encoding="utf-8"), "isolated:False")

    def test_selective_environment_inheritance_preserves_clear_env_isolation(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            stdout_path = directory / "child.stdout"
            stderr_path = directory / "child.stderr"
            secret = "synthetic-secret-that-must-not-be-recorded"
            environment = dict(os.environ)
            environment["BENCHMARK_TEST_SECRET"] = secret
            environment["BENCHMARK_TEST_UNRELATED"] = "must-not-be-inherited"
            child = (
                "import hashlib,json,os,sys; "
                "json.dump({'secret_sha256': hashlib.sha256("
                "os.environ['BENCHMARK_TEST_SECRET'].encode()).hexdigest(), "
                "'unrelated_present': 'BENCHMARK_TEST_UNRELATED' in os.environ}, "
                "sys.stdout)"
            )
            arguments = [
                sys.executable,
                str(PROCESS_SCRIPT),
                "--stdout",
                str(stdout_path),
                "--stderr",
                str(stderr_path),
                "--timeout-seconds",
                "2",
                "--inherit-env-var",
                "BENCHMARK_TEST_SECRET",
                "--clear-env",
                "--",
                sys.executable,
                "-c",
                child,
            ]
            completed = subprocess.run(
                arguments,
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=5,
                check=False,
            )

            self.assertEqual(completed.returncode, 0, completed.stderr.decode())
            measurement = json.loads(completed.stdout)
            self.assertEqual(measurement["exit_code"], 0)
            self.assertNotIn(secret.encode(), completed.stdout)
            self.assertNotIn(secret, "\0".join(arguments))
            child_result = json.loads(stdout_path.read_bytes())
            self.assertEqual(
                child_result["secret_sha256"],
                hashlib.sha256(secret.encode()).hexdigest(),
            )
            self.assertIs(child_result["unrelated_present"], False)

    def test_selective_environment_inheritance_rejects_invalid_or_missing_names(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for index, (name, expected) in enumerate(
                (
                    ("INVALID=NAME", b"invalid --inherit-env-var name"),
                    ("BENCHMARK_TEST_MISSING", b"is not set"),
                )
            ):
                with self.subTest(name=name):
                    environment = dict(os.environ)
                    environment.pop("BENCHMARK_TEST_MISSING", None)
                    completed = subprocess.run(
                        [
                            sys.executable,
                            str(PROCESS_SCRIPT),
                            "--stdout",
                            str(directory / f"{index}.stdout"),
                            "--stderr",
                            str(directory / f"{index}.stderr"),
                            "--timeout-seconds",
                            "2",
                            "--inherit-env-var",
                            name,
                            "--clear-env",
                            "--",
                            sys.executable,
                            "-c",
                            "raise SystemExit(99)",
                        ],
                        env=environment,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        timeout=5,
                        check=False,
                    )
                    self.assertEqual(completed.returncode, 2)
                    self.assertIn(expected, completed.stderr)
                    self.assertEqual(completed.stdout, b"")

    def test_timeout_escalates_to_process_group_kill(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            started = time.monotonic()
            completed, result = run_script(
                PROCESS_SCRIPT,
                [
                    "--request",
                    "{}",
                    "--stdout",
                    str(directory / "stdout"),
                    "--stderr",
                    str(directory / "stderr"),
                    "--timeout-seconds",
                    "0.05",
                    "--terminate-grace-seconds",
                    "0.02",
                    "--",
                    sys.executable,
                    "-c",
                    (
                        "import signal,time; "
                        "signal.signal(signal.SIGTERM, signal.SIG_IGN); "
                        "time.sleep(10)"
                    ),
                ],
                timeout=2,
            )
            wall_seconds = time.monotonic() - started
            self.assertEqual(completed.returncode, 0, completed.stderr.decode())
            self.assertIs(result["timed_out"], True)
            self.assertLess(result["exit_code"], 0)
            self.assertLess(wall_seconds, 1.0)

    def test_nonfinite_timeout_is_rejected_before_starting_child(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            completed = subprocess.run(
                [
                    sys.executable,
                    str(PROCESS_SCRIPT),
                    "--stdout",
                    str(directory / "stdout"),
                    "--stderr",
                    str(directory / "stderr"),
                    "--timeout-seconds",
                    "nan",
                    "--",
                    sys.executable,
                    "-c",
                    "raise SystemExit(99)",
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(completed.returncode, 2)
            self.assertIn(b"must be finite", completed.stderr)
            self.assertFalse((directory / "stdout").exists())


class SemanticResponseComparisonTests(unittest.TestCase):
    def test_declared_field_is_parsed_on_both_sides_and_envelope_stays_exact(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            parser_log = directory / "parser.jsonl"
            parser = directory / "semantic-parser"
            parser.write_text(
                f"""#!/usr/bin/env python3
import json
import sys

if sys.argv[1:] != ["eval-json"]:
    raise SystemExit(10)
request = json.load(sys.stdin)
if request.get("op") != "parse_expr" or request.get("vars") != ["x", "y"]:
    raise SystemExit(11)
with open({str(parser_log)!r}, "a", encoding="utf-8") as handle:
    handle.write(json.dumps(request, sort_keys=True) + "\\n")
json.dump({{"op": "parse_expr", "canonical": "(x + 1)/(y + 1)"}}, sys.stdout)
""",
                encoding="utf-8",
            )
            parser.chmod(0o755)
            cpp_response = directory / "cpp.json"
            rust_response = directory / "rust.json"
            cpp_response.write_bytes(
                compact_json(
                    {
                        "op": "rat_add",
                        "result": "(2*x+2)/(2*y+2)",
                        "tag": "stable",
                        "vars": ["x", "y"],
                    }
                )
            )
            rust_response.write_bytes(
                compact_json(
                    {
                        "op": "rat_add",
                        "result": "(x+1)/(y+1)",
                        "tag": "stable",
                        "vars": ["x", "y"],
                    }
                )
            )
            command = """
set -euo pipefail
source "$1"
hf_semantic_response "$2" '[]' '[]' '["result"]' '["x","y"]' "$3"
"""

            outputs: list[bytes] = []
            for response in (cpp_response, rust_response):
                completed = subprocess.run(
                    [
                        "bash",
                        "-c",
                        command,
                        "semantic-comparison-test",
                        str(RESPONSE_COMPARISON_SCRIPT),
                        str(response),
                        str(parser),
                    ],
                    cwd=REPOSITORY,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    timeout=5,
                    check=False,
                )
                self.assertEqual(completed.returncode, 0, completed.stderr.decode())
                outputs.append(completed.stdout)

            self.assertEqual(outputs[0], outputs[1])
            canonical = json.loads(outputs[0])
            self.assertEqual(canonical["result"], "(x + 1)/(y + 1)")
            self.assertEqual(canonical["tag"], "stable")
            parsed_requests = [
                json.loads(line)
                for line in parser_log.read_text(encoding="utf-8").splitlines()
            ]
            self.assertEqual(
                [request["expr"] for request in parsed_requests],
                ["(2*x+2)/(2*y+2)", "(x+1)/(y+1)"],
            )

            changed_envelope = json.loads(rust_response.read_bytes())
            changed_envelope["tag"] = "changed"
            rust_response.write_bytes(compact_json(changed_envelope))
            changed = subprocess.run(
                [
                    "bash",
                    "-c",
                    command,
                    "semantic-comparison-test",
                    str(RESPONSE_COMPARISON_SCRIPT),
                    str(rust_response),
                    str(parser),
                ],
                cwd=REPOSITORY,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=5,
                check=False,
            )
            self.assertEqual(changed.returncode, 0, changed.stderr.decode())
            self.assertNotEqual(outputs[0], changed.stdout)


class PairedStatisticsTests(unittest.TestCase):
    def test_single_pair_has_a_defined_bootstrap_bound(self) -> None:
        completed, result = run_script(
            STATS_SCRIPT,
            [],
            input_value=stats_input(paired_samples({"multiply": 1.02}, pairs=1)),
        )
        self.assertEqual(completed.returncode, 0, completed.stderr.decode())
        self.assertAlmostEqual(result["global"]["upper_95_ci"], 1.02, places=10)

    def test_paired_geometric_means_and_bootstrap_are_deterministic(self) -> None:
        document = stats_input(paired_samples({"multiply": 1.02, "resultant": 1.08}))
        first, first_result = run_script(STATS_SCRIPT, [], input_value=document)
        second, second_result = run_script(STATS_SCRIPT, [], input_value=document)
        self.assertEqual(first.returncode, 0, first.stderr.decode())
        self.assertEqual(second.returncode, 0, second.stderr.decode())
        self.assertEqual(first_result, second_result)
        self.assertEqual(first_result["status"], "pass")
        global_result = first_result["global"]
        self.assertAlmostEqual(
            global_result["geometric_mean_ratio"], math.sqrt(1.02 * 1.08), places=10
        )
        self.assertAlmostEqual(
            first_result["workloads"]["multiply"]["upper_95_ci"], 1.02, places=10
        )
        self.assertEqual(
            first_result["workloads"]["multiply"]["median_elapsed_ns"],
            {"cpp": 1_000_000.0, "rust": 1_020_000.0},
        )
        self.assertEqual(
            first_result["workloads"]["multiply"]["order"],
            {"cpp_first": 2, "rust_first": 2},
        )

    def test_unbalanced_block_order_is_invalid(self) -> None:
        samples = paired_samples({"multiply": 1.0})
        for index in range(0, len(samples), 2):
            pair = samples[index : index + 2]
            pair.sort(key=lambda sample: sample["backend"] != "cpp")
            pair[0]["position"] = 1
            pair[1]["position"] = 2
            samples[index : index + 2] = pair
        completed, result = run_script(
            STATS_SCRIPT, [], input_value=stats_input(samples)
        )
        self.assertEqual(completed.returncode, 2)
        self.assertEqual(result["status"], "invalid")
        self.assertIn("not balanced/interleaved", result["error"])

    def test_global_tail_and_memory_thresholds_are_independent(self) -> None:
        samples = paired_samples(
            {"normal": 1.0, "slow_tail": 1.20}, rust_rss_ratio=1.50
        )
        completed, result = run_script(
            STATS_SCRIPT,
            [],
            input_value=stats_input(
                samples,
                global_upper_ci=1.20,
                max_workload_ratio=1.10,
                max_rss_ratio=1.20,
            ),
        )
        self.assertEqual(completed.returncode, 1)
        gates = {
            (failure["gate"], failure.get("workload")) for failure in result["failures"]
        }
        self.assertIn(("severe_tail", "slow_tail"), gates)
        self.assertIn(("peak_rss", "normal"), gates)
        self.assertIn(("peak_rss", "slow_tail"), gates)

    def test_global_upper_confidence_bound_is_a_hard_gate(self) -> None:
        completed, result = run_script(
            STATS_SCRIPT,
            [],
            input_value=stats_input(
                paired_samples({"multiply": 1.06, "resultant": 1.06}),
                global_upper_ci=1.05,
                max_workload_ratio=1.20,
            ),
        )
        self.assertEqual(completed.returncode, 1)
        self.assertIn(
            "global_upper_95_ci",
            {failure["gate"] for failure in result["failures"]},
        )

    def test_zero_rss_counter_cannot_claim_memory_parity(self) -> None:
        samples = paired_samples({"multiply": 1.0})
        for sample in samples:
            sample["max_rss_bytes"] = 0
        completed, result = run_script(
            STATS_SCRIPT, [], input_value=stats_input(samples)
        )
        self.assertEqual(completed.returncode, 1)
        self.assertIn("peak_rss", {failure["gate"] for failure in result["failures"]})


class BenchmarkDriverTests(unittest.TestCase):
    def _environment(self) -> dict[str, str]:
        environment = dict(os.environ)
        for name in (
            "BENCH_WORKLOADS",
            "BENCHMARK_POLICY",
            "BENCHMARK_CORPUS",
            "BENCHMARK_MODE",
            "BENCHMARK_TIER",
            "HYPERFLINT_RUST",
            "HYPERFLINT_CPP",
            "HYPERFLINT_CPP_CMAKE_CACHE",
            "HYPERFLINT_CPP_SOURCE",
            "HYPERFLINT_CPP_PROFILE",
            "HYPERFLINT_CPP_BUILD_TYPE",
            "HYPERFLINT_CPP_MIMALLOC",
            "HYPERFLINT_CPP_OPENMP",
            "HYPERBOLICA_SOURCE",
            "RUST_REVISION",
            "CPP_REVISION",
            "RESULTS_FILE",
            "RAW_SAMPLES_FILE",
            "METADATA_FILE",
            "EVIDENCE_DIR",
            "PAIRS",
            "ITERATIONS",
            "WARMUP",
            "THREADS",
            "BOOTSTRAP_SAMPLES",
            "SEED",
            "CPUSET",
            "GLOBAL_UPPER_CI",
            "MAX_WORKLOAD_RATIO",
            "MAX_RSS_RATIO",
            "SYMBOLICA_LICENSE",
        ):
            environment.pop(name, None)
        environment["PYTHON"] = sys.executable
        return environment

    def _fake_cargo(self, directory: Path) -> Path:
        cargo = directory / "fake-cargo"
        cargo.write_text(
            "#!/bin/sh\nprintf '%s\\n' 'cargo 0.0.0-fake'\n", encoding="utf-8"
        )
        cargo.chmod(0o755)
        return cargo

    def _policy(self) -> dict[str, object]:
        return {
            "schema": 1,
            "policy_id": "license-free-driver-test-v1",
            "locked": True,
            "requirements": {
                "minimum_pairs_per_workload": 2,
                "warmup_runs": 0,
                "threads": 1,
                "bootstrap_samples": 20,
                "seed": 7,
                "cpuset": "0",
                "thresholds": {
                    "global_upper_ci": 1_000_000_000,
                    "max_workload_ratio": 1_000_000_000,
                    "max_rss_ratio": 1_000_000_000,
                },
                "clock": "perf_counter_ns",
                "rusage": "wait4",
                "order": "paired_interleaved",
                "affinity_scope": "measurement_helper_and_backend",
                "samples_include_process_startup": True,
                "backend_profiles": {
                    "rust": "release-lto",
                    "cpp_oracle": "release-portable",
                },
                "required_cpp_revision": "2" * 40,
                "required_cpp_cmake": {
                    "build_type": "Release",
                    "mimalloc": "ON",
                    "openmp": "ON",
                    "generator": "Unix Makefiles",
                    "cxx_flags": "",
                    "cxx_flags_release": "-O3 -DNDEBUG",
                    "asan": "OFF",
                    "tsan": "OFF",
                    "cli_static_deps": "OFF",
                },
                "environment_allowlist": [
                    "OMP_NUM_THREADS",
                    "OPENBLAS_NUM_THREADS",
                    "RAYON_NUM_THREADS",
                    "SYMBOLICA_HIDE_BANNER",
                    "SYMBOLICA_LICENSE",
                ],
                "performance_environment": {
                    "OMP_NUM_THREADS": "1",
                    "OPENBLAS_NUM_THREADS": "1",
                    "RAYON_NUM_THREADS": "1",
                    "SYMBOLICA_HIDE_BANNER": "1",
                },
            },
        }

    def test_license_is_inherited_only_by_the_rust_backend(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            fixture_path = directory / "benchmark.jsonl"
            fixture_path.write_bytes(
                compact_json({"name": "fake", "request": {"op": "fake"}}) + b"\n"
            )
            corpus_path = directory / "corpus.json"
            corpus_path.write_bytes(
                compact_json(
                    {
                        "schema": 1,
                        "fixture_sha256": hashlib.sha256(
                            fixture_path.read_bytes()
                        ).hexdigest(),
                        "workloads": [{"name": "fake", "tier": "qualification"}],
                    }
                )
            )
            policy_path = directory / "policy.json"
            policy_path.write_bytes(compact_json(self._policy()))
            rust_backend = directory / "fake-rust-backend"
            cpp_backend = directory / "fake-cpp-backend"
            secret = "synthetic-driver-license-do-not-record"
            backend_script = """#!/bin/sh
[ "$#" -eq 1 ] && [ "$1" = eval-json ] || exit 10
[ -z "${HOME+x}" ] || exit 11
[ "${OMP_NUM_THREADS-}" = 1 ] || exit 12
[ "${OPENBLAS_NUM_THREADS-}" = 1 ] || exit 13
[ "${RAYON_NUM_THREADS-}" = 1 ] || exit 14
[ "${SYMBOLICA_HIDE_BANNER-}" = 1 ] || exit 15
case "${0##*/}" in
  fake-rust-backend)
    [ "${SYMBOLICA_LICENSE-}" = @EXPECTED_LICENSE@ ] || exit 16
    ;;
  fake-cpp-backend)
    [ -z "${SYMBOLICA_LICENSE+x}" ] || exit 19
    ;;
  *) exit 20 ;;
esac
IFS= read -r request || exit 17
[ "$request" = '{"op":"fake"}' ] || exit 18
printf '%s\n' '{"ok":true}'
""".replace("@EXPECTED_LICENSE@", shlex.quote(secret))
            rust_backend.write_text(backend_script, encoding="utf-8")
            cpp_backend.write_text(backend_script, encoding="utf-8")
            rust_backend.chmod(0o755)
            cpp_backend.chmod(0o755)
            (directory / "CMakeCache.txt").write_text(
                "\n".join(
                    (
                        f"CMAKE_HOME_DIRECTORY:INTERNAL={REPOSITORY}",
                        "CMAKE_BUILD_TYPE:STRING=Release",
                        "HF_BUILD_VARIANT:STRING=release-portable",
                        "HF_MIMALLOC:BOOL=ON",
                        "HF_OPENMP:BOOL=ON",
                        "",
                    )
                ),
                encoding="utf-8",
            )
            taskset_log = directory / "taskset.log"
            fake_taskset = directory / "taskset"
            fake_taskset.write_text(
                """#!/bin/sh
[ "$#" -ge 3 ] && [ "$1" = -c ] || exit 20
printf '%s|%s\n' "$2" "$3" >> {log}
shift 2
exec "$@"
""".format(log=shlex.quote(str(taskset_log))),
                encoding="utf-8",
            )
            fake_taskset.chmod(0o755)
            evidence = directory / "evidence"
            environment = self._environment()
            environment.update(
                {
                    "CARGO": str(self._fake_cargo(directory)),
                    "BENCH_WORKLOADS": str(fixture_path),
                    "BENCHMARK_POLICY": str(policy_path),
                    "BENCHMARK_CORPUS": str(corpus_path),
                    "BENCHMARK_MODE": "exploratory",
                    "BENCHMARK_TIER": "qualification",
                    "HYPERFLINT_RUST": str(rust_backend),
                    "HYPERFLINT_CPP": str(cpp_backend),
                    "BUILD_RUST": "0",
                    "PAIRS": "2",
                    "WARMUP": "0",
                    "THREADS": "1",
                    "BOOTSTRAP_SAMPLES": "20",
                    "SEED": "7",
                    "CPUSET": "7",
                    "TIMEOUT_SECONDS": "2",
                    "GLOBAL_UPPER_CI": "1000000000",
                    "MAX_WORKLOAD_RATIO": "1000000000",
                    "MAX_RSS_RATIO": "1000000000",
                    "EVIDENCE_DIR": str(evidence),
                    "PATH": f"{directory}:{environment['PATH']}",
                    "SYMBOLICA_LICENSE": secret,
                }
            )
            completed = subprocess.run(
                ["bash", str(DRIVER_SCRIPT)],
                cwd=REPOSITORY,
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=20,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr.decode())
            self.assertNotIn(secret.encode(), completed.stdout)
            self.assertNotIn(secret.encode(), completed.stderr)
            metadata = json.loads((evidence / "metadata.json").read_bytes())
            self.assertEqual(metadata["status"], "passed")
            self.assertEqual(metadata["configuration"]["pairs_per_workload"], 2)
            self.assertIs(metadata["measurement"]["environment_sanitized"], True)
            self.assertEqual(
                metadata["measurement"]["credential_inheritance"],
                {
                    "rust_symbolica_license": True,
                    "cpp_oracle_symbolica_license": False,
                },
            )
            samples = json.loads((evidence / "samples.json").read_bytes())
            self.assertEqual(len(samples), 4)
            self.assertEqual({sample["backend"] for sample in samples}, {"cpp", "rust"})
            taskset_invocations = taskset_log.read_text(encoding="utf-8").splitlines()
            self.assertEqual(len(taskset_invocations), 6)
            self.assertTrue(
                all(
                    invocation == f"7|{sys.executable}"
                    for invocation in taskset_invocations
                )
            )
            correctness = json.loads((evidence / "correctness.json").read_bytes())
            self.assertEqual(correctness["status"], "pass")
            qualification = json.loads((evidence / "qualification.json").read_bytes())
            self.assertEqual(qualification["status"], "exploratory")
            self.assertIs(qualification["qualified"], False)
            for artifact in evidence.rglob("*"):
                if artifact.is_file():
                    self.assertNotIn(secret.encode(), artifact.read_bytes(), artifact)

    def test_qualification_rejects_revision_environment_override(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            environment = self._environment()
            environment.update(
                {
                    "CARGO": str(self._fake_cargo(directory)),
                    "BENCHMARK_MODE": "qualification",
                    "RUST_REVISION": "1" * 40,
                }
            )
            completed = subprocess.run(
                ["bash", str(DRIVER_SCRIPT)],
                cwd=REPOSITORY,
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=10,
                check=False,
            )
            self.assertEqual(completed.returncode, 1)
            self.assertIn(
                b"RUST_REVISION cannot override qualification provenance",
                completed.stderr,
            )

    def test_qualification_rejects_native_toolchain_environment_override(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            environment = self._environment()
            environment.update(
                {
                    "CARGO": str(self._fake_cargo(directory)),
                    "BENCHMARK_MODE": "qualification",
                    "CXXFLAGS": "-ffast-math",
                }
            )
            completed = subprocess.run(
                ["bash", str(DRIVER_SCRIPT)],
                cwd=REPOSITORY,
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=10,
                check=False,
            )
            self.assertEqual(completed.returncode, 1)
            self.assertIn(
                b"CXXFLAGS cannot alter a qualification build",
                completed.stderr,
            )

    def test_qualification_rejects_replacement_policy_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            policy = json.loads(
                (REPOSITORY / "tests/fixtures/benchmark-policy.json").read_bytes()
            )
            policy["requirements"]["thresholds"]["global_upper_ci"] = 99  # type: ignore[index]
            policy_path = directory / "replacement-policy.json"
            policy_path.write_bytes(compact_json(policy))
            environment = self._environment()
            environment.update(
                {
                    "CARGO": str(self._fake_cargo(directory)),
                    "BENCHMARK_MODE": "qualification",
                    "BENCHMARK_POLICY": str(policy_path),
                }
            )
            completed = subprocess.run(
                ["bash", str(DRIVER_SCRIPT)],
                cwd=REPOSITORY,
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=10,
                check=False,
            )
            self.assertEqual(completed.returncode, 1)
            self.assertIn(b"checked-in locked policy bytes", completed.stderr)


class QualificationPolicyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.directory = Path(self.temporary.name)
        self.analysis = self._analysis()
        self.policy = self._policy()
        self.corpus = {
            "schema": 1,
            "fixture_sha256": "d" * 64,
            "workloads": [
                {"name": "multiply", "tier": "qualification"},
                {"name": "resultant", "tier": "qualification"},
                {"name": "cold_start", "tier": "exploratory"},
            ],
        }
        self.policy_path = self._write("policy.json", self.policy)
        self.corpus_path = self._write("corpus.json", self.corpus)
        self.metadata = self._metadata()

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def _write(self, name: str, value: object) -> Path:
        path = self.directory / name
        path.write_bytes(compact_json(value))
        return path

    def _policy(self) -> dict[str, object]:
        return {
            "schema": 1,
            "policy_id": "hyperbolica-performance-v1",
            "locked": True,
            "requirements": {
                "minimum_pairs_per_workload": 4,
                "warmup_runs": 2,
                "threads": 1,
                "bootstrap_samples": 500,
                "seed": 1729,
                "cpuset": "0",
                "thresholds": {
                    "global_upper_ci": 1.10,
                    "max_workload_ratio": 1.15,
                    "max_rss_ratio": 1.25,
                },
                "clock": "perf_counter_ns",
                "rusage": "wait4",
                "order": "paired_interleaved",
                "affinity_scope": "measurement_helper_and_backend",
                "samples_include_process_startup": True,
                "backend_profiles": {
                    "rust": "release-lto",
                    "cpp_oracle": "release-portable",
                },
                "required_cpp_revision": "2" * 40,
                "required_cpp_cmake": {
                    "build_type": "Release",
                    "mimalloc": "ON",
                    "openmp": "ON",
                    "generator": "Unix Makefiles",
                    "cxx_flags": "",
                    "cxx_flags_release": "-O3 -DNDEBUG",
                    "asan": "OFF",
                    "tsan": "OFF",
                    "cli_static_deps": "OFF",
                },
                "environment_allowlist": [
                    "OMP_NUM_THREADS",
                    "OPENBLAS_NUM_THREADS",
                    "RAYON_NUM_THREADS",
                    "SYMBOLICA_HIDE_BANNER",
                    "SYMBOLICA_LICENSE",
                ],
                "performance_environment": {
                    "OMP_NUM_THREADS": "1",
                    "OPENBLAS_NUM_THREADS": "1",
                    "RAYON_NUM_THREADS": "1",
                    "SYMBOLICA_HIDE_BANNER": "1",
                },
            },
        }

    def _analysis(self) -> dict[str, Any]:
        self.samples = paired_samples({"multiply": 1.02, "resultant": 1.03})
        for sample in self.samples:
            sample["user_ns"] = 100_000
            sample["sys_ns"] = 10_000
        completed, result = run_script(
            STATS_SCRIPT,
            [],
            input_value=stats_input(self.samples),
        )
        self.assertEqual(completed.returncode, 0, completed.stderr.decode())
        return result

    def _backend(self, revision: str, profile: str) -> dict[str, object]:
        is_rust = profile == "release-lto"
        backend_name = "rust" if is_rust else "cpp"
        binary = self.directory / backend_name
        binary.write_bytes(f"synthetic-{backend_name}-binary".encode())
        build: dict[str, object] = {
            "source_revision": revision,
            "profile": profile,
            "command": "build --release",
            "manifest": f"/evidence/{'rust' if is_rust else 'cpp'}-build.json",
            "manifest_sha256": ("c" if is_rust else "d") * 64,
        }
        build["built_by_driver"] = True
        if not is_rust:
            cache = self.directory / "CMakeCache.txt"
            cache.write_text("synthetic locked CMake cache\n", encoding="utf-8")
            build["binary_version"] = (
                "HF_VERSION: test\nHF_BUILD_VARIANT: release-portable\n"
            )
            build["cmake"] = {
                "cache": str(cache),
                "cache_sha256": hashlib.sha256(cache.read_bytes()).hexdigest(),
                "build_type": "Release",
                "mimalloc": "ON",
                "openmp": "ON",
                "generator": "Unix Makefiles",
                "cxx_flags": "",
                "cxx_flags_release": "-O3 -DNDEBUG",
                "asan": "OFF",
                "tsan": "OFF",
                "cli_static_deps": "OFF",
            }
        return {
            "source": "/source/tree",
            "revision": revision,
            "binary": str(binary),
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "build": build,
        }

    def _materialize_artifacts(self, metadata: dict[str, object]) -> None:
        samples_path = self._write("samples.json", self.samples)
        samples_csv_path = self.directory / "samples.csv"
        csv_buffer = io.StringIO(newline="")
        writer = csv.DictWriter(
            csv_buffer, fieldnames=list(SAMPLE_FIELDS), lineterminator="\n"
        )
        writer.writeheader()
        writer.writerows(self.samples)
        samples_csv_path.write_text(csv_buffer.getvalue(), encoding="utf-8")

        correctness_path = self._write(
            "correctness.json",
            {
                "schema": 1,
                "status": "pass",
                "workloads": [
                    {
                        "name": "multiply",
                        "compare": "byte",
                        "canonical_response_sha256": "1" * 64,
                    },
                    {
                        "name": "resultant",
                        "compare": "normalized",
                        "canonical_response_sha256": "2" * 64,
                    },
                ],
            },
        )
        analysis_path = self._write("analysis.json", self.analysis)
        summary_path = self.directory / "summary.csv"
        summary_path.write_text(
            "workload,cpp_median_ms,rust_median_ms,rust_over_cpp,status\n",
            encoding="utf-8",
        )

        for backend_name in ("rust", "cpp_oracle"):
            provenance = metadata[backend_name]  # type: ignore[index]
            build = provenance["build"]  # type: ignore[index]
            manifest: dict[str, object] = {
                "schema": 1,
                "source": provenance["source"],  # type: ignore[index]
                "source_revision": build["source_revision"],  # type: ignore[index]
                "binary": provenance["binary"],  # type: ignore[index]
                "binary_sha256": provenance["binary_sha256"],  # type: ignore[index]
                "profile": build["profile"],  # type: ignore[index]
                "command": build["command"],  # type: ignore[index]
            }
            manifest["built_by_driver"] = True
            if backend_name == "rust":
                manifest["tools"] = {
                    "rustc": "rustc 1.90.0",
                    "cargo": "cargo 1.90.0",
                }
            else:
                manifest["binary_version"] = build["binary_version"]  # type: ignore[index]
                manifest["cmake"] = build["cmake"]  # type: ignore[index]
            manifest_path = self._write(f"{backend_name}-build.json", manifest)
            build["manifest"] = str(manifest_path)  # type: ignore[index]
            build["manifest_sha256"] = hashlib.sha256(  # type: ignore[index]
                manifest_path.read_bytes()
            ).hexdigest()

        metadata["artifacts"] = {
            "summary_csv": str(summary_path),
            "samples_json": str(samples_path),
            "samples_csv": str(samples_csv_path),
            "analysis_json": str(analysis_path),
            "correctness_json": str(correctness_path),
            "summary_sha256": hashlib.sha256(summary_path.read_bytes()).hexdigest(),
            "samples_sha256": hashlib.sha256(samples_path.read_bytes()).hexdigest(),
            "samples_csv_sha256": hashlib.sha256(
                samples_csv_path.read_bytes()
            ).hexdigest(),
            "analysis_sha256": hashlib.sha256(analysis_path.read_bytes()).hexdigest(),
            "correctness_sha256": hashlib.sha256(
                correctness_path.read_bytes()
            ).hexdigest(),
        }

    def _metadata(self) -> dict[str, object]:
        metadata: dict[str, object] = {
            "evidence_schema": 2,
            "mode": "qualification",
            "status": "passed",
            "run_id": "synthetic-qualification-run",
            "policy": {
                "id": self.policy["policy_id"],
                "sha256": hashlib.sha256(self.policy_path.read_bytes()).hexdigest(),
            },
            "corpus": {
                "sha256": hashlib.sha256(self.corpus_path.read_bytes()).hexdigest(),
                "fixture_sha256": self.corpus["fixture_sha256"],
                "workloads": ["multiply", "resultant"],
            },
            "configuration": {
                "pairs_per_workload": 4,
                "warmup_runs": 2,
                "threads": 1,
                "bootstrap_samples": 500,
                "seed": 1729,
                "thresholds": self.policy["requirements"][  # type: ignore[index]
                    "thresholds"
                ],
            },
            "measurement": {
                "clock": "perf_counter_ns",
                "rusage": "wait4",
                "order": "paired_interleaved",
                "affinity_scope": "measurement_helper_and_backend",
                "samples_include_process_startup": True,
                "cpuset": "0",
                "environment_sanitized": True,
                "environment_allowlist": self.policy["requirements"][  # type: ignore[index]
                    "environment_allowlist"
                ],
                "performance_environment": self.policy["requirements"][  # type: ignore[index]
                    "performance_environment"
                ],
                "credential_inheritance": {
                    "rust_symbolica_license": True,
                    "cpp_oracle_symbolica_license": False,
                },
            },
            "artifacts": {
                "summary_sha256": "0" * 64,
                "samples_sha256": "a" * 64,
                "samples_csv_sha256": "b" * 64,
                "analysis_sha256": "c" * 64,
                "correctness_sha256": "d" * 64,
            },
            "rust": self._backend("1" * 40, "release-lto"),
            "cpp_oracle": self._backend("2" * 40, "release-portable"),
            "analysis": self.analysis,
        }
        self._materialize_artifacts(metadata)
        return metadata

    def _verify(
        self, metadata: dict[str, object], mode: str
    ) -> tuple[subprocess.CompletedProcess[bytes], dict[str, Any]]:
        metadata_path = self._write("metadata.json", metadata)
        return run_script(
            POLICY_SCRIPT,
            [
                "--metadata",
                str(metadata_path),
                "--policy",
                str(self.policy_path),
                "--corpus",
                str(self.corpus_path),
                "--mode",
                mode,
            ],
        )

    def test_locked_qualification_passes(self) -> None:
        completed, result = self._verify(self.metadata, "qualification")
        self.assertEqual(completed.returncode, 0, completed.stderr.decode())
        self.assertEqual(result["status"], "pass")
        self.assertIs(result["qualified"], True)
        self.assertEqual(result["deviations"], [])

    def test_tampered_samples_bytes_fail_the_recorded_hash(self) -> None:
        samples_path = Path(self.metadata["artifacts"]["samples_json"])  # type: ignore[index]
        samples_path.write_bytes(samples_path.read_bytes() + b"\n")
        completed, result = self._verify(self.metadata, "qualification")
        self.assertEqual(completed.returncode, 1)
        self.assertIn(
            "samples_sha256_mismatch",
            {deviation["code"] for deviation in result["deviations"]},
        )

    def test_sample_pair_and_order_accounting_is_recomputed(self) -> None:
        metadata = copy.deepcopy(self.metadata)
        artifacts = metadata["artifacts"]  # type: ignore[index]
        samples_path = Path(artifacts["samples_json"])
        samples = json.loads(samples_path.read_bytes())
        samples = [
            sample
            for sample in samples
            if not (sample["workload"] == "resultant" and sample["pair"] == 4)
        ]
        samples_path.write_bytes(compact_json(samples))
        artifacts["samples_sha256"] = hashlib.sha256(  # type: ignore[index]
            samples_path.read_bytes()
        ).hexdigest()

        samples_csv_path = Path(artifacts["samples_csv"])
        csv_buffer = io.StringIO(newline="")
        writer = csv.DictWriter(
            csv_buffer, fieldnames=list(SAMPLE_FIELDS), lineterminator="\n"
        )
        writer.writeheader()
        writer.writerows(samples)
        samples_csv_path.write_text(csv_buffer.getvalue(), encoding="utf-8")
        artifacts["samples_csv_sha256"] = hashlib.sha256(  # type: ignore[index]
            samples_csv_path.read_bytes()
        ).hexdigest()

        completed, result = self._verify(metadata, "qualification")
        self.assertEqual(completed.returncode, 1)
        codes = {deviation["code"] for deviation in result["deviations"]}
        self.assertIn("sample_global_pair_count", codes)
        self.assertIn("sample_workload_pair_count", codes)
        self.assertIn("sample_workload_order", codes)
        self.assertIn("analysis_not_reproducible", codes)

    def test_missing_correctness_is_an_exploratory_deviation(self) -> None:
        correctness_path = Path(  # type: ignore[index]
            self.metadata["artifacts"]["correctness_json"]
        )
        correctness_path.unlink()
        completed, result = self._verify(self.metadata, "exploratory")
        self.assertEqual(completed.returncode, 0, completed.stderr.decode())
        self.assertEqual(result["status"], "exploratory")
        self.assertIs(result["qualified"], False)
        self.assertIs(result["would_qualify"], False)
        self.assertIn(
            "correctness_unreadable",
            {deviation["code"] for deviation in result["deviations"]},
        )

    def test_build_manifest_content_is_bound_to_metadata(self) -> None:
        metadata = copy.deepcopy(self.metadata)
        rust_build = metadata["rust"]["build"]  # type: ignore[index]
        manifest_path = Path(rust_build["manifest"])
        manifest = json.loads(manifest_path.read_bytes())
        manifest["profile"] = "debug"
        manifest_path.write_bytes(compact_json(manifest))
        rust_build["manifest_sha256"] = hashlib.sha256(  # type: ignore[index]
            manifest_path.read_bytes()
        ).hexdigest()
        completed, result = self._verify(metadata, "qualification")
        self.assertEqual(completed.returncode, 1)
        self.assertIn(
            "rust_manifest_profile",
            {deviation["code"] for deviation in result["deviations"]},
        )

    def test_missing_build_manifest_fails_qualification(self) -> None:
        manifest_path = Path(  # type: ignore[index]
            self.metadata["cpp_oracle"]["build"]["manifest"]
        )
        manifest_path.unlink()
        completed, result = self._verify(self.metadata, "qualification")
        self.assertEqual(completed.returncode, 1)
        self.assertIn(
            "cpp_oracle_manifest_unreadable",
            {deviation["code"] for deviation in result["deviations"]},
        )

    def test_timed_binary_bytes_are_reopened_and_hashed(self) -> None:
        rust_binary = Path(self.metadata["rust"]["binary"])  # type: ignore[index]
        rust_binary.write_bytes(rust_binary.read_bytes() + b"tampered")
        completed, result = self._verify(self.metadata, "qualification")
        self.assertEqual(completed.returncode, 1)
        self.assertIn(
            "rust_binary_sha256_mismatch",
            {deviation["code"] for deviation in result["deviations"]},
        )

    def test_cpp_cmake_cache_bytes_are_reopened_and_hashed(self) -> None:
        cache = Path(  # type: ignore[index]
            self.metadata["cpp_oracle"]["build"]["cmake"]["cache"]
        )
        cache.write_bytes(cache.read_bytes() + b"tampered")
        completed, result = self._verify(self.metadata, "qualification")
        self.assertEqual(completed.returncode, 1)
        self.assertIn(
            "cpp_oracle_cmake_cache_sha256_mismatch",
            {deviation["code"] for deviation in result["deviations"]},
        )

    def test_dirty_or_unbound_provenance_fails_qualification(self) -> None:
        metadata = copy.deepcopy(self.metadata)
        metadata["cpp_oracle"]["revision"] = f"{'2' * 40}-dirty"  # type: ignore[index]
        completed, result = self._verify(metadata, "qualification")
        self.assertEqual(completed.returncode, 1)
        self.assertIs(result["qualified"], False)
        codes = {deviation["code"] for deviation in result["deviations"]}
        self.assertIn("cpp_oracle_revision", codes)
        self.assertIn("cpp_oracle_build_revision", codes)

    def test_cpp_release_allocator_and_openmp_settings_are_locked(self) -> None:
        metadata = copy.deepcopy(self.metadata)
        metadata["cpp_oracle"]["build"]["cmake"]["mimalloc"] = "OFF"  # type: ignore[index]
        completed, result = self._verify(metadata, "qualification")
        self.assertEqual(completed.returncode, 1)
        self.assertIn(
            "cpp_oracle_cmake_mimalloc",
            {deviation["code"] for deviation in result["deviations"]},
        )

    def test_same_binary_cannot_pose_as_both_backends(self) -> None:
        metadata = copy.deepcopy(self.metadata)
        metadata["cpp_oracle"]["binary"] = metadata["rust"]["binary"]  # type: ignore[index]
        metadata["cpp_oracle"]["binary_sha256"] = metadata["rust"][  # type: ignore[index]
            "binary_sha256"
        ]
        completed, result = self._verify(metadata, "qualification")
        self.assertEqual(completed.returncode, 1)
        codes = {deviation["code"] for deviation in result["deviations"]}
        self.assertIn("backend_binary_path_collision", codes)
        self.assertIn("backend_binary_hash_collision", codes)

    def test_policy_rechecks_metrics_instead_of_trusting_status(self) -> None:
        metadata = copy.deepcopy(self.metadata)
        metadata["analysis"]["global"]["upper_95_ci"] = 9.0  # type: ignore[index]
        completed, result = self._verify(metadata, "qualification")
        self.assertEqual(completed.returncode, 1)
        self.assertIn(
            "global_upper_95_ci",
            {deviation["code"] for deviation in result["deviations"]},
        )

    def test_exploratory_mode_can_never_claim_qualification(self) -> None:
        metadata = copy.deepcopy(self.metadata)
        metadata["corpus"]["sha256"] = "0" * 64  # type: ignore[index]
        completed, result = self._verify(metadata, "exploratory")
        self.assertEqual(completed.returncode, 0, completed.stderr.decode())
        self.assertEqual(result["status"], "exploratory")
        self.assertIs(result["qualified"], False)
        self.assertIs(result["would_qualify"], False)
        self.assertIn(
            "corpus_sha256",
            {deviation["code"] for deviation in result["deviations"]},
        )

    def test_exploratory_is_the_safe_default_even_for_pristine_evidence(self) -> None:
        metadata_path = self._write("metadata.json", self.metadata)
        completed, result = run_script(
            POLICY_SCRIPT,
            [
                "--metadata",
                str(metadata_path),
                "--policy",
                str(self.policy_path),
                "--corpus",
                str(self.corpus_path),
            ],
        )
        self.assertEqual(completed.returncode, 0, completed.stderr.decode())
        self.assertEqual(result["status"], "exploratory")
        self.assertIs(result["qualified"], False)
        self.assertIs(result["would_qualify"], True)

    def test_verdict_binds_the_exact_run_and_evidence_hashes(self) -> None:
        completed, result = self._verify(self.metadata, "qualification")
        self.assertEqual(completed.returncode, 0, completed.stderr.decode())
        binding = result["evidence"]
        self.assertEqual(binding["run_id"], self.metadata["run_id"])
        metadata_path = self.directory / "metadata.json"
        self.assertEqual(
            binding["metadata_sha256"],
            hashlib.sha256(metadata_path.read_bytes()).hexdigest(),
        )
        self.assertEqual(
            binding["rust_binary_sha256"],
            self.metadata["rust"]["binary_sha256"],  # type: ignore[index]
        )
        self.assertEqual(
            binding["cpp_manifest_sha256"],
            self.metadata["cpp_oracle"]["build"]["manifest_sha256"],  # type: ignore[index]
        )

    def test_fixture_bytes_are_bound_independently_of_workload_names(self) -> None:
        metadata = copy.deepcopy(self.metadata)
        metadata["corpus"]["fixture_sha256"] = "e" * 64  # type: ignore[index]
        completed, result = self._verify(metadata, "qualification")
        self.assertEqual(completed.returncode, 1)
        self.assertIn(
            "fixture_sha256",
            {deviation["code"] for deviation in result["deviations"]},
        )

    def test_affinity_and_sanitized_environment_are_mandatory(self) -> None:
        metadata = copy.deepcopy(self.metadata)
        metadata["measurement"]["cpuset"] = ""  # type: ignore[index]
        metadata["measurement"]["environment_sanitized"] = False  # type: ignore[index]
        completed, result = self._verify(metadata, "qualification")
        self.assertEqual(completed.returncode, 1)
        codes = {deviation["code"] for deviation in result["deviations"]}
        self.assertIn("measurement_cpuset", codes)
        self.assertIn("measurement_environment_sanitized", codes)

    def test_cpp_oracle_credential_inheritance_is_rejected(self) -> None:
        metadata = copy.deepcopy(self.metadata)
        metadata["measurement"]["credential_inheritance"][  # type: ignore[index]
            "cpp_oracle_symbolica_license"
        ] = True
        completed, result = self._verify(metadata, "qualification")
        self.assertEqual(completed.returncode, 1)
        self.assertIn(
            "measurement_cpp_oracle_symbolica_license",
            {deviation["code"] for deviation in result["deviations"]},
        )

    def test_credential_inheritance_schema_rejects_extra_keys(self) -> None:
        metadata = copy.deepcopy(self.metadata)
        metadata["measurement"]["credential_inheritance"][  # type: ignore[index]
            "untracked_backend"
        ] = False
        completed, result = self._verify(metadata, "qualification")
        self.assertEqual(completed.returncode, 1)
        self.assertIn(
            "measurement_credential_inheritance_schema",
            {deviation["code"] for deviation in result["deviations"]},
        )

    def test_cpp_setup_commands_use_the_credential_scrubber(self) -> None:
        source = DRIVER_SCRIPT.read_text(encoding="utf-8")
        self.assertIn(
            'without_symbolica_license "$cmake_bin" -S "$cpp_source"', source
        )
        self.assertIn(
            'without_symbolica_license "$cmake_bin" --build "$cpp_build_dir"',
            source,
        )
        self.assertIn(
            'without_symbolica_license "$cpp_bin" --version', source
        )


if __name__ == "__main__":
    unittest.main()

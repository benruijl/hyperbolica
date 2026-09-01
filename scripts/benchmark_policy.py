#!/usr/bin/env python3
"""Verify benchmark evidence against a locked qualification policy."""

from __future__ import annotations

import argparse
import csv
import hashlib
import io
import json
import math
import re
import sys
from pathlib import Path
from typing import Any, Mapping, Sequence, TextIO

from benchmark_stats import StatisticsInputError
from benchmark_stats import analyze as analyze_statistics


class PolicyInputError(ValueError):
    """A policy, corpus, metadata, or analysis document is structurally invalid."""


SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
REVISION_PATTERN = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")
CPUSET_PATTERN = re.compile(r"^[0-9]+(?:-[0-9]+)?(?:,[0-9]+(?:-[0-9]+)?)*$")
ENVIRONMENT_NAME_PATTERN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
ALLOWED_ENVIRONMENT_NAMES = {
    "LANG",
    "LC_ALL",
    "OMP_NUM_THREADS",
    "OPENBLAS_NUM_THREADS",
    "PATH",
    "RAYON_NUM_THREADS",
    "SYMBOLICA_HIDE_BANNER",
    "SYMBOLICA_LICENSE_SERVER",
    "TZ",
}
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


def _mapping(value: object, location: str) -> Mapping[str, Any]:
    if not isinstance(value, dict):
        raise PolicyInputError(f"{location} must be a JSON object")
    return value


def _integer(value: object, location: str, *, minimum: int) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        raise PolicyInputError(f"{location} must be an integer >= {minimum}")
    return value


def _text(value: object, location: str) -> str:
    if not isinstance(value, str) or not value:
        raise PolicyInputError(f"{location} must be a non-empty string")
    return value


def _load(path: Path) -> tuple[object, bytes]:
    raw = path.read_bytes()
    try:
        return json.loads(raw), raw
    except json.JSONDecodeError as error:
        raise PolicyInputError(f"invalid JSON in {path}: {error}") from error


def _sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def _deviation(
    deviations: list[dict[str, Any]],
    code: str,
    message: str,
    *,
    actual: object = None,
    expected: object = None,
) -> None:
    record: dict[str, Any] = {"code": code, "message": message}
    if actual is not None:
        record["actual"] = actual
    if expected is not None:
        record["expected"] = expected
    deviations.append(record)


def _expect_equal(
    deviations: list[dict[str, Any]],
    code: str,
    message: str,
    actual: object,
    expected: object,
) -> None:
    if actual != expected:
        _deviation(deviations, code, message, actual=actual, expected=expected)


def _expect_ratio_at_most(
    deviations: list[dict[str, Any]],
    code: str,
    message: str,
    actual: object,
    limit: object,
) -> None:
    if (
        isinstance(actual, bool)
        or not isinstance(actual, (int, float))
        or not math.isfinite(actual)
        or actual < 0
        or isinstance(limit, bool)
        or not isinstance(limit, (int, float))
        or actual > limit
    ):
        _deviation(
            deviations, code, message, actual=actual, expected={"maximum": limit}
        )


def _artifact_path(value: object, base: Path) -> Path | None:
    if not isinstance(value, str) or not value:
        return None
    path = Path(value)
    return path if path.is_absolute() else base / path


def _read_bound_artifact(
    deviations: list[dict[str, Any]],
    name: str,
    path_value: object,
    expected_sha256: object,
    *,
    base: Path,
) -> bytes | None:
    path = _artifact_path(path_value, base)
    if path is None:
        _deviation(
            deviations,
            f"{name}_path",
            f"{name} artifact path is absent",
            actual=path_value,
        )
        return None
    try:
        raw = path.read_bytes()
    except OSError as error:
        _deviation(
            deviations,
            f"{name}_unreadable",
            f"{name} artifact cannot be read",
            actual={"path": str(path), "error": str(error)},
        )
        return None
    actual_sha256 = _sha256(raw)
    if (
        isinstance(expected_sha256, str)
        and SHA256_PATTERN.fullmatch(expected_sha256)
        and actual_sha256 != expected_sha256
    ):
        _deviation(
            deviations,
            f"{name}_sha256_mismatch",
            f"{name} artifact bytes differ from the recorded SHA-256",
            actual=actual_sha256,
            expected=expected_sha256,
        )
    return raw


def _read_bound_json(
    deviations: list[dict[str, Any]],
    name: str,
    path_value: object,
    expected_sha256: object,
    *,
    base: Path,
) -> object | None:
    raw = _read_bound_artifact(
        deviations,
        name,
        path_value,
        expected_sha256,
        base=base,
    )
    if raw is None:
        return None
    try:
        return json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        _deviation(
            deviations,
            f"{name}_json",
            f"{name} artifact is not valid UTF-8 JSON",
            actual=str(error),
        )
        return None


def _sample_integer(value: object, *, minimum: int) -> bool:
    return not isinstance(value, bool) and isinstance(value, int) and value >= minimum


def _validate_sample_records(
    deviations: list[dict[str, Any]], value: object
) -> list[dict[str, object]] | None:
    if not isinstance(value, list) or not value:
        _deviation(
            deviations,
            "samples_schema",
            "samples artifact must be a non-empty JSON array",
        )
        return None
    records: list[dict[str, object]] = []
    expected_fields = set(SAMPLE_FIELDS)
    for index, item in enumerate(value):
        if not isinstance(item, dict) or set(item) != expected_fields:
            _deviation(
                deviations,
                "samples_schema",
                f"samples[{index}] does not have the exact raw-sample fields",
            )
            return None
        if not isinstance(item.get("workload"), str) or not item["workload"]:
            _deviation(
                deviations,
                "samples_schema",
                f"samples[{index}].workload must be a non-empty string",
            )
            return None
        if item.get("backend") not in {"cpp", "rust"}:
            _deviation(
                deviations,
                "samples_schema",
                f"samples[{index}].backend must be cpp or rust",
            )
            return None
        integer_fields = {
            "pair": 1,
            "position": 1,
            "elapsed_ns": 1,
            "user_ns": 0,
            "sys_ns": 0,
            "max_rss_bytes": 0,
        }
        if any(
            not _sample_integer(item.get(field), minimum=minimum)
            for field, minimum in integer_fields.items()
        ) or item.get("position") not in {1, 2}:
            _deviation(
                deviations,
                "samples_schema",
                f"samples[{index}] has invalid integer measurement fields",
            )
            return None
        records.append(dict(item))
    return records


def _parse_samples_csv(
    deviations: list[dict[str, Any]], raw: bytes
) -> list[dict[str, object]] | None:
    try:
        text = raw.decode("utf-8")
        reader = csv.DictReader(io.StringIO(text, newline=""))
        if reader.fieldnames != list(SAMPLE_FIELDS):
            raise ValueError("header differs from the locked raw-sample columns")
        records: list[dict[str, object]] = []
        for index, row in enumerate(reader):
            if set(row) != set(SAMPLE_FIELDS) or any(
                value is None for value in row.values()
            ):
                raise ValueError(f"row {index + 2} has missing or extra columns")
            records.append(
                {
                    "workload": row["workload"],
                    "pair": int(row["pair"]),
                    "position": int(row["position"]),
                    "backend": row["backend"],
                    "elapsed_ns": int(row["elapsed_ns"]),
                    "user_ns": int(row["user_ns"]),
                    "sys_ns": int(row["sys_ns"]),
                    "max_rss_bytes": int(row["max_rss_bytes"]),
                }
            )
    except (UnicodeDecodeError, csv.Error, TypeError, ValueError) as error:
        _deviation(
            deviations,
            "samples_csv_schema",
            "samples CSV artifact is malformed",
            actual=str(error),
        )
        return None
    validated = _validate_sample_records([], records)
    if validated is None:
        _deviation(
            deviations,
            "samples_csv_schema",
            "samples CSV contains invalid raw-sample values",
        )
        return None
    return records


def _validate_correctness(
    deviations: list[dict[str, Any]],
    value: object,
    qualification_workloads: Sequence[str],
) -> None:
    if not isinstance(value, dict):
        _deviation(
            deviations,
            "correctness_schema",
            "correctness artifact must be a JSON object",
        )
        return
    _expect_equal(
        deviations,
        "correctness_schema",
        "correctness artifact schema differs",
        value.get("schema"),
        1,
    )
    _expect_equal(
        deviations,
        "correctness_status",
        "correctness preflight did not pass",
        value.get("status"),
        "pass",
    )
    workloads = value.get("workloads")
    if not isinstance(workloads, list):
        _deviation(
            deviations,
            "correctness_workloads",
            "correctness workloads must be an array",
        )
        return
    names: list[str] = []
    for index, item in enumerate(workloads):
        if not isinstance(item, dict):
            _deviation(
                deviations,
                "correctness_workload_schema",
                f"correctness.workloads[{index}] must be an object",
            )
            return
        name = item.get("name")
        compare = item.get("compare")
        response_sha256 = item.get("canonical_response_sha256")
        if (
            not isinstance(name, str)
            or not name
            or compare not in {"byte", "normalized"}
            or not isinstance(response_sha256, str)
            or not SHA256_PATTERN.fullmatch(response_sha256)
        ):
            _deviation(
                deviations,
                "correctness_workload_schema",
                f"correctness.workloads[{index}] is malformed",
            )
            return
        names.append(name)
    if len(names) != len(set(names)):
        _deviation(
            deviations,
            "correctness_workloads",
            "correctness artifact contains duplicate workload names",
        )
    _expect_equal(
        deviations,
        "correctness_workloads",
        "correctness workload set differs from the qualification corpus",
        sorted(names),
        list(qualification_workloads),
    )


def _validate_build_manifest(
    deviations: list[dict[str, Any]],
    name: str,
    value: object,
    provenance: object,
) -> None:
    if not isinstance(value, dict) or not isinstance(provenance, dict):
        _deviation(
            deviations,
            f"{name}_manifest_schema",
            f"{name} build manifest must be a JSON object",
        )
        return
    build = provenance.get("build")
    if not isinstance(build, dict):
        return
    expected_fields = {
        "schema": 1,
        "source": provenance.get("source"),
        "source_revision": build.get("source_revision"),
        "binary": provenance.get("binary"),
        "binary_sha256": provenance.get("binary_sha256"),
        "profile": build.get("profile"),
        "command": build.get("command"),
    }
    for field, expected in expected_fields.items():
        _expect_equal(
            deviations,
            f"{name}_manifest_{field}",
            f"{name} build manifest {field} differs from metadata",
            value.get(field),
            expected,
        )
    if name == "rust":
        _expect_equal(
            deviations,
            "rust_manifest_driver_build",
            "Rust build manifest driver-build flag differs from metadata",
            value.get("built_by_driver"),
            build.get("built_by_driver"),
        )
        tools = value.get("tools")
        if not isinstance(tools, dict) or any(
            not isinstance(tools.get(tool), str) or not tools[tool]
            for tool in ("rustc", "cargo")
        ):
            _deviation(
                deviations,
                "rust_manifest_tools",
                "Rust build manifest tool versions are absent",
            )
    else:
        _expect_equal(
            deviations,
            "cpp_oracle_manifest_driver_build",
            "C++ build manifest driver-build flag differs from metadata",
            value.get("built_by_driver"),
            build.get("built_by_driver"),
        )
        _expect_equal(
            deviations,
            "cpp_oracle_manifest_binary_version",
            "C++ build manifest binary version stamp differs from metadata",
            value.get("binary_version"),
            build.get("binary_version"),
        )
        _expect_equal(
            deviations,
            "cpp_oracle_manifest_cmake",
            "C++ build manifest CMake settings differ from metadata",
            value.get("cmake"),
            build.get("cmake"),
        )


def _verify_declared_artifacts(
    deviations: list[dict[str, Any]],
    metadata: Mapping[str, Any],
    analysis: Mapping[str, Any],
    requirements: Mapping[str, Any],
    qualification_workloads: Sequence[str],
    *,
    base: Path,
) -> None:
    artifacts_value = metadata.get("artifacts")
    artifacts = artifacts_value if isinstance(artifacts_value, dict) else {}

    _read_bound_artifact(
        deviations,
        "summary",
        artifacts.get("summary_csv"),
        artifacts.get("summary_sha256"),
        base=base,
    )

    samples_value = _read_bound_json(
        deviations,
        "samples",
        artifacts.get("samples_json"),
        artifacts.get("samples_sha256"),
        base=base,
    )
    samples = _validate_sample_records(deviations, samples_value)

    samples_csv_raw = _read_bound_artifact(
        deviations,
        "samples_csv",
        artifacts.get("samples_csv"),
        artifacts.get("samples_csv_sha256"),
        base=base,
    )
    csv_samples = (
        _parse_samples_csv(deviations, samples_csv_raw)
        if samples_csv_raw is not None
        else None
    )
    if samples is not None and csv_samples is not None:
        _expect_equal(
            deviations,
            "samples_csv_content",
            "samples CSV rows differ from samples JSON",
            csv_samples,
            samples,
        )

    correctness_value = _read_bound_json(
        deviations,
        "correctness",
        artifacts.get("correctness_json"),
        artifacts.get("correctness_sha256"),
        base=base,
    )
    if correctness_value is not None:
        _validate_correctness(deviations, correctness_value, qualification_workloads)

    for name, metadata_name in (("rust", "rust"), ("cpp_oracle", "cpp_oracle")):
        provenance = metadata.get(metadata_name)
        build = provenance.get("build") if isinstance(provenance, dict) else None
        build_mapping = build if isinstance(build, dict) else {}
        provenance_mapping = provenance if isinstance(provenance, dict) else {}
        _read_bound_artifact(
            deviations,
            f"{name}_binary",
            provenance_mapping.get("binary"),
            provenance_mapping.get("binary_sha256"),
            base=base,
        )
        manifest_value = _read_bound_json(
            deviations,
            f"{name}_manifest",
            build_mapping.get("manifest"),
            build_mapping.get("manifest_sha256"),
            base=base,
        )
        if manifest_value is not None:
            _validate_build_manifest(deviations, name, manifest_value, provenance)
        if name == "cpp_oracle":
            cmake = build_mapping.get("cmake")
            cmake_mapping = cmake if isinstance(cmake, dict) else {}
            _read_bound_artifact(
                deviations,
                "cpp_oracle_cmake_cache",
                cmake_mapping.get("cache"),
                cmake_mapping.get("cache_sha256"),
                base=base,
            )

    if samples is None:
        return
    statistics_input = {
        "samples": samples,
        "thresholds": requirements.get("thresholds"),
        "bootstrap_samples": requirements.get("bootstrap_samples"),
        "seed": requirements.get("seed"),
    }
    try:
        recomputed = analyze_statistics(statistics_input)
    except (StatisticsInputError, ArithmeticError) as error:
        _deviation(
            deviations,
            "samples_statistical_input",
            "raw samples cannot support the locked statistical analysis",
            actual=str(error),
        )
        return

    recomputed_workloads = recomputed["workloads"]
    _expect_equal(
        deviations,
        "sample_workload_set",
        "sample-derived workload set differs from the qualification corpus",
        sorted(recomputed_workloads),
        list(qualification_workloads),
    )
    analysis_global = analysis.get("global")
    analysis_global_mapping = (
        analysis_global if isinstance(analysis_global, dict) else {}
    )
    _expect_equal(
        deviations,
        "sample_global_workload_count",
        "analysis workload count differs from raw samples",
        analysis_global_mapping.get("workloads"),
        recomputed["global"]["workloads"],
    )
    _expect_equal(
        deviations,
        "sample_global_pair_count",
        "analysis pair count differs from raw samples",
        analysis_global_mapping.get("pairs"),
        recomputed["global"]["pairs"],
    )
    analysis_workloads = analysis.get("workloads")
    analysis_workload_mapping = (
        analysis_workloads if isinstance(analysis_workloads, dict) else {}
    )
    for workload, recomputed_result in recomputed_workloads.items():
        analysis_result = analysis_workload_mapping.get(workload)
        analysis_result_mapping = (
            analysis_result if isinstance(analysis_result, dict) else {}
        )
        _expect_equal(
            deviations,
            "sample_workload_pair_count",
            f"{workload!r} analysis pair count differs from raw samples",
            analysis_result_mapping.get("pairs"),
            recomputed_result["pairs"],
        )
        _expect_equal(
            deviations,
            "sample_workload_order",
            f"{workload!r} analysis order accounting differs from raw samples",
            analysis_result_mapping.get("order"),
            recomputed_result["order"],
        )
    if recomputed != dict(analysis):
        _deviation(
            deviations,
            "analysis_not_reproducible",
            "analysis cannot be reproduced exactly from the declared raw samples",
        )


def _qualification_workloads(corpus: Mapping[str, Any]) -> tuple[list[str], str]:
    if corpus.get("schema") != 1:
        raise PolicyInputError("corpus.schema must be 1")
    fixture_sha256 = _text(corpus.get("fixture_sha256"), "corpus.fixture_sha256")
    if not SHA256_PATTERN.fullmatch(fixture_sha256):
        raise PolicyInputError("corpus.fixture_sha256 must be a lowercase SHA-256")
    records = corpus.get("workloads")
    if not isinstance(records, list) or not records:
        raise PolicyInputError("corpus.workloads must be a non-empty array")
    selected: list[str] = []
    seen: set[str] = set()
    for index, value in enumerate(records):
        record = _mapping(value, f"corpus.workloads[{index}]")
        name = _text(record.get("name"), f"corpus.workloads[{index}].name")
        tier = _text(record.get("tier"), f"corpus.workloads[{index}].tier")
        if name in seen:
            raise PolicyInputError(f"duplicate corpus workload {name!r}")
        seen.add(name)
        if tier == "qualification":
            selected.append(name)
    if not selected:
        raise PolicyInputError("corpus has no qualification-tier workloads")
    return sorted(selected), fixture_sha256


def _requirements(policy: Mapping[str, Any]) -> tuple[str, Mapping[str, Any]]:
    if policy.get("schema") != 1:
        raise PolicyInputError("policy.schema must be 1")
    policy_id = _text(policy.get("policy_id"), "policy.policy_id")
    requirements = _mapping(policy.get("requirements"), "policy.requirements")
    _integer(
        requirements.get("minimum_pairs_per_workload"),
        "policy.requirements.minimum_pairs_per_workload",
        minimum=2,
    )
    _integer(
        requirements.get("warmup_runs"),
        "policy.requirements.warmup_runs",
        minimum=0,
    )
    _integer(requirements.get("threads"), "policy.requirements.threads", minimum=1)
    _integer(
        requirements.get("bootstrap_samples"),
        "policy.requirements.bootstrap_samples",
        minimum=1,
    )
    _integer(requirements.get("seed"), "policy.requirements.seed", minimum=0)
    cpuset = _text(requirements.get("cpuset"), "policy.requirements.cpuset")
    if not CPUSET_PATTERN.fullmatch(cpuset):
        raise PolicyInputError(
            "policy.requirements.cpuset must be an explicit CPU list such as '2' or '2-3'"
        )
    thresholds = _mapping(
        requirements.get("thresholds"), "policy.requirements.thresholds"
    )
    for name in ("global_upper_ci", "max_workload_ratio", "max_rss_ratio"):
        value = thresholds.get(name)
        if (
            isinstance(value, bool)
            or not isinstance(value, (int, float))
            or not math.isfinite(value)
            or value <= 0
        ):
            raise PolicyInputError(
                f"policy.requirements.thresholds.{name} must be positive"
            )
    profiles = _mapping(
        requirements.get("backend_profiles"),
        "policy.requirements.backend_profiles",
    )
    for backend in ("rust", "cpp_oracle"):
        _text(
            profiles.get(backend),
            f"policy.requirements.backend_profiles.{backend}",
        )
    required_cpp_revision = _text(
        requirements.get("required_cpp_revision"),
        "policy.requirements.required_cpp_revision",
    )
    if not REVISION_PATTERN.fullmatch(required_cpp_revision):
        raise PolicyInputError(
            "policy.requirements.required_cpp_revision must be a full Git object ID"
        )
    required_cpp_cmake = _mapping(
        requirements.get("required_cpp_cmake"),
        "policy.requirements.required_cpp_cmake",
    )
    expected_cpp_cmake = {
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
    if dict(required_cpp_cmake) != expected_cpp_cmake:
        raise PolicyInputError(
            "policy.requirements.required_cpp_cmake must lock the portable "
            "Release generator, flags, allocator, OpenMP, sanitizer, and "
            "dependency settings"
        )
    environment_allowlist = requirements.get("environment_allowlist")
    if not isinstance(environment_allowlist, list) or not environment_allowlist:
        raise PolicyInputError(
            "policy.requirements.environment_allowlist must be a non-empty array"
        )
    if (
        any(not isinstance(name, str) for name in environment_allowlist)
        or environment_allowlist != sorted(set(environment_allowlist))
        or any(
            not ENVIRONMENT_NAME_PATTERN.fullmatch(name)
            or name not in ALLOWED_ENVIRONMENT_NAMES
            or name.startswith("HF_")
            for name in environment_allowlist
        )
    ):
        raise PolicyInputError(
            "policy.requirements.environment_allowlist must be sorted, unique, and sanitized"
        )
    performance_environment = _mapping(
        requirements.get("performance_environment"),
        "policy.requirements.performance_environment",
    )
    threads = str(requirements["threads"])
    expected_environment = {
        "OMP_NUM_THREADS": threads,
        "OPENBLAS_NUM_THREADS": threads,
        "RAYON_NUM_THREADS": threads,
        "SYMBOLICA_HIDE_BANNER": "1",
    }
    if dict(performance_environment) != expected_environment:
        raise PolicyInputError(
            "policy.requirements.performance_environment must pin banner and all thread counts"
        )
    if not set(expected_environment).issubset(environment_allowlist):
        raise PolicyInputError(
            "the environment allowlist must include every performance-environment key"
        )
    expected_measurement = {
        "clock": "perf_counter_ns",
        "rusage": "wait4",
        "order": "paired_interleaved",
        "affinity_scope": "measurement_helper_and_backend",
        "samples_include_process_startup": True,
    }
    for name, expected in expected_measurement.items():
        if requirements.get(name) != expected:
            raise PolicyInputError(
                f"locked policies must set requirements.{name} to {expected!r}"
            )
    return policy_id, requirements


def _backend_provenance_deviations(
    deviations: list[dict[str, Any]],
    name: str,
    value: object,
    required_profile: object,
    required_revision: object,
    *,
    require_driver_build: bool = False,
    required_cmake: object = None,
) -> None:
    if not isinstance(value, dict):
        _deviation(deviations, f"{name}_provenance", f"{name} provenance is absent")
        return
    binary = value.get("binary")
    if not isinstance(binary, str) or not binary:
        _deviation(deviations, f"{name}_binary", f"{name} binary path is absent")
    revision = value.get("revision")
    if not isinstance(revision, str) or not REVISION_PATTERN.fullmatch(revision):
        _deviation(
            deviations,
            f"{name}_revision",
            f"{name} revision is not a clean full Git object ID",
            actual=revision,
        )
    binary_sha256 = value.get("binary_sha256")
    if not isinstance(binary_sha256, str) or not SHA256_PATTERN.fullmatch(
        binary_sha256
    ):
        _deviation(
            deviations,
            f"{name}_binary_sha256",
            f"{name} binary SHA-256 is missing or malformed",
            actual=binary_sha256,
        )
    source = value.get("source")
    if not isinstance(source, str) or not source:
        _deviation(deviations, f"{name}_source", f"{name} source path is absent")
    build = value.get("build")
    if not isinstance(build, dict):
        _deviation(deviations, f"{name}_build", f"{name} build provenance is absent")
        return
    _expect_equal(
        deviations,
        f"{name}_build_revision",
        f"{name} binary is not bound to its recorded source revision",
        build.get("source_revision"),
        revision,
    )
    command = build.get("command")
    if not isinstance(command, str) or not command:
        _deviation(
            deviations, f"{name}_build_command", f"{name} build command is absent"
        )
    manifest = build.get("manifest")
    if not isinstance(manifest, str) or not manifest:
        _deviation(
            deviations,
            f"{name}_build_manifest_path",
            f"{name} build manifest path is absent",
        )
    manifest_sha256 = build.get("manifest_sha256")
    if not isinstance(manifest_sha256, str) or not SHA256_PATTERN.fullmatch(
        manifest_sha256
    ):
        _deviation(
            deviations,
            f"{name}_build_manifest",
            f"{name} build manifest SHA-256 is missing or malformed",
            actual=manifest_sha256,
        )
    if required_profile is not None:
        _expect_equal(
            deviations,
            f"{name}_profile",
            f"{name} build profile differs from the locked policy",
            build.get("profile"),
            required_profile,
        )
    if required_revision is not None:
        _expect_equal(
            deviations,
            f"{name}_required_revision",
            f"{name} revision differs from the locked policy",
            revision,
            required_revision,
        )
    if require_driver_build:
        _expect_equal(
            deviations,
            f"{name}_driver_build",
            f"{name} binary was not built by the benchmark driver",
            build.get("built_by_driver"),
            True,
        )
    if required_cmake is not None:
        cmake = build.get("cmake")
        if not isinstance(cmake, dict):
            _deviation(
                deviations,
                f"{name}_cmake",
                f"{name} CMake configuration is absent",
            )
        else:
            required = _mapping(required_cmake, "required CMake configuration")
            for field, expected in required.items():
                _expect_equal(
                    deviations,
                    f"{name}_cmake_{field}",
                    f"{name} CMake {field} differs from the locked policy",
                    cmake.get(field),
                    expected,
                )
            cache_sha256 = cmake.get("cache_sha256")
            if not isinstance(cache_sha256, str) or not SHA256_PATTERN.fullmatch(
                cache_sha256
            ):
                _deviation(
                    deviations,
                    f"{name}_cmake_cache_sha256",
                    f"{name} CMake cache SHA-256 is missing or malformed",
                    actual=cache_sha256,
                )
        binary_version = build.get("binary_version")
        if (
            not isinstance(binary_version, str)
            or f"HF_BUILD_VARIANT: {required_profile}" not in binary_version
        ):
            _deviation(
                deviations,
                f"{name}_binary_version",
                f"{name} binary does not report the locked build-profile stamp",
                actual=binary_version,
            )


def verify(
    metadata_value: object,
    policy_value: object,
    corpus_value: object,
    analysis_value: object,
    *,
    metadata_sha256: str,
    policy_sha256: str,
    corpus_sha256: str,
    analysis_sha256: str | None,
    mode: str,
    artifact_base: Path | None = None,
) -> dict[str, Any]:
    metadata = _mapping(metadata_value, "metadata")
    policy = _mapping(policy_value, "policy")
    corpus = _mapping(corpus_value, "corpus")
    analysis = _mapping(analysis_value, "analysis")
    policy_id, requirements = _requirements(policy)
    qualification_workloads, fixture_sha256 = _qualification_workloads(corpus)
    deviations: list[dict[str, Any]] = []

    _expect_equal(
        deviations,
        "evidence_schema",
        "metadata evidence schema differs",
        metadata.get("evidence_schema"),
        2,
    )
    if policy.get("locked") is not True:
        _deviation(deviations, "policy_unlocked", "qualification policy is not locked")
    policy_record = metadata.get("policy")
    if not isinstance(policy_record, dict):
        _deviation(deviations, "policy_binding", "metadata has no policy binding")
    else:
        _expect_equal(
            deviations,
            "policy_id",
            "metadata policy ID differs",
            policy_record.get("id"),
            policy_id,
        )
        _expect_equal(
            deviations,
            "policy_sha256",
            "metadata is not bound to the exact policy bytes",
            policy_record.get("sha256"),
            policy_sha256,
        )

    corpus_record = metadata.get("corpus")
    if not isinstance(corpus_record, dict):
        _deviation(deviations, "corpus_binding", "metadata has no corpus binding")
    else:
        _expect_equal(
            deviations,
            "corpus_sha256",
            "metadata is not bound to the exact corpus bytes",
            corpus_record.get("sha256"),
            corpus_sha256,
        )
        _expect_equal(
            deviations,
            "fixture_sha256",
            "metadata is not bound to the exact benchmark fixture bytes",
            corpus_record.get("fixture_sha256"),
            fixture_sha256,
        )
        names = corpus_record.get("workloads")
        actual_names = sorted(names) if isinstance(names, list) else names
        _expect_equal(
            deviations,
            "corpus_workloads",
            "metadata workload set differs from the qualification corpus",
            actual_names,
            qualification_workloads,
        )

    _expect_equal(
        deviations,
        "metadata_mode",
        "metadata was not recorded as qualification evidence",
        metadata.get("mode"),
        "qualification",
    )
    _expect_equal(
        deviations,
        "metadata_status",
        "benchmark run did not complete successfully",
        metadata.get("status"),
        "passed",
    )

    configuration = metadata.get("configuration")
    if not isinstance(configuration, dict):
        _deviation(deviations, "configuration", "metadata configuration is absent")
    else:
        configuration_requirements = {
            "pairs_per_workload": "minimum_pairs_per_workload",
            "warmup_runs": "warmup_runs",
            "threads": "threads",
            "bootstrap_samples": "bootstrap_samples",
            "seed": "seed",
            "thresholds": "thresholds",
        }
        for name, requirement_name in configuration_requirements.items():
            _expect_equal(
                deviations,
                f"configuration_{name}",
                f"metadata {name} differs from the locked policy",
                configuration.get(name),
                requirements.get(requirement_name),
            )

    measurement = metadata.get("measurement")
    if not isinstance(measurement, dict):
        _deviation(deviations, "measurement", "measurement method is absent")
    else:
        for name in (
            "clock",
            "rusage",
            "order",
            "affinity_scope",
            "samples_include_process_startup",
            "cpuset",
            "environment_allowlist",
            "performance_environment",
        ):
            _expect_equal(
                deviations,
                f"measurement_{name}",
                f"measurement {name} differs from the locked policy",
                measurement.get(name),
                requirements.get(name),
            )
        _expect_equal(
            deviations,
            "measurement_environment_sanitized",
            "child environment was not recorded as sanitized",
            measurement.get("environment_sanitized"),
            True,
        )

    artifacts = metadata.get("artifacts")
    if not isinstance(artifacts, dict):
        _deviation(deviations, "artifacts", "artifact hashes are absent")
    else:
        for artifact_name in (
            "summary_sha256",
            "samples_sha256",
            "samples_csv_sha256",
            "analysis_sha256",
            "correctness_sha256",
        ):
            artifact_sha256 = artifacts.get(artifact_name)
            if not isinstance(artifact_sha256, str) or not SHA256_PATTERN.fullmatch(
                artifact_sha256
            ):
                _deviation(
                    deviations,
                    artifact_name,
                    f"{artifact_name.removesuffix('_sha256')} SHA-256 is missing or malformed",
                    actual=artifact_sha256,
                )
        if analysis_sha256 is not None:
            _expect_equal(
                deviations,
                "analysis_sha256",
                "metadata is not bound to the supplied analysis bytes",
                artifacts.get("analysis_sha256"),
                analysis_sha256,
            )

    _expect_equal(
        deviations,
        "embedded_analysis",
        "metadata's embedded analysis differs from the supplied analysis artifact",
        metadata.get("analysis"),
        analysis,
    )

    profiles = requirements.get("backend_profiles")
    if profiles is not None and not isinstance(profiles, dict):
        raise PolicyInputError("policy.requirements.backend_profiles must be an object")
    profile_mapping = profiles if isinstance(profiles, dict) else {}
    _backend_provenance_deviations(
        deviations,
        "rust",
        metadata.get("rust"),
        profile_mapping.get("rust"),
        requirements.get("required_rust_revision"),
        require_driver_build=True,
    )
    _backend_provenance_deviations(
        deviations,
        "cpp_oracle",
        metadata.get("cpp_oracle"),
        profile_mapping.get("cpp_oracle"),
        requirements.get("required_cpp_revision"),
        require_driver_build=True,
        required_cmake=requirements.get("required_cpp_cmake"),
    )
    rust_provenance = metadata.get("rust")
    cpp_provenance = metadata.get("cpp_oracle")
    if isinstance(rust_provenance, dict) and isinstance(cpp_provenance, dict):
        rust_binary = rust_provenance.get("binary")
        cpp_binary = cpp_provenance.get("binary")
        if isinstance(rust_binary, str) and rust_binary == cpp_binary:
            _deviation(
                deviations,
                "backend_binary_path_collision",
                "Rust and C++ provenance names the same backend binary",
                actual=rust_binary,
            )
        rust_binary_sha256 = rust_provenance.get("binary_sha256")
        cpp_binary_sha256 = cpp_provenance.get("binary_sha256")
        if (
            isinstance(rust_binary_sha256, str)
            and rust_binary_sha256 == cpp_binary_sha256
        ):
            _deviation(
                deviations,
                "backend_binary_hash_collision",
                "Rust and C++ backend binaries have the same SHA-256",
                actual=rust_binary_sha256,
            )

    _verify_declared_artifacts(
        deviations,
        metadata,
        analysis,
        requirements,
        qualification_workloads,
        base=artifact_base if artifact_base is not None else Path.cwd(),
    )

    _expect_equal(
        deviations,
        "analysis_schema",
        "statistical analysis schema differs",
        analysis.get("schema"),
        1,
    )
    _expect_equal(
        deviations,
        "analysis_status",
        "statistical gates did not pass",
        analysis.get("status"),
        "pass",
    )
    _expect_equal(
        deviations,
        "analysis_failures",
        "statistical analysis contains gate failures",
        analysis.get("failures"),
        [],
    )
    analysis_configuration = analysis.get("configuration")
    if not isinstance(analysis_configuration, dict):
        _deviation(
            deviations, "analysis_configuration", "analysis configuration is absent"
        )
    else:
        for name in ("bootstrap_samples", "seed", "thresholds"):
            _expect_equal(
                deviations,
                f"analysis_{name}",
                f"analysis {name} differs from the locked policy",
                analysis_configuration.get(name),
                requirements.get(name),
            )
        _expect_equal(
            deviations,
            "analysis_method",
            "analysis does not use the locked paired bootstrap method",
            analysis_configuration.get("method"),
            "paired_log_ratio_stratified_bootstrap",
        )

    locked_thresholds = requirements["thresholds"]
    global_result = analysis.get("global")
    if not isinstance(global_result, dict):
        _deviation(deviations, "analysis_global", "global analysis is absent")
    else:
        _expect_ratio_at_most(
            deviations,
            "global_upper_95_ci",
            "global upper confidence bound exceeds the locked threshold",
            global_result.get("upper_95_ci"),
            locked_thresholds["global_upper_ci"],
        )

    workload_results = analysis.get("workloads")
    if not isinstance(workload_results, dict):
        _deviation(deviations, "analysis_workloads", "per-workload analysis is absent")
    else:
        _expect_equal(
            deviations,
            "analysis_workload_set",
            "analyzed workload set differs from the qualification corpus",
            sorted(workload_results),
            qualification_workloads,
        )
        minimum_pairs = requirements["minimum_pairs_per_workload"]
        for name in qualification_workloads:
            result = workload_results.get(name)
            if not isinstance(result, dict):
                continue
            pairs = result.get("pairs")
            if (
                isinstance(pairs, bool)
                or not isinstance(pairs, int)
                or pairs < minimum_pairs
            ):
                _deviation(
                    deviations,
                    "minimum_pairs",
                    f"{name!r} has too few matched pairs",
                    actual=pairs,
                    expected=minimum_pairs,
                )
            _expect_ratio_at_most(
                deviations,
                "severe_tail",
                f"{name!r} exceeds the locked severe-tail threshold",
                result.get("geometric_mean_ratio"),
                locked_thresholds["max_workload_ratio"],
            )
            _expect_ratio_at_most(
                deviations,
                "peak_rss",
                f"{name!r} exceeds the locked peak-RSS threshold",
                result.get("peak_rss_ratio"),
                locked_thresholds["max_rss_ratio"],
            )
            order = result.get("order")
            if not isinstance(order, dict):
                _deviation(
                    deviations, "order_balance", f"{name!r} has no order accounting"
                )
                continue
            cpp_first = order.get("cpp_first")
            rust_first = order.get("rust_first")
            if (
                isinstance(cpp_first, bool)
                or isinstance(rust_first, bool)
                or not isinstance(cpp_first, int)
                or not isinstance(rust_first, int)
                or cpp_first == 0
                or rust_first == 0
                or abs(cpp_first - rust_first) > 1
                or cpp_first + rust_first != pairs
            ):
                _deviation(
                    deviations,
                    "order_balance",
                    f"{name!r} is not a balanced paired/interleaved run",
                    actual=order,
                )

    would_qualify = not deviations
    artifacts_mapping = (
        artifacts if isinstance(artifacts, dict) else {}
    )
    rust = metadata.get("rust")
    rust_mapping = rust if isinstance(rust, dict) else {}
    rust_build = rust_mapping.get("build")
    rust_build_mapping = rust_build if isinstance(rust_build, dict) else {}
    cpp = metadata.get("cpp_oracle")
    cpp_mapping = cpp if isinstance(cpp, dict) else {}
    cpp_build = cpp_mapping.get("build")
    cpp_build_mapping = cpp_build if isinstance(cpp_build, dict) else {}
    evidence_binding = {
        "run_id": metadata.get("run_id"),
        "metadata_sha256": metadata_sha256,
        "summary_sha256": artifacts_mapping.get("summary_sha256"),
        "samples_sha256": artifacts_mapping.get("samples_sha256"),
        "samples_csv_sha256": artifacts_mapping.get("samples_csv_sha256"),
        "analysis_sha256": artifacts_mapping.get("analysis_sha256"),
        "correctness_sha256": artifacts_mapping.get("correctness_sha256"),
        "rust_manifest_sha256": rust_build_mapping.get("manifest_sha256"),
        "cpp_manifest_sha256": cpp_build_mapping.get("manifest_sha256"),
        "rust_binary_sha256": rust_mapping.get("binary_sha256"),
        "cpp_binary_sha256": cpp_mapping.get("binary_sha256"),
    }
    if mode == "qualification":
        return {
            "schema": 1,
            "mode": mode,
            "status": "pass" if would_qualify else "fail",
            "qualified": would_qualify,
            "policy_id": policy_id,
            "policy_sha256": policy_sha256,
            "corpus_sha256": corpus_sha256,
            "evidence": evidence_binding,
            "deviations": deviations,
        }
    return {
        "schema": 1,
        "mode": mode,
        "status": "exploratory",
        "qualified": False,
        "would_qualify": would_qualify,
        "policy_id": policy_id,
        "policy_sha256": policy_sha256,
        "corpus_sha256": corpus_sha256,
        "evidence": evidence_binding,
        "deviations": deviations,
    }


def _write_json(value: object, path: str, *, pretty: bool) -> None:
    handle: TextIO
    should_close = path != "-"
    if should_close:
        output_path = Path(path)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        handle = output_path.open("w", encoding="utf-8")
    else:
        handle = sys.stdout
    try:
        if pretty:
            json.dump(value, handle, indent=2, sort_keys=True, allow_nan=False)
        else:
            json.dump(
                value, handle, sort_keys=True, separators=(",", ":"), allow_nan=False
            )
        handle.write("\n")
    finally:
        if should_close:
            handle.close()


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Verify benchmark metadata and analysis against a locked policy/corpus.",
        epilog=(
            "Qualification is deliberately opt-in: the default exploratory mode "
            "always emits qualified=false even if it reports would_qualify=true. "
            "Qualification additionally requires exact policy/corpus hashes, clean "
            "source and binary/build provenance, wait4/perf_counter_ns paired "
            "measurement, the full corpus, locked statistical settings, and passing "
            "gates. Declared summary/sample JSON/CSV, correctness, build manifests, "
            "timed binaries, and CMake cache are reopened and hashed; analysis is "
            "recomputed from the sample JSON. "
            "POLICY requires schema, policy_id, locked, and requirements "
            "containing minimum_pairs_per_workload, warmup_runs, threads, bootstrap_"
            "samples, seed, thresholds, backend_profiles, required_cpp_revision, "
            "required_cpp_cmake, cpuset, environment_allowlist, performance_environment, "
            "clock, rusage, order, affinity_scope, and samples_include_process_startup. "
            "CORPUS requires fixture_"
            "sha256 and workloads [{name,tier}]. Exit status is 0 for exploratory or qualified "
            "evidence, 1 for a qualification failure, and 2 for invalid documents."
        ),
    )
    parser.add_argument("--metadata", required=True, type=Path)
    parser.add_argument("--policy", required=True, type=Path)
    parser.add_argument("--corpus", required=True, type=Path)
    parser.add_argument(
        "--analysis",
        type=Path,
        help="analysis JSON; defaults to metadata.analysis",
    )
    parser.add_argument(
        "--mode", choices=("exploratory", "qualification"), default="exploratory"
    )
    parser.add_argument(
        "--output", default="-", help="output JSON path (default: stdout)"
    )
    parser.add_argument("--pretty", action="store_true")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parser().parse_args(argv)
    try:
        metadata, metadata_raw = _load(arguments.metadata)
        policy, policy_raw = _load(arguments.policy)
        corpus, corpus_raw = _load(arguments.corpus)
        if arguments.analysis is None:
            analysis = _mapping(metadata, "metadata").get("analysis")
            analysis_sha256 = None
        else:
            analysis, analysis_raw = _load(arguments.analysis)
            analysis_sha256 = _sha256(analysis_raw)
        result = verify(
            metadata,
            policy,
            corpus,
            analysis,
            metadata_sha256=_sha256(metadata_raw),
            policy_sha256=_sha256(policy_raw),
            corpus_sha256=_sha256(corpus_raw),
            analysis_sha256=analysis_sha256,
            mode=arguments.mode,
            artifact_base=arguments.metadata.resolve().parent,
        )
    except (OSError, PolicyInputError) as error:
        invalid = {"schema": 1, "status": "invalid", "error": str(error)}
        _write_json(invalid, arguments.output, pretty=arguments.pretty)
        return 2
    _write_json(result, arguments.output, pretty=arguments.pretty)
    if arguments.mode == "qualification" and not result["qualified"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

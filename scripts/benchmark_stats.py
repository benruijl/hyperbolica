#!/usr/bin/env python3
"""Analyze paired, interleaved HyperFLINT/Hyperbolica benchmark samples."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import random
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Mapping, Sequence, TextIO


class StatisticsInputError(ValueError):
    """The sample document cannot support a paired performance comparison."""


@dataclass(frozen=True)
class Sample:
    workload: str
    pair: str
    position: int
    backend: str
    elapsed_ns: int
    max_rss_bytes: int
    ordinal: int


@dataclass(frozen=True)
class Thresholds:
    global_upper_ci: float
    max_workload_ratio: float
    max_rss_ratio: float

    def as_json(self) -> dict[str, float]:
        return {
            "global_upper_ci": self.global_upper_ci,
            "max_workload_ratio": self.max_workload_ratio,
            "max_rss_ratio": self.max_rss_ratio,
        }


def _mapping(value: object, location: str) -> Mapping[str, Any]:
    if not isinstance(value, dict):
        raise StatisticsInputError(f"{location} must be a JSON object")
    return value


def _integer(value: object, location: str, *, minimum: int) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        raise StatisticsInputError(f"{location} must be an integer >= {minimum}")
    return value


def _positive_number(value: object, location: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise StatisticsInputError(f"{location} must be a positive number")
    parsed = float(value)
    if not math.isfinite(parsed) or parsed <= 0:
        raise StatisticsInputError(f"{location} must be a finite positive number")
    return parsed


def _pair_identifier(value: object, location: str) -> str:
    if isinstance(value, bool) or not isinstance(value, (str, int)):
        raise StatisticsInputError(f"{location} must be a string or integer")
    # Preserve the distinction between the JSON values 1 and "1".
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def _parse_sample(value: object, ordinal: int) -> Sample:
    record = _mapping(value, f"samples[{ordinal}]")
    workload = record.get("workload")
    if not isinstance(workload, str) or not workload:
        raise StatisticsInputError(f"samples[{ordinal}].workload must be non-empty")
    backend = record.get("backend")
    if backend not in {"cpp", "rust"}:
        raise StatisticsInputError(
            f"samples[{ordinal}].backend must be 'cpp' or 'rust'"
        )
    position = _integer(
        record.get("position"), f"samples[{ordinal}].position", minimum=1
    )
    if position not in {1, 2}:
        raise StatisticsInputError(f"samples[{ordinal}].position must be 1 or 2")
    return Sample(
        workload=workload,
        pair=_pair_identifier(record.get("pair"), f"samples[{ordinal}].pair"),
        position=position,
        backend=backend,
        elapsed_ns=_integer(
            record.get("elapsed_ns"), f"samples[{ordinal}].elapsed_ns", minimum=1
        ),
        max_rss_bytes=_integer(
            record.get("max_rss_bytes"),
            f"samples[{ordinal}].max_rss_bytes",
            minimum=0,
        ),
        ordinal=ordinal,
    )


def _parse_thresholds(value: object) -> Thresholds:
    record = _mapping(value, "thresholds")
    return Thresholds(
        global_upper_ci=_positive_number(
            record.get("global_upper_ci"), "thresholds.global_upper_ci"
        ),
        max_workload_ratio=_positive_number(
            record.get("max_workload_ratio"), "thresholds.max_workload_ratio"
        ),
        max_rss_ratio=_positive_number(
            record.get("max_rss_ratio"), "thresholds.max_rss_ratio"
        ),
    )


def _geometric_mean_from_logs(log_ratios: Sequence[float]) -> float:
    return math.exp(math.fsum(log_ratios) / len(log_ratios))


def _median(values: Sequence[int]) -> int | float:
    ordered = sorted(values)
    midpoint = len(ordered) // 2
    if len(ordered) % 2:
        return ordered[midpoint]
    return (ordered[midpoint - 1] + ordered[midpoint]) / 2


def _percentile_upper(values: list[float], confidence: float = 0.95) -> float:
    values.sort()
    # Nearest-rank is conservative and deterministic for the one-sided bound.
    index = max(0, math.ceil(confidence * len(values)) - 1)
    return values[index]


def _derived_rng(seed: int, label: str) -> random.Random:
    digest = hashlib.sha256(f"{seed}\0{label}".encode("utf-8")).digest()
    return random.Random(int.from_bytes(digest[:16], "big"))


def _bootstrap_upper(
    log_ratios: Sequence[float], bootstrap_samples: int, seed: int, label: str
) -> float:
    if len(log_ratios) == 1:
        return math.exp(log_ratios[0])
    generator = _derived_rng(seed, label)
    count = len(log_ratios)
    means = [
        math.exp(
            math.fsum(log_ratios[generator.randrange(count)] for _ in range(count))
            / count
        )
        for _ in range(bootstrap_samples)
    ]
    return _percentile_upper(means)


def _global_bootstrap_upper(
    workloads: Mapping[str, Sequence[float]], bootstrap_samples: int, seed: int
) -> float:
    if sum(len(values) for values in workloads.values()) == 1:
        only_value = next(iter(workloads.values()))[0]
        return math.exp(only_value)
    generator = _derived_rng(seed, "global-stratified")
    bootstrapped: list[float] = []
    ordered = [workloads[name] for name in sorted(workloads)]
    for _ in range(bootstrap_samples):
        workload_means: list[float] = []
        for values in ordered:
            workload_means.append(
                math.fsum(
                    values[generator.randrange(len(values))] for _ in range(len(values))
                )
                / len(values)
            )
        # Every corpus workload has equal weight, even if a recovered run has a
        # different number of valid pairs for one workload.
        bootstrapped.append(math.exp(math.fsum(workload_means) / len(workload_means)))
    return _percentile_upper(bootstrapped)


def _rss_ratio(rust_peak: int, cpp_peak: int) -> float | None:
    if cpp_peak == 0:
        # A zero baseline is not usable RSS evidence. Treating 0/0 as parity
        # would let an unavailable resource counter satisfy the memory gate.
        return None
    return rust_peak / cpp_peak


def analyze(document: object) -> dict[str, Any]:
    root = _mapping(document, "input")
    raw_samples = root.get("samples")
    if not isinstance(raw_samples, list) or not raw_samples:
        raise StatisticsInputError("samples must be a non-empty JSON array")
    samples = [_parse_sample(value, index) for index, value in enumerate(raw_samples)]
    thresholds = _parse_thresholds(root.get("thresholds"))
    bootstrap_samples = _integer(
        root.get("bootstrap_samples", 10_000), "bootstrap_samples", minimum=1
    )
    seed = _integer(root.get("seed", 0), "seed", minimum=0)

    grouped: dict[str, dict[str, dict[str, Sample]]] = {}
    for sample in samples:
        pair = grouped.setdefault(sample.workload, {}).setdefault(sample.pair, {})
        if sample.backend in pair:
            raise StatisticsInputError(
                f"duplicate {sample.backend} sample for {sample.workload!r} pair {sample.pair}"
            )
        pair[sample.backend] = sample

    failures: list[dict[str, Any]] = []
    workload_results: dict[str, dict[str, Any]] = {}
    logs_by_workload: dict[str, list[float]] = {}
    all_rss_ratios: list[float] = []
    total_pairs = 0

    for workload in sorted(grouped):
        log_ratios: list[float] = []
        rust_first = 0
        cpp_first = 0
        rust_peak = 0
        cpp_peak = 0
        rust_elapsed: list[int] = []
        cpp_elapsed: list[int] = []
        for pair_name, pair in sorted(grouped[workload].items()):
            if set(pair) != {"cpp", "rust"}:
                raise StatisticsInputError(
                    f"{workload!r} pair {pair_name} must contain one cpp and one rust sample"
                )
            cpp = pair["cpp"]
            rust = pair["rust"]
            if {cpp.position, rust.position} != {1, 2}:
                raise StatisticsInputError(
                    f"{workload!r} pair {pair_name} must use positions 1 and 2"
                )
            first, second = sorted((cpp, rust), key=lambda sample: sample.position)
            if second.ordinal != first.ordinal + 1:
                raise StatisticsInputError(
                    f"{workload!r} pair {pair_name} is not adjacent in execution order"
                )
            if rust.position == 1:
                rust_first += 1
            else:
                cpp_first += 1
            ratio = rust.elapsed_ns / cpp.elapsed_ns
            log_ratios.append(math.log(ratio))
            rust_elapsed.append(rust.elapsed_ns)
            cpp_elapsed.append(cpp.elapsed_ns)
            rust_peak = max(rust_peak, rust.max_rss_bytes)
            cpp_peak = max(cpp_peak, cpp.max_rss_bytes)

        pair_count = len(log_ratios)
        if abs(rust_first - cpp_first) > 1 or (
            pair_count > 1 and (rust_first == 0 or cpp_first == 0)
        ):
            raise StatisticsInputError(
                f"{workload!r} backend-first positions are not balanced/interleaved: "
                f"rust={rust_first}, cpp={cpp_first}"
            )

        geometric_mean = _geometric_mean_from_logs(log_ratios)
        upper_ci = _bootstrap_upper(
            log_ratios, bootstrap_samples, seed, f"workload:{workload}"
        )
        memory_ratio = _rss_ratio(rust_peak, cpp_peak)
        if geometric_mean > thresholds.max_workload_ratio:
            failures.append(
                {
                    "gate": "severe_tail",
                    "workload": workload,
                    "actual": geometric_mean,
                    "limit": thresholds.max_workload_ratio,
                }
            )
        if memory_ratio is None or memory_ratio > thresholds.max_rss_ratio:
            failures.append(
                {
                    "gate": "peak_rss",
                    "workload": workload,
                    "actual": memory_ratio,
                    "limit": thresholds.max_rss_ratio,
                }
            )
        if memory_ratio is not None:
            all_rss_ratios.append(memory_ratio)
        workload_results[workload] = {
            "pairs": pair_count,
            "geometric_mean_ratio": geometric_mean,
            "upper_95_ci": upper_ci,
            "median_elapsed_ns": {
                "cpp": _median(cpp_elapsed),
                "rust": _median(rust_elapsed),
            },
            "order": {"cpp_first": cpp_first, "rust_first": rust_first},
            "peak_rss_bytes": {"cpp": cpp_peak, "rust": rust_peak},
            "peak_rss_ratio": memory_ratio,
        }
        logs_by_workload[workload] = log_ratios
        total_pairs += pair_count

    global_geometric_mean = _geometric_mean_from_logs(
        [math.fsum(values) / len(values) for values in logs_by_workload.values()]
    )
    global_upper_ci = _global_bootstrap_upper(logs_by_workload, bootstrap_samples, seed)
    if global_upper_ci > thresholds.global_upper_ci:
        failures.append(
            {
                "gate": "global_upper_95_ci",
                "actual": global_upper_ci,
                "limit": thresholds.global_upper_ci,
            }
        )

    return {
        "schema": 1,
        "status": "pass" if not failures else "fail",
        "failures": failures,
        "configuration": {
            "bootstrap_samples": bootstrap_samples,
            "seed": seed,
            "thresholds": thresholds.as_json(),
            "method": "paired_log_ratio_stratified_bootstrap",
        },
        "global": {
            "workloads": len(workload_results),
            "pairs": total_pairs,
            "geometric_mean_ratio": global_geometric_mean,
            "upper_95_ci": global_upper_ci,
            "max_peak_rss_ratio": max(all_rss_ratios) if all_rss_ratios else None,
        },
        "workloads": workload_results,
    }


def _read_json(path: str) -> object:
    if path == "-":
        return json.load(sys.stdin)
    with Path(path).open("r", encoding="utf-8") as handle:
        return json.load(handle)


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
        description=(
            "Compute paired Rust/C++ log-ratio statistics, a deterministic "
            "one-sided bootstrap 95% upper bound, and tail/RSS gates."
        ),
        epilog=(
            "INPUT is a JSON object with samples [{workload,pair,position,backend,"
            "elapsed_ns,max_rss_bytes}], thresholds {global_upper_ci,"
            "max_workload_ratio,max_rss_ratio}, bootstrap_samples, and seed. "
            "Samples must appear in execution order and each workload must balance "
            "which backend occupies position 1. Exit status is 0 for pass, 1 for a "
            "gate failure, and 2 for invalid evidence."
        ),
    )
    parser.add_argument("--input", default="-", help="input JSON path (default: stdin)")
    parser.add_argument(
        "--output", default="-", help="output JSON path (default: stdout)"
    )
    parser.add_argument("--pretty", action="store_true", help="pretty-print JSON")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parser().parse_args(argv)
    try:
        result = analyze(_read_json(arguments.input))
    except (OSError, json.JSONDecodeError, StatisticsInputError) as error:
        invalid = {"schema": 1, "status": "invalid", "error": str(error)}
        _write_json(invalid, arguments.output, pretty=arguments.pretty)
        return 2
    _write_json(result, arguments.output, pretty=arguments.pretty)
    return 0 if result["status"] == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())

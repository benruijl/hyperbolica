#!/usr/bin/env python3
"""Resource-bounded, failure-preserving exploratory performance matrix.

This does not run builds or modify the locked qualification policy. Each case
gets an exact preflight before any timing pairs; a failed case stays in the
report and does not prevent later cases from running.
"""

from __future__ import annotations

import argparse
import copy
import csv
import fnmatch
import json
import math
import os
import shutil
import statistics
import subprocess
import sys
from pathlib import Path

from performance_matrix_cases import DEFAULT_MATRIX, REPOSITORY, compact, digest_bytes, inventory

MIB = 1024 * 1024


def environment() -> dict[str, str]:
    selected = {name: os.environ[name] for name in ["PATH", "LD_LIBRARY_PATH", "SYMBOLICA_LICENSE"]
                if name in os.environ}
    selected.update(SYMBOLICA_HIDE_BANNER="1", OMP_NUM_THREADS="1", OPENBLAS_NUM_THREADS="1",
                    RAYON_NUM_THREADS="1", HF_MAX_THREADS_PER_CALL="1")
    return selected


def normalized(value: object, ignored: list[str], recursive: list[str]) -> object:
    def walk(node: object) -> object:
        if isinstance(node, dict):
            return {key: walk(item) for key, item in node.items() if key not in recursive}
        if isinstance(node, list):
            return [walk(item) for item in node]
        return node
    return walk({key: item for key, item in value.items() if key not in ignored})


def transport(value: dict, case: dict, raw: bytes) -> bytes:
    if case.get("compare", "byte") == "byte":
        return raw
    return compact(normalized(value, case.get("ignore", []), case.get("ignore_recursive", [])))


class MatrixRunner:
    def __init__(self, arguments: argparse.Namespace):
        self.args = arguments
        self.output = arguments.output.resolve()
        self.env = environment()
        self.prlimit = shutil.which("prlimit")
        if self.prlimit is None:
            raise ValueError("Linux prlimit is required for address-space and output-file bounds")
        self.serial = 0

    def invoke(self, command: list[str], request: bytes, case: dict, label: str,
               *, timeout: float | None = None) -> dict:
        self.serial += 1
        stem = self.output / "raw" / f"{self.serial:06d}.{label}"
        stdout = stem.with_suffix(stem.suffix + ".stdout")
        stderr = stem.with_suffix(stem.suffix + ".stderr")
        command = [self.prlimit, f"--as={int(case['memory_mib'] * MIB)}",
                   f"--fsize={64 * MIB}", "--core=0", "--", *command]
        # A growing matrix parent must not directly fork the measured backend:
        # Linux ru_maxrss would include that inherited pre-exec address space.
        # A fresh, small helper execs first, then measures only its own child.
        # Helper startup and its own resource usage are outside that interval.
        helper = [sys.executable, str(REPOSITORY / "scripts/benchmark_process.py"),
            "--stdout", str(stdout), "--stderr", str(stderr),
            "--timeout-seconds", str(timeout or case["timeout_seconds"]),
            "--terminate-grace-seconds", "0.1", "--cwd", str(REPOSITORY), "--clear-env"]
        for name in self.env:
            helper.extend(["--inherit-env-var", name])
        helper.extend(["--", *command])
        try:
            completed = subprocess.run(helper, input=request, stdout=subprocess.PIPE,
                stderr=subprocess.PIPE, cwd=REPOSITORY, env=self.env, check=False)
            if completed.returncode != 0:
                raise ValueError("measurement helper failed: " + completed.stderr.decode(errors="replace"))
            measurement = json.loads(completed.stdout)
            if not isinstance(measurement, dict) or not all(key in measurement for key in
                    ["elapsed_ns", "user_ns", "sys_ns", "max_rss_bytes", "exit_code", "timed_out"]):
                raise ValueError("measurement helper returned invalid metrics")
        except (OSError, ValueError) as error:
            return {"status": "launch_error", "reason": str(error)}
        record = {**measurement, "stdout": str(stdout), "stderr": str(stderr)}
        if measurement["timed_out"]:
            record["status"] = "timeout"
        elif measurement["exit_code"] != 0:
            # FLINT reports allocation exceptions on stdout; Rust and Python
            # usually use stderr. Read bounded diagnostic prefixes from both.
            diagnostic = ""
            for stream in [stderr, stdout]:
                with stream.open("rb") as source:
                    diagnostic += source.read(65536).decode(errors="replace").lower()
            memory_error = any(fragment in diagnostic.lower() for fragment in
                ["out of memory", "bad_alloc", "cannot allocate memory", "unable to allocate memory",
                 "memory allocation", "memoryerror"])
            record["status"] = "memory_limit_or_allocation_failure" if memory_error else "process_error"
        else:
            record["status"] = "ok"
        return record

    def backend(self, executable: Path, case: dict, label: str) -> dict:
        request = copy.deepcopy(case["request"])
        # This explicit file is identical for both backends and is part of the
        # effective request hash. Never use a cwd-dependent table lookup.
        if request["op"] in {"hyperflint", "integration_step", "zero_one_period", "zero_inf_period",
                              "apply_mzv_reductions", "evaluate_periods", "fibration_basis"}:
            request.setdefault("mzv_data_path", str(REPOSITORY / "data/mzv_reductions.json"))
        result = self.invoke([str(executable), "eval-json"], compact(request) + b"\n", case, label)
        result["effective_request_sha256"] = digest_bytes(compact(request))
        if result["status"] == "ok":
            try:
                value = json.loads(Path(result["stdout"]).read_bytes())
                if not isinstance(value, dict):
                    raise ValueError("response is not a JSON object")
                if "error" in value or value.get("failed") or value.get("divergent"):
                    result.update(status="backend_rejection", response=value)
                else:
                    result["response"] = value
            except (ValueError, UnicodeError) as error:
                result.update(status="invalid_response", reason=str(error))
        return result

    def parse_scalar(self, expression: str, variables: list[str], case: dict) -> str:
        result = self.invoke([str(self.args.parser), "eval-json"],
            compact({"op": "parse_expr", "vars": variables, "expr": expression}) + b"\n",
            case, "validation.scalar", timeout=min(60, case["timeout_seconds"]))
        if result["status"] != "ok":
            raise ValueError("scalar validation " + result["status"])
        response = json.loads(Path(result["stdout"]).read_bytes())
        if not isinstance(response, dict) or not isinstance(response.get("canonical"), str) or "error" in response:
            raise ValueError("scalar parser rejected an algebraic definition")
        return response["canonical"]

    def algebraic(self, response: dict, variables: list[str], case: dict) -> list[dict]:
        output, seen = [], set()
        for entry in response.get("algebraic_letters") or []:
            index = entry["idx"]
            if not isinstance(index, int) or index < 1 or index in seen:
                raise ValueError("invalid/duplicate algebraic root index")
            seen.add(index)
            variable = entry.get("var")
            if variable is None:
                variable = response["vars"][entry["var_idx"]]
            polynomial = entry.get("poly", entry.get("polynomial"))
            discriminant = entry.get("disc", entry.get("discriminant"))
            for key, expected in [("wm", f"Wm_{index}"), ("wp", f"Wp_{index}"),
                                  ("wm_over_wp", f"WmOverWp_{index}")]:
                if key in entry and entry[key] != expected:
                    raise ValueError("incorrect algebraic root symbol")
            parse = lambda expression: self.parse_scalar(expression, variables, case)
            leading = parse(f"({polynomial})/(({variable})^2-({entry['sum']})*({variable})+({entry['product']}))")
            if "lc" in entry and parse(entry["lc"]) != leading:
                raise ValueError("algebraic leading coefficient is inconsistent")
            if parse(f"({discriminant})-({leading})^2*((({entry['sum']})^2)-4*({entry['product']}))") != "0":
                raise ValueError("algebraic discriminant/Vieta identity is inconsistent")
            known = {"idx", "var", "var_idx", "poly", "polynomial", "disc", "discriminant",
                     "lc", "sum", "product", "wm", "wp", "wm_over_wp"}
            output.append({"index": index, "variable": variable, "polynomial": parse(polynomial),
                "discriminant": parse(discriminant), "sum": parse(entry["sum"]),
                "product": parse(entry["product"]), "leading": leading,
                "extra": {key: value for key, value in entry.items() if key not in known}})
        return sorted(output, key=lambda entry: entry["index"])

    def preflight(self, case: dict, records: dict) -> dict:
        variables = list(dict.fromkeys([*case["request"].get("vars", []),
            *records["reference"]["response"].get("vars", []),
            *records["candidate"]["response"].get("vars", [])])) or ["matrix_dummy"]
        # Parser variables are transport spellings, as in the locked harness.
        fixture = {**case, "semantic_variables": variables}
        fixture_path = self.output / "raw" / (case["name"] + ".comparison.json")
        fixture_path.write_bytes(compact(fixture))
        values, definitions = {}, {}
        for name, record in records.items():
            result = self.invoke(["bash", str(REPOSITORY / "scripts/performance_matrix_compare.sh"),
                record["stdout"], str(fixture_path), str(self.args.parser)], b"", case,
                "validation." + name, timeout=min(60, case["timeout_seconds"]))
            if result["status"] != "ok":
                raise ValueError(name + " response canonicalization " + result["status"])
            values[name] = Path(result["stdout"]).read_bytes()
            definitions[name] = self.algebraic(record["response"], variables, case)
        return {"status": "equal" if values["reference"] == values["candidate"] and
                definitions["reference"] == definitions["candidate"] else "mismatch",
            "response_sha256": {name: digest_bytes(value) for name, value in values.items()},
            "algebraic_definitions": definitions}

    def run_case(self, case: dict) -> dict:
        record = {key: case[key] for key in ["name", "tier", "request_sha256", "attachment_index",
                  "source_name", "timeout_seconds", "memory_mib"] if key in case}
        if case.get("availability"):
            return {**record, "status": case["availability"], "reason": case["reason"]}
        if not selected(case, self.args):
            return {**record, "status": "not_selected"}
        backends = {"reference": self.args.reference, "candidate": self.args.candidate}
        first = {name: self.backend(path, case, "preflight." + name) for name, path in backends.items()}
        record["preflight_runs"] = first
        if any(result["status"] != "ok" for result in first.values()):
            return {**record, "status": "preflight_failed"}
        try:
            record["comparison"] = self.preflight(case, first)
        except (ValueError, KeyError, TypeError, IndexError) as error:
            return {**record, "status": "validation_failed", "reason": str(error)}
        if record["comparison"]["status"] != "equal":
            return {**record, "status": "mismatch"}
        expected = {name: transport(run["response"], case, Path(run["stdout"]).read_bytes())
                    for name, run in first.items()}
        expected_metadata = {name: compact({key: run["response"].get(key)
            for key in ["vars", "algebraic_letters"]}) for name, run in first.items()}
        for run in first.values():
            run.pop("response", None)  # Raw output files retain the full mathematical result.
        samples = []
        for pair in range(-self.args.warmup, self.args.pairs):
            order = list(backends) if pair % 2 == 0 else list(reversed(backends))
            current = []
            for position, name in enumerate(order):
                run = self.backend(backends[name], case, ("warmup." if pair < 0 else "sample.") + name)
                run.update(backend=name, pair=pair + 1, position=position + 1)
                if run["status"] == "ok" and transport(run["response"], case,
                        Path(run["stdout"]).read_bytes()) != expected[name]:
                    run["status"] = "nondeterministic_response"
                if run["status"] == "ok" and compact({key: run["response"].get(key)
                        for key in ["vars", "algebraic_letters"]}) != expected_metadata[name]:
                    run["status"] = "nondeterministic_algebraic_context"
                run.pop("response", None)
                current.append(run)
            if pair >= 0:
                samples.extend(current)
            if any(run["status"] != "ok" for run in current):
                return {**record, "status": "sampling_failed", "samples": samples, "failed_runs": current}
        by_backend = {name: [run for run in samples if run["backend"] == name] for name in backends}
        medians = {name: statistics.median(run["elapsed_ns"] for run in runs) / 1e6
                   for name, runs in by_backend.items()}
        ratios = [by_backend["candidate"][i]["elapsed_ns"] / by_backend["reference"][i]["elapsed_ns"]
                  for i in range(self.args.pairs)]
        return {**record, "status": "measured", "samples": samples, "median_ms": medians,
            "paired_candidate_over_reference": math.exp(statistics.mean(map(math.log, ratios))),
            "pair_ratio_min": min(ratios), "pair_ratio_max": max(ratios),
            "max_rss_mib": {name: max(run["max_rss_bytes"] for run in runs) / MIB
                            for name, runs in by_backend.items()}}


def selected(case: dict, arguments: argparse.Namespace) -> bool:
    if arguments.case:
        return any(fnmatch.fnmatchcase(case["name"], pattern) for pattern in arguments.case)
    return "all" in arguments.tier or case["tier"] in arguments.tier


def write_reports(output: Path, records: list[dict], metadata: dict) -> None:
    (output / "results.json").write_text(json.dumps({"metadata": metadata, "cases": records}, indent=2) + "\n")
    with (output / "summary.csv").open("w", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(["name", "tier", "status", "reference_ms", "candidate_ms", "paired_ratio",
                         "reference_rss_mib", "candidate_rss_mib", "reference_preflight", "candidate_preflight"])
        for record in records:
            timing, rss = record.get("median_ms", {}), record.get("max_rss_mib", {})
            first = record.get("preflight_runs", {})
            writer.writerow([record["name"], record["tier"], record["status"], timing.get("reference"),
                timing.get("candidate"), record.get("paired_candidate_over_reference"), rss.get("reference"),
                rss.get("candidate"), first.get("reference", {}).get("status"), first.get("candidate", {}).get("status")])
    lines = ["# Exploratory performance matrix", "", "Every inventory entry appears below. Times are cold-process wall-time medians; ratio is the geometric mean of adjacent candidate/reference pairs. A ratio below 1 favors the candidate. Missing equality or incomplete samples produce no ratio. This is not a qualification/parity gate.", "",
        "| Case | Tier | Status | Reference ms | Candidate ms | Paired ratio | Peak MiB ref/candidate |",
        "|---|---|---|---:|---:|---:|---:|"]
    for record in records:
        timing, rss = record.get("median_ms", {}), record.get("max_rss_mib", {})
        cell = lambda value: f"{value:.3f}" if value is not None else "—"
        status = record["status"]
        if "preflight_runs" in record and status == "preflight_failed":
            status += ": " + "/".join(run["status"] for run in record["preflight_runs"].values())
        lines.append(f"| `{record['name']}` | {record['tier']} | {status} | {cell(timing.get('reference'))} | {cell(timing.get('candidate'))} | {cell(record.get('paired_candidate_over_reference'))} | {cell(rss.get('reference'))}/{cell(rss.get('candidate'))} |")
    (output / "overview.md").write_text("\n".join(lines) + "\n")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--matrix", type=Path, default=DEFAULT_MATRIX)
    result.add_argument("--attachment", type=Path)
    result.add_argument("--list", action="store_true", help="resolve full inventory without launching processes")
    result.add_argument("--tier", action="append", default=[], choices=["core", "api", "scaling", "attachment", "heavy", "pipeline", "all"])
    result.add_argument("--case", action="append", default=[], help="case glob; overrides tier selection")
    result.add_argument("--reference", type=Path, help="reference executable or exec launcher")
    result.add_argument("--candidate", type=Path, help="candidate executable or exec launcher")
    result.add_argument("--parser", type=Path, help="Symbolica-backed canonicalizer; defaults to candidate")
    result.add_argument("--artifact", action="append", default=[], type=Path, help="also hash an underlying binary/runtime library/build manifest")
    result.add_argument("--pairs", type=int, default=3)
    result.add_argument("--warmup", type=int, default=1)
    result.add_argument("--cpu", type=int, help="pin driver and all descendants to one allowed logical CPU")
    result.add_argument("--output", type=Path, help="new evidence directory (must not already exist)")
    return result


def main(argv: list[str] | None = None) -> int:
    arguments = parser().parse_args(argv)
    if not arguments.tier:
        arguments.tier = ["core", "scaling"]
    try:
        cases, provenance = inventory(arguments.matrix, arguments.attachment)
        if arguments.pairs < 1 or arguments.warmup < 0:
            raise ValueError("pairs must be >=1 and warmup must be >=0")
        if arguments.case and not any(selected(case, arguments) for case in cases):
            raise ValueError("case selection matches no inventory entries")
        if arguments.list:
            print(json.dumps({"provenance": provenance, "cases": cases}, indent=2))
            return 0
        if arguments.reference is None or arguments.candidate is None or arguments.output is None:
            raise ValueError("running requires --reference, --candidate, and --output")
        for key in ["reference", "candidate", "parser"]:
            value = getattr(arguments, key) or arguments.candidate
            value = value.resolve()
            if not value.is_file() or not os.access(value, os.X_OK):
                raise ValueError(f"{key} executable is unavailable: {value}")
            setattr(arguments, key, value)
        if arguments.cpu is not None:
            if arguments.cpu not in os.sched_getaffinity(0):
                raise ValueError("selected CPU is outside the current allowed affinity")
            os.sched_setaffinity(0, {arguments.cpu})
        artifacts = list(dict.fromkeys([arguments.reference, arguments.candidate, arguments.parser,
            Path(__file__).resolve(), REPOSITORY / "scripts/performance_matrix_cases.py",
            REPOSITORY / "scripts/performance_matrix_compare.sh", REPOSITORY / "scripts/benchmark_process.py",
            REPOSITORY / "scripts/lib/response-comparison.sh", REPOSITORY / "scripts/normalize-response.jq",
            REPOSITORY / "data/mzv_reductions.json", *arguments.artifact]))
        metadata = {**provenance, "mode": "exploratory", "pairs": arguments.pairs, "warmup": arguments.warmup,
            "cpu": arguments.cpu, "threads": 1, "tiers": arguments.tier, "case_patterns": arguments.case,
            "artifacts": [{"path": str(path.resolve()), "sha256": digest_bytes(path.read_bytes())} for path in artifacts],
            "memory_bound": "Linux RLIMIT_AS per process, inherited by descendants (not aggregate RSS)",
            "output_bound_bytes_per_file": 64 * MIB, "timeout_scope": "whole process group",
            "symbolica_license_inherited": "SYMBOLICA_LICENSE" in os.environ}
        runner = MatrixRunner(arguments)
        arguments.output.mkdir(parents=True, exist_ok=False)
        (arguments.output / "raw").mkdir()
        (arguments.output / "inventory.json").write_text(json.dumps(cases, indent=2) + "\n")
        records = []
        for case in cases:
            record = runner.run_case(case)
            records.append(record)
            write_reports(arguments.output, records, metadata)
            print(case["name"] + ": " + record["status"], flush=True)
        return 1 if any(record["status"] in {"preflight_failed", "validation_failed", "mismatch", "sampling_failed"}
                        for record in records) else 0
    except (OSError, ValueError) as error:
        print("performance-matrix: " + str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())

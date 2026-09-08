"""License-free coverage of the independent resource-bounded matrix harness."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

REPOSITORY = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPOSITORY / "scripts"))

from performance_matrix import MatrixRunner, environment, main, selected, write_reports
from performance_matrix_cases import DEFAULT_MATRIX, compact, dense_polynomial, digest_bytes, inventory, scaling_cases


class InventoryTests(unittest.TestCase):
    def test_full_inventory_keeps_all_attachment_entries_without_the_file(self):
        cases, provenance = inventory(DEFAULT_MATRIX, None)
        manifest = json.loads(DEFAULT_MATRIX.read_text())
        source_count = sum(len([line for line in (REPOSITORY / source["path"]).read_text().splitlines()
                                if line.strip()]) for source in manifest["sources"])
        self.assertEqual(len(cases), source_count + len(scaling_cases(manifest["scaling"]))
                         + manifest["attachment"]["expected_cases"])
        self.assertIn(f"resolves {len(cases)} workloads", (REPOSITORY / "docs/performance-matrix.md").read_text())
        self.assertEqual(len({case["name"] for case in cases}), len(cases))
        self.assertEqual(sum(case["tier"] == "api" for case in cases), 79)
        attached = [case for case in cases if "attachment_index" in case]
        self.assertEqual(sorted(case["attachment_index"] for case in attached), list(range(33)))
        self.assertEqual(sum(case.get("availability") == "attachment_missing" for case in attached), 10)
        self.assertEqual(sum(case.get("availability") == "requires_subtropica_preprocessing" for case in attached), 23)
        self.assertEqual(len(provenance["sources"]), 2)
        self.assertTrue(all(case["timeout_seconds"] > 0 and case["memory_mib"] > 0
                            for case in cases if "request" in case))

    def test_wrong_attachment_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "incorrect.json"
            path.write_text('{"cases": []}')
            with self.assertRaisesRegex(ValueError, "SHA-256"):
                inventory(DEFAULT_MATRIX, path)

    def test_root_attachment_requests_use_the_bridge_algebraic_option(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = json.loads(DEFAULT_MATRIX.read_text())
            source_cases = [{"name": f"case{index}", "variables": ["x3", "x4"],
                "parameters": ["mm", "s12", "s23"], "integration_order": ["x4", "x3"],
                "integrand": "1/(1+x3+x4)"} for index in range(33)]
            source = root / "attachment.json"
            source.write_bytes(compact({"cases": source_cases}))
            manifest["attachment"]["sha256"] = digest_bytes(source.read_bytes())
            matrix = root / "matrix.json"
            matrix.write_bytes(compact(manifest))
            cases, _ = inventory(matrix, source)
            for name in ["attachment.findroots21_a", "attachment.findroots21_b"]:
                request = next(case["request"] for case in cases if case["name"] == name)
                self.assertEqual(request, {"op": "hyperflint", "vars": ["x3", "x4", "mm", "s12", "s23"],
                    "vars_int": ["x4", "x3"], "parallel": False, "check_divergences": False,
                    "f": "1/(1+x3+x4)", "algebraic_letters": True})

    def test_explicit_case_selection_overrides_default_tiers(self):
        arguments = argparse.Namespace(case=["attachment.tst*"], tier=["core", "scaling"])
        self.assertTrue(selected({"name": "attachment.tst4", "tier": "heavy"}, arguments))
        self.assertFalse(selected({"name": "scale.multiply.d8.v1", "tier": "scaling"}, arguments))

    def test_environment_omits_ambient_algorithm_overrides(self):
        with patch.dict(os.environ, {"HF_PERIOD_TUPLES": "0", "RAYON_NUM_THREADS": "31"}):
            value = environment()
            self.assertNotIn("HF_PERIOD_TUPLES", value)
            self.assertEqual(value["RAYON_NUM_THREADS"], "1")

    def test_dense_and_rational_families_use_supported_entry_points(self):
        self.assertEqual(len(dense_polynomial(["x", "y"], 4).split("+")), 15)
        self.assertEqual(len(dense_polynomial(["x", "y", "z"], 4).split("+")), 35)
        cases, _ = inventory(DEFAULT_MATRIX, None)
        powers = [case for case in cases if case["name"].startswith("scale.parse_rational_power.")]
        self.assertTrue(any("^(-" in case["request"]["expr"] for case in powers))
        self.assertTrue(all(case["request"]["op"] == "parse_expr" for case in powers))
        derivatives = [case for case in cases if case["name"].startswith("scale.rational_derivative.")]
        self.assertTrue(all(case["request"]["op"] == "differentiate_wordlist" and
                            case["request"]["wl"][0]["word"] == [] for case in derivatives))


@unittest.skipUnless(shutil.which("prlimit"), "requires Linux prlimit")
class RunnerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.mock = self.root / "backend"
        self.mock.write_text(f"#!{sys.executable}\n" + """import json, sys, time
request = json.load(sys.stdin)
if request.get('reject'):
    print(json.dumps({'failed': True, 'reason': 'unsupported test input'}))
elif request.get('memory'):
    value = bytearray(256 * 1024 * 1024)
elif request.get('flint_memory'):
    print('FLINT exception (General error): Unable to allocate memory (536870912).')
    sys.exit(1)
elif request.get('wait'):
    time.sleep(30)
else:
    print(json.dumps({'op': request['op'], 'result': '1'}))
""")
        self.mock.chmod(0o700)
        self.output = self.root / "evidence"
        self.output.mkdir()
        (self.output / "raw").mkdir()
        self.arguments = argparse.Namespace(output=self.output, reference=self.mock, candidate=self.mock,
            parser=self.mock, warmup=0, pairs=2, tier=["core"], case=[])
        self.runner = MatrixRunner(self.arguments)

    def case(self, name="test", **request):
        return {"name": name, "tier": "core", "timeout_seconds": 2,
            "memory_mib": 128, "compare": "byte", "request": {"op": "test", **request}}

    def test_equal_preflight_collects_alternating_pairs(self):
        record = self.runner.run_case(self.case())
        self.assertEqual(record["status"], "measured")
        self.assertEqual([run["backend"] for run in record["samples"]],
                         ["reference", "candidate", "candidate", "reference"])
        self.assertEqual(record["comparison"]["status"], "equal")
        self.assertGreater(record["paired_candidate_over_reference"], 0)

    def test_backend_rejection_has_no_false_timing_ratio_and_next_case_runs(self):
        failure = self.runner.run_case(self.case("rejected", reject=True))
        success = self.runner.run_case(self.case("next"))
        self.assertEqual(failure["status"], "preflight_failed")
        self.assertEqual(failure["preflight_runs"]["candidate"]["status"], "backend_rejection")
        self.assertNotIn("paired_candidate_over_reference", failure)
        self.assertEqual(success["status"], "measured")
        write_reports(self.output, [failure, success], {})
        report = json.loads((self.output / "results.json").read_text())
        self.assertEqual(len(report["cases"]), 2)
        self.assertIn("rejected", (self.output / "overview.md").read_text())

    def test_timeout_and_address_space_are_enforced(self):
        case = self.case("timeout", wait=True)
        case["timeout_seconds"] = 0.1
        result = self.runner.backend(self.mock, case, "timeout")
        self.assertEqual(result["status"], "timeout")
        result = self.runner.backend(self.mock, self.case("memory", memory=True), "memory")
        self.assertEqual(result["status"], "memory_limit_or_allocation_failure")

    def test_flint_allocation_diagnostic_on_stdout_is_classified_without_a_ratio(self):
        result = self.runner.run_case(self.case("flint_memory", flint_memory=True))
        self.assertEqual(result["status"], "preflight_failed")
        self.assertEqual(result["preflight_runs"]["reference"]["status"],
                         "memory_limit_or_allocation_failure")
        self.assertNotIn("paired_candidate_over_reference", result)

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux ru_maxrss regression")
    def test_backend_rss_is_not_inherited_from_a_large_matrix_parent(self):
        allocation = bytearray(96 * 1024 * 1024)
        for offset in range(0, len(allocation), 4096):
            allocation[offset] = 1
        result = self.runner.backend(self.mock, self.case("small_child"), "small_child")
        self.assertEqual(result["status"], "ok")
        self.assertLess(result["max_rss_bytes"], 64 * 1024 * 1024)
        self.assertEqual(allocation[0], 1)  # Keep the resident allocation live across the call.

    def test_timeout_reaches_descendants_in_the_same_process_group(self):
        code = "import subprocess,sys,time; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); print(child.pid,flush=True); time.sleep(30)"
        result = self.runner.invoke([sys.executable, "-c", code], b"", self.case(), "descendant", timeout=0.2)
        self.assertEqual(result["status"], "timeout")
        child = int(Path(result["stdout"]).read_text())
        state = Path(f"/proc/{child}/stat")
        if state.exists():
            # An orphan can remain a zombie until PID 1 reaps it; it cannot
            # execute or consume memory. A live descendant is a real failure.
            self.assertEqual(state.read_text().split()[2], "Z")

    def test_mismatched_responses_are_not_timed(self):
        other = self.root / "different"
        other.write_text(f"#!{sys.executable}\nprint('{{\"op\":\"test\",\"result\":\"2\"}}')\n")
        other.chmod(0o700)
        self.arguments.candidate = other
        result = self.runner.run_case(self.case())
        self.assertEqual(result["status"], "mismatch")
        self.assertNotIn("samples", result)

    def test_ignored_context_fields_still_require_sample_determinism(self):
        counter = self.root / "counter"
        self.mock.write_text(f"#!{sys.executable}\n" +
            "import json\nfrom pathlib import Path\n" +
            f"path=Path({str(counter)!r})\n" +
            "count=int(path.read_text()) if path.exists() else 0\n" +
            "path.write_text(str(count+1))\n" +
            "print(json.dumps({'op':'test','result':'1','vars':[str(count)]}))\n")
        case = self.case()
        case.update(compare="normalized", ignore=["vars", "algebraic_letters"])
        result = self.runner.run_case(case)
        self.assertEqual(result["comparison"]["status"], "equal")
        self.assertEqual(result["status"], "sampling_failed")
        self.assertTrue(all(run["status"] == "nondeterministic_algebraic_context"
                            for run in result["failed_runs"]))

    def test_opaque_or_incorrect_algebraic_symbols_cannot_be_ignored(self):
        response = {"vars": ["x"], "algebraic_letters": [{"idx": 1, "var_idx": 0, "wm": "Wm_2"}]}
        with self.assertRaisesRegex(ValueError, "root symbol"):
            self.runner.algebraic(response, ["x"], self.case())


if __name__ == "__main__":
    unittest.main()

"""Licensed installed-extension integration tests.

The suite is skipped unless ``HYPERBOLICA_RUN_LICENSED_TESTS=1``. When opted
in, one advisory file lock is held for the entire suite so independent CI jobs
configured with the same ``HYPERBOLICA_LICENSE_LOCK`` path do not contend for
the same Symbolica license server port.
"""

from __future__ import annotations

import copy
import os
import tempfile
import unittest
from typing import BinaryIO

import hyperbolica as hb


RUN_LICENSED = os.environ.get("HYPERBOLICA_RUN_LICENSED_TESTS") == "1"


@unittest.skipUnless(
    RUN_LICENSED,
    "set HYPERBOLICA_RUN_LICENSED_TESTS=1 to execute licensed integrations",
)
class LicensedIntegrationTests(unittest.TestCase):
    _lock_file: BinaryIO | None = None

    @classmethod
    def setUpClass(cls) -> None:
        try:
            import fcntl
        except ImportError as error:
            raise unittest.SkipTest(
                "licensed-suite serialization currently requires POSIX fcntl"
            ) from error

        lock_path = os.environ.get(
            "HYPERBOLICA_LICENSE_LOCK",
            os.path.join(tempfile.gettempdir(), "hyperbolica-symbolica-license.lock"),
        )
        cls._lock_file = open(lock_path, "a+b")
        fcntl.flock(cls._lock_file.fileno(), fcntl.LOCK_EX)
        if not hb.is_licensed():
            fcntl.flock(cls._lock_file.fileno(), fcntl.LOCK_UN)
            cls._lock_file.close()
            cls._lock_file = None
            raise unittest.SkipTest("the embedded Symbolica kernel is not licensed")

    @classmethod
    def tearDownClass(cls) -> None:
        if cls._lock_file is None:
            return
        import fcntl

        fcntl.flock(cls._lock_file.fileno(), fcntl.LOCK_UN)
        cls._lock_file.close()
        cls._lock_file = None

    def test_direct_integration_returns_the_shipped_expression_type(self) -> None:
        x = hb.S("python_licensed_direct_x")
        result = hb.integrate(1 / (x + 1) ** 2, [x])
        self.assertIs(type(result), hb.Expression)
        self.assertEqual(result.to_canonical_string(), hb.N(1).to_canonical_string())

    def test_detailed_result_is_inspectable_and_copyable(self) -> None:
        x = hb.S("python_licensed_detailed_x")
        result = hb.integrate_detailed(1 / (x + 1) ** 2, [x])

        self.assertIs(type(result.expression), hb.Expression)
        self.assertEqual(result.variable_count, 1)
        self.assertEqual(result.indeterminate_count, len(result.indeterminates))
        self.assertEqual(result.algebraic_letter_count, len(result.algebraic_letters))
        self.assertEqual(result.term_count, len(result))
        self.assertIsNot(copy.copy(result), result)
        self.assertIsNot(copy.deepcopy(result), result)

    def test_prepared_input_captures_independent_default_options(self) -> None:
        x = hb.S("python_licensed_prepared_x")
        options = hb.IntegrationOptions(parallel=False)
        prepared = hb.prepare(1 / (x + 1) ** 2, [x], options)
        options.parallel = True

        self.assertFalse(prepared.options.parallel)
        self.assertEqual(prepared.variable_count, 1)
        self.assertEqual(prepared.indeterminate_count, len(prepared.indeterminates))
        self.assertEqual(prepared.shuffle_term_count, len(prepared))
        self.assertIsNot(copy.copy(prepared), prepared)
        self.assertIsNot(copy.deepcopy(prepared), prepared)
        self.assertIs(type(prepared.integrate()), hb.Expression)

    def test_builtin_log_accepts_exact_rational_arguments(self) -> None:
        x = hb.S("python_licensed_log_x")
        for argument in (x + 1, x * (x + 1), (x + 1) / (x + 2)):
            prepared = hb.prepare(argument.log(), [x])
            self.assertEqual(prepared.variable_count, 1)
            self.assertGreater(prepared.shuffle_term_count, 0)

        with self.assertRaisesRegex(hb.InputError, "nested Hlog/log"):
            hb.prepare((x + 1).log().log(), [x])

    def test_divergence_error_retains_structured_fields(self) -> None:
        x = hb.S("python_licensed_divergent_x")
        options = hb.IntegrationOptions(check_divergences=True, parallel=False)
        with self.assertRaises(hb.DivergentIntegralError) as caught:
            hb.integrate(1 / x, [x], options)

        error = caught.exception
        self.assertIn(error.boundary, {"zero", "infinity"})
        self.assertEqual(error.variable, "python_licensed_divergent_x")
        self.assertIsInstance(error.log_power, int)
        self.assertIsInstance(error.power, int)


if __name__ == "__main__":
    unittest.main(verbosity=2)

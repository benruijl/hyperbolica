"""Unlicensed contract tests for an installed Hyperbolica extension.

These tests exercise type identity, metadata, signatures, copies, and input
validation only. They intentionally stop before exact integration starts, so
they are suitable for ordinary wheel smoke jobs without a Symbolica license.
"""

from __future__ import annotations

import copy
import inspect
import pickle
import sys
import types
import unittest

import hyperbolica as hb


class InstalledExtensionContractTests(unittest.TestCase):
    def test_package_metadata_and_public_exports_are_available(self) -> None:
        self.assertRegex(hb.__version__, r"^\d+\.\d+\.\d+")
        self.assertEqual(hb.__api_version__, 1)
        self.assertEqual(hb.__symbolica_version__, hb.get_version())
        self.assertIn("Hyperbolica code is MIT", hb.__license__)
        self.assertIn("express prior permission", hb.__symbolica_license__)

        required = {
            "Expression",
            "IntegrationOptions",
            "IntegrationResult",
            "PreparedIntegral",
            "__all__",
            "__api_version__",
            "__license__",
            "__symbolica_license__",
            "__symbolica_version__",
            "__version__",
            "integrate",
            "integrate_detailed",
            "integrate_detailed_over",
            "integrate_over",
            "prepare",
        }
        self.assertTrue(required.issubset(hb.__all__))
        self.assertEqual(hb.__all__, sorted(set(hb.__all__)))

    def test_shipped_symbolica_constructors_share_one_expression_type(self) -> None:
        x = hb.S("python_identity_x")
        parsed = hb.E("python_identity_x+1")
        number = hb.N(2)
        arithmetic = (x + number) ** 2

        self.assertIs(type(x), hb.Expression)
        self.assertIs(type(parsed), hb.Expression)
        self.assertIs(type(number), hb.Expression)
        self.assertIs(type(arithmetic), hb.Expression)

    def test_expression_copy_and_pickle_stay_in_the_embedded_kernel(self) -> None:
        expression = (hb.S("python_pickle_x") + 1) ** 2
        canonical = expression.to_canonical_string()

        previous_symbolica = sys.modules.get("symbolica")
        sentinel = types.ModuleType("symbolica")
        sys.modules["symbolica"] = sentinel
        symbolica_modules = {
            name for name in sys.modules if name == "symbolica" or name.startswith("symbolica.")
        }
        try:
            values = [
                copy.copy(expression),
                copy.deepcopy(expression),
                pickle.loads(pickle.dumps(expression)),
            ]
            for value in values:
                self.assertIs(type(value), hb.Expression)
                self.assertEqual(value.to_canonical_string(), canonical)
            self.assertIs(sys.modules["symbolica"], sentinel)
            self.assertEqual(
                {
                    name
                    for name in sys.modules
                    if name == "symbolica" or name.startswith("symbolica.")
                },
                symbolica_modules,
            )
        finally:
            if previous_symbolica is None:
                del sys.modules["symbolica"]
            else:
                sys.modules["symbolica"] = previous_symbolica

    def test_python_call_signatures_are_stable(self) -> None:
        self.assertEqual(
            str(inspect.signature(hb.integrate)),
            "(expression, variables, options=None)",
        )
        self.assertEqual(
            str(inspect.signature(hb.integrate_detailed)),
            "(expression, variables, options=None)",
        )
        self.assertEqual(
            str(inspect.signature(hb.integrate_over)),
            "(expression, variables, intervals, options=None)",
        )
        self.assertEqual(
            str(inspect.signature(hb.integrate_detailed_over)),
            "(expression, variables, intervals, options=None)",
        )
        self.assertEqual(
            str(inspect.signature(hb.prepare)),
            "(expression, variables, options=None)",
        )
        self.assertEqual(
            str(inspect.signature(hb.PreparedIntegral.integrate)),
            "(self, options=None)",
        )
        self.assertIn(
            "check_divergences=False", hb.IntegrationOptions.__text_signature__
        )
        self.assertIn("native", hb.prepare.__doc__.lower())
        self.assertIn("interval", hb.integrate_over.__doc__.lower())
        self.assertIn("materialize", hb.IntegrationResult.expression.__doc__.lower())

    def test_exception_hierarchy_is_programmatic(self) -> None:
        self.assertTrue(issubclass(hb.InputError, hb.HyperbolicaError))
        self.assertTrue(issubclass(hb.DuplicateVariableError, hb.InputError))
        self.assertTrue(issubclass(hb.AlgebraError, hb.HyperbolicaError))
        self.assertTrue(issubclass(hb.ContextError, hb.AlgebraError))
        self.assertTrue(issubclass(hb.DivergentIntegralError, hb.HyperbolicaError))

    def test_non_symbol_variable_stops_at_structural_validation(self) -> None:
        expression = hb.E("python_validation_x+1")
        with self.assertRaisesRegex(
            hb.InputError, "must be a plain Symbolica symbol"
        ):
            hb.prepare(expression, [expression])

    def test_duplicate_variable_error_exposes_the_variable(self) -> None:
        x = hb.S("python_duplicate_x")
        with self.assertRaises(hb.DuplicateVariableError) as caught:
            hb.prepare(x + 1, [x, x])
        self.assertEqual(caught.exception.variable, "python_duplicate_x")

    def test_foreign_objects_are_rejected_by_the_native_boundary(self) -> None:
        x = hb.S("python_type_error_x")
        with self.assertRaises(TypeError):
            hb.prepare("python_type_error_x+1", [x])
        with self.assertRaises(TypeError):
            hb.prepare(x + 1, ["python_type_error_x"])
        with self.assertRaises(TypeError):
            hb.integrate(x + 1, [x], object())

    def test_non_real_infinity_is_rejected_before_integration(self) -> None:
        x = hb.S("python_complex_infinity_bound_x")
        with self.assertRaisesRegex(hb.InputError, "real directed infinities"):
            hb.integrate_over(
                hb.N(1),
                [x],
                [(hb.Symbol.COMPLEX_INFINITY, hb.Symbol.INFINITY)],
            )

    def test_interval_count_must_match_the_variable_schedule(self) -> None:
        x = hb.S("python_interval_count_x")
        with self.assertRaisesRegex(hb.InputError, "interval count 0"):
            hb.integrate_over(hb.N(1), [x], [])

    def test_options_are_value_objects_with_independent_copies(self) -> None:
        options = hb.IntegrationOptions(
            check_divergences=True,
            parallel=False,
            introduce_algebraic_letters=True,
            close_final_positive_letters=False,
            mzv_reductions=[("MZV3", "zeta3")],
            mzv_basis=["zeta3"],
        )
        shallow = copy.copy(options)
        deep = copy.deepcopy(options)

        self.assertEqual(options, shallow)
        self.assertEqual(options, deep)
        self.assertIsNot(options, shallow)
        self.assertIsNot(options, deep)
        shallow.mzv_basis.append("does_not_escape_the_getter")
        self.assertEqual(options.mzv_basis, ["zeta3"])
        deep.parallel = True
        self.assertFalse(options.parallel)

        namespace = {"IntegrationOptions": hb.IntegrationOptions}
        rendered = repr(options)
        self.assertEqual(eval(rendered, namespace), options)
        self.assertIn('mzv_reductions=[("MZV3", "zeta3")]', rendered)

    def test_mzv_defaults_are_embedded_and_explicit_empty_is_preserved(self) -> None:
        standard = hb.IntegrationOptions()
        self.assertGreater(len(standard.mzv_reductions), 100)
        self.assertGreater(len(standard.mzv_basis), 1)
        standard_repr = repr(standard)
        self.assertLess(len(standard_repr), 300)
        self.assertNotIn("mzv_reductions", standard_repr)
        self.assertNotIn("mzv_basis", standard_repr)
        self.assertEqual(
            eval(standard_repr, {"IntegrationOptions": hb.IntegrationOptions}),
            standard,
        )

        explicit_empty = hb.IntegrationOptions(mzv_reductions=[])
        self.assertEqual(explicit_empty.mzv_reductions, [])
        self.assertEqual(explicit_empty.mzv_basis, [])
        self.assertNotEqual(standard, explicit_empty)
        empty_repr = repr(explicit_empty)
        self.assertIn("mzv_reductions=[]", empty_repr)
        self.assertEqual(
            eval(empty_repr, {"IntegrationOptions": hb.IntegrationOptions}),
            explicit_empty,
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)

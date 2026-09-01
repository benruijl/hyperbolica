"""Static-only usage sample checked against the packaged PEP 561 stub."""

# pyright: reportMissingModuleSource=false

from __future__ import annotations

import copy

import hyperbolica as hb


x: hb.Expression = hb.S("typing_x")
integrand: hb.Expression = 1 / (x + 1) ** 2
options = hb.IntegrationOptions(parallel=False, mzv_basis=["zeta3"])
options.check_divergences = True
reductions: list[tuple[str, str]] = options.mzv_reductions
basis: list[str] = copy.deepcopy(options).mzv_basis

prepared: hb.PreparedIntegral = hb.prepare(integrand, [x], options)
prepared_options: hb.IntegrationOptions = prepared.options
prepared_variables: list[hb.Expression] = prepared.integration_variables
prepared_term_count: int = len(prepared)

expression: hb.Expression = prepared.integrate()
detailed: hb.IntegrationResult = hb.integrate_detailed(integrand, [x], options)
result_expression: hb.Expression = detailed.expression
result_variables: list[hb.Expression] = detailed.integration_variables
result_term_count: int = len(detailed)
letters: list[hb.AlgebraicLetter] = detailed.algebraic_letters

try:
    hb.integrate(integrand, [x], options)
except hb.DuplicateVariableError as error:
    duplicate_name: str = error.variable
except hb.DivergentIntegralError as error:
    boundary: str = error.boundary
    power: int = error.power

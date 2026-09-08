"""Deterministic workload expansion for the independent exploratory matrix."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[1]
DEFAULT_MATRIX = REPOSITORY / "tests/fixtures/performance-matrix.json"


def digest_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def compact(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def dense_polynomial(variables: list[str], degree: int, offset: int = 0) -> str:
    """Every monomial of total degree <= degree with small deterministic coefficients."""
    def powers(count: int, remaining: int):
        if count == 0:
            yield ()
        else:
            for exponent in range(remaining + 1):
                for rest in powers(count - 1, remaining - exponent):
                    yield (exponent, *rest)
    terms = []
    for index, exponents in enumerate(powers(len(variables), degree)):
        factors = [str(1 + (index + offset) % 7)]
        factors.extend(f"{variable}^{exponent}" for variable, exponent in zip(variables, exponents) if exponent)
        terms.append("*".join(factors))
    return "+".join(terms)


def scaling_cases(options: dict) -> list[dict]:
    cases = []

    def add(name: str, request: dict, *, integration: bool = False, fields: list[str] | None = None) -> None:
        fields = fields or (["result[].coef"] if integration else ["result"])
        cases.append({
            "name": "scale." + name, "tier": options["tier"],
            "timeout_seconds": options["timeout_seconds"], "memory_mib": options["memory_mib"],
            "compare": "semantic", "semantic_fields": fields,
            "ignore": ["vars", "timing_compute_s", "algebraic_letters"] if integration else ["vars"],
            "reason": "Exact algebraic fields are canonicalized; all undeclared transport fields remain exact.",
            "request": request,
        })

    for degree in options["multiply_degrees"]:
        for count in options["multiply_variables"]:
            variables = [f"x{i}" for i in range(count)]
            add(f"multiply.d{degree}.v{count}", {
                "op": "mul", "vars": variables,
                "a": "+".join(f"{i+1}*{x}^{degree}+{x}" for i, x in enumerate(variables)) + "+1",
                "b": "+".join(f"{i+2}*{x}^{degree-1}-{x}" for i, x in enumerate(variables)) + "+3",
            })
    for degree in options["dense_degrees"]:
        for count in options["dense_variables"]:
            variables = [f"x{i}" for i in range(count)]
            polynomial = dense_polynomial(variables, degree)
            add(f"dense_multiply.d{degree}.v{count}", {"op": "mul", "vars": variables,
                "a": polynomial, "b": dense_polynomial(variables, degree - 1, 3)})
            add(f"dense_power.d{degree}.v{count}", {"op": "pow", "vars": variables,
                "a": polynomial, "n": 2})
    for exponent in options["rational_powers"]:
        add(f"parse_rational_power.n{exponent}", {"op": "parse_expr", "vars": ["x", "y", "z"],
            "expr": f"((x^2+y*z+1)/(x*y+z^2+2))^({exponent})"}, fields=["canonical"])
    for degree in options["derivative_denominator_powers"]:
        add(f"rational_derivative.den{degree}", {"op": "differentiate_wordlist",
            "vars": ["x", "y", "z"], "var": "x",
            "wl": [{"coef": f"(x^3+x*y+y^2+1)/((y+z)^{degree}*(1+z)^{degree})", "word": []}]},
            fields=["result[].coef"])
    for degree in options["gcd_degrees"]:
        common = f"((x+y+z)^{degree}+1)"
        add(f"gcd.d{degree}", {"op": "gcd", "vars": ["x", "y", "z"],
            "a": f"{common}*(x^{degree+1}+y)", "b": f"{common}*(x^{degree+2}+z)"})
    for degree in options["resultant_degrees"]:
        add(f"resultant.d{degree}", {"op": "resultant", "vars": ["x", "y", "z"],
            "var": "x", "a": f"x^{degree}+y*x+z", "b": f"x^{degree-1}+y*x+z+1"})
    for multiplicity in options["partial_fraction_multiplicities"]:
        for poles in [2, 3, 4]:
            names = ["a", "b", "c", "d"][:poles]
            denominator = "*".join(f"(x-{name})^{multiplicity}" for name in names)
            cases.append({"name": f"scale.partial_fractions.p{poles}.m{multiplicity}",
                "tier": options["tier"], "timeout_seconds": options["timeout_seconds"],
                "memory_mib": options["memory_mib"], "compare": "byte",
                "request": {"op": "partial_fractions", "vars": ["x", *names],
                    "var": "x", "f": f"(x+a)/({denominator})"}})
    for order in options["series_orders"]:
        add(f"laurent.order{order}", {"op": "series_expansion", "vars": ["x"], "var": "x",
            "max_order": order, "f": "(1+x+x^2)/(x^3*(1-x-x^2))"})
    for weight in options["period_weights"]:
        add(f"period.zero_one.w{weight}", {"op": "zero_one_period", "vars": ["x"],
            "word": ["0"] * (weight - 1) + ["1"]})
        add(f"period.zero_inf.w{weight}", {"op": "zero_inf_period", "vars": ["x"],
            "word": ["0"] * (weight - 1) + ["-2"]})
    for weight in options["integration_log_weights"]:
        numerator = f"Log[1+x]^{weight}" if weight else "1"
        add(f"integration.logw{weight}", {"op": "hyperflint", "vars": ["x"],
            "vars_int": ["x"], "expr": f"{numerator}/(1+x)^2", "parallel": False,
            "check_divergences": True}, integration=True)
    for count in options["integration_dimensions"]:
        variables = [f"x{i}" for i in range(count)]
        add(f"integration.v{count}", {"op": "hyperflint", "vars": variables,
            "vars_int": variables, "f": "1/(" + "*".join(f"(1+{x})^2" for x in variables) + ")",
            "parallel": False, "check_divergences": True}, integration=True)
    return cases


def inventory(matrix_path: Path, attachment_path: Path | None) -> tuple[list[dict], dict]:
    manifest = json.loads(matrix_path.read_text())
    if manifest.get("schema") != 1:
        raise ValueError("unsupported matrix schema")
    cases, sources = [], []
    for source in manifest["sources"]:
        path = REPOSITORY / source["path"]
        contents = path.read_bytes()
        sources.append({"path": source["path"], "sha256": digest_bytes(contents)})
        for line in contents.splitlines():
            if not line.strip():
                continue
            case = json.loads(line)
            case["name"] = source["prefix"] + "." + case["name"]
            for key in ["tier", "timeout_seconds", "memory_mib"]:
                case[key] = source[key]
            cases.append(case)
    cases.extend(scaling_cases(manifest["scaling"]))
    attachment = manifest["attachment"]
    source_cases = None
    if attachment_path is not None:
        contents = attachment_path.read_bytes()
        if digest_bytes(contents) != attachment["sha256"]:
            raise ValueError("attachment SHA-256 does not match the matrix manifest")
        source_cases = json.loads(contents)["cases"]
        if len(source_cases) != attachment["expected_cases"]:
            raise ValueError("attachment case count does not match the matrix manifest")
        sources.append({"path": str(attachment_path.resolve()), "sha256": attachment["sha256"]})
    entries = attachment["direct"] + attachment["pipeline"]
    if sorted(entry["index"] for entry in entries) != list(range(attachment["expected_cases"])):
        raise ValueError("attachment inventory must cover every index exactly once")
    for entry in entries:
        case = {**entry, "name": "attachment." + entry["id"], "attachment_index": entry["index"]}
        source = source_cases[entry["index"]] if source_cases is not None else None
        if source is not None:
            case["source_name"] = source["name"]
            case["source_case_sha256"] = digest_bytes(compact(source))
        if entry in attachment["pipeline"]:
            case.update(tier="pipeline", availability="requires_subtropica_preprocessing",
                reason="Projective gauge, analytic continuation/subtractions, and requested epsilon order are not supplied.")
            if entry.get("literal_zero"):
                case["reason"] = "Literal-zero graph input; frontend unused-variable/zero handling must be specified. It is not a backend benchmark."
        elif source is None:
            case.update(availability="attachment_missing", reason="Supply the exact attached hf_benchmarks.json with --attachment.")
        else:
            if not source.get("integration_order"):
                raise ValueError("direct attachment case has no integration order")
            expression = source["integrand"]
            request = {"op": "hyperflint", "vars": source["variables"] + source["parameters"],
                "vars_int": source["integration_order"], "parallel": False, "check_divergences": False,
                "expr" if "Log[" in expression or "Hlog[" in expression else "f": expression}
            if entry.get("algebraic_letters"):
                request["algebraic_letters"] = True
            case.update(request=request, compare="semantic", semantic_fields=["result[].coef"],
                ignore=["timing_compute_s", "vars", "algebraic_letters"],
                reason="Exact coefficient comparison, exact shuffle keys, and separate exact algebraic-definition validation.")
        cases.append(case)
    names = [case["name"] for case in cases]
    if len(names) != len(set(names)):
        raise ValueError("matrix case names must be unique")
    for case in cases:
        if "request" in case:
            case["request_sha256"] = digest_bytes(compact(case["request"]))
            for key in ["timeout_seconds", "memory_mib"]:
                if not isinstance(case[key], (int, float)) or case[key] <= 0:
                    raise ValueError(f"{case['name']}: {key} must be positive")
    return cases, {"matrix_sha256": digest_bytes(matrix_path.read_bytes()), "sources": sources}

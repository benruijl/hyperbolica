"""Installed-extension smoke test for coherent Symbolica type identity.

Run after ``maturin develop`` or against an installed wheel.  The test stops
at Hyperbolica's structural variable validator, so it proves that expressions
constructed by the shipped module cross the PyO3 boundary without requiring a
licensed mathematical evaluation.
"""

import hyperbolica as hb


def test_shipped_expression_reaches_prepare_validation() -> None:
    expression = hb.E("python_smoke_x+1")
    assert isinstance(expression, hb.Expression)

    try:
        hb.prepare(expression, [expression])
    except hb.InputError as error:
        assert "must be a plain Symbolica symbol" in str(error)
    else:
        raise AssertionError("prepare accepted a non-symbol integration variable")


if __name__ == "__main__":
    test_shipped_expression_reaches_prepare_validation()

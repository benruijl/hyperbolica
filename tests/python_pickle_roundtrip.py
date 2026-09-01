"""Fresh-process pickle checks for the installed standalone extension.

The writer and both readers are invoked separately by
``test-python-installed.sh``. One reader verifies that unpickling imports the
package-local reconstructor. The other deliberately changes symbol-registration
order before unpickling, which rejects process-local raw Atom serialization.
"""

from __future__ import annotations

from pathlib import Path
import pickle
import sys


def write_payload(path: Path) -> None:
    import hyperbolica as hb

    expression = (hb.S("python_fresh_pickle_x") + 1) ** 3
    path.write_bytes(pickle.dumps(expression))


def assert_payload(expression: object) -> None:
    import hyperbolica as hb

    assert type(expression) is hb.Expression
    assert expression.to_canonical_string() == (
        (hb.S("python_fresh_pickle_x") + 1) ** 3
    ).to_canonical_string()
    assert "symbolica" not in sys.modules
    assert "symbolica.symbolica" not in sys.modules


def read_payload(path: Path) -> None:
    assert "hyperbolica" not in sys.modules
    assert "hyperbolica.hyperbolica" not in sys.modules
    assert_payload(pickle.loads(path.read_bytes()))


def read_perturbed_payload(path: Path) -> None:
    assert "hyperbolica" not in sys.modules
    assert "hyperbolica.hyperbolica" not in sys.modules

    import hyperbolica as hb

    for index in range(32):
        hb.S(f"python_pickle_reader_perturbation_{index}")
    assert_payload(pickle.loads(path.read_bytes()))


if __name__ == "__main__":
    actions = {"write", "read", "read-perturbed"}
    if len(sys.argv) != 3 or sys.argv[1] not in actions:
        raise SystemExit(
            f"usage: {sys.argv[0]} write|read|read-perturbed PICKLE_PATH"
        )
    action, payload = sys.argv[1], Path(sys.argv[2])
    if action == "write":
        write_payload(payload)
    elif action == "read":
        read_payload(payload)
    else:
        read_perturbed_payload(payload)

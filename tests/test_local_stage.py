from __future__ import annotations

import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[1]
STAGE_SCRIPT = REPOSITORY / "scripts" / "stage-local.sh"


class LocalStageTests(unittest.TestCase):
    def test_refuses_occupied_output_before_building(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fake_cargo = root / "fake-cargo"
            cargo_marker = root / "cargo-was-invoked"
            fake_cargo.write_text(
                "#!/bin/sh\n: > \"$FAKE_CARGO_MARKER\"\nexit 99\n",
                encoding="utf-8",
            )
            fake_cargo.chmod(0o755)
            environment = dict(os.environ)
            environment.update(
                {
                    "CARGO": str(fake_cargo),
                    "FAKE_CARGO_MARKER": str(cargo_marker),
                }
            )

            occupied_file = root / "occupied-file"
            occupied_file.write_text("unrelated\n", encoding="utf-8")
            nonempty_directory = root / "nonempty-directory"
            nonempty_directory.mkdir()
            unrelated = nonempty_directory / "unrelated.txt"
            unrelated.write_text("keep me\n", encoding="utf-8")

            for output in (occupied_file, nonempty_directory):
                with self.subTest(output=output.name):
                    completed = subprocess.run(
                        ["bash", str(STAGE_SCRIPT), str(output)],
                        cwd=REPOSITORY,
                        env=environment,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        check=False,
                        timeout=10,
                    )
                    self.assertEqual(completed.returncode, 2)

            self.assertFalse(cargo_marker.exists())
            self.assertEqual(occupied_file.read_text(encoding="utf-8"), "unrelated\n")
            self.assertEqual(unrelated.read_text(encoding="utf-8"), "keep me\n")

    def test_stages_all_surfaces_notices_and_default_compatibility(self) -> None:
        system = platform.system()
        if system == "Darwin":
            backend = "libhyperbolica.dylib"
            adapter = "libhyperflint_librarylink.dylib"
        elif system == "Linux":
            backend = "libhyperbolica.so"
            adapter = "libhyperflint_librarylink.so"
        else:
            self.skipTest("local staging supports macOS and Linux")

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fake_cargo = root / "fake-cargo"
            cargo_log = root / "cargo.log"
            fake_cargo.write_text(
                """#!/bin/sh
set -eu
[ -z "${SYMBOLICA_LICENSE+x}" ] || exit 90
[ -z "${SYMBOLICA_LICENSE_SERVER+x}" ] || exit 93
if [ -n "${STAGE_RACE_OUTPUT-}" ] && [ ! -e "${STAGE_RACE_MARKER-}" ]; then
    mkdir -p "$STAGE_RACE_OUTPUT"
    printf 'unrelated during build\n' > "$STAGE_RACE_OUTPUT/hyperflint"
    : > "$STAGE_RACE_MARKER"
fi
printf '%s|%s\\n' "${HYPERBOLICA_SUBTROPICA_VERSION-}" "$*" >> "$FAKE_CARGO_LOG"
target=
manifest=
previous=
for argument in "$@"; do
    if [ "$previous" = target ]; then target=$argument; fi
    if [ "$previous" = manifest ]; then manifest=$argument; fi
    case "$argument" in
        --target-dir) previous=target ;;
        --manifest-path) previous=manifest ;;
        *) previous= ;;
    esac
done
[ -n "$target" ] || exit 91
mkdir -p "$target/release"
case "$manifest" in
    */librarylink/Cargo.toml)
        : > "$target/release/$FAKE_ADAPTER_NAME"
        ;;
    */Cargo.toml)
        : > "$target/release/$FAKE_BACKEND_NAME"
        printf '%s\\n' '#!/bin/sh' \
            'printf "HF_VERSION: 0.1.0.0\\nHF_BUILD_VARIANT: rust-symbolica\\n"' \
            > "$target/release/hyperflint"
        chmod +x "$target/release/hyperflint"
        ;;
    *) exit 92 ;;
esac
""",
                encoding="utf-8",
            )
            fake_cargo.chmod(0o755)

            root_target = root / "root-target"
            librarylink_target = root / "librarylink-target"
            stage = root / "stage"
            environment = dict(os.environ)
            environment.update(
                {
                    "CARGO": str(fake_cargo),
                    "FAKE_CARGO_LOG": str(cargo_log),
                    "FAKE_BACKEND_NAME": backend,
                    "FAKE_ADAPTER_NAME": adapter,
                    "HYPERBOLICA_TARGET_DIR": str(root_target),
                    "HYPERBOLICA_LIBRARYLINK_TARGET_DIR": str(librarylink_target),
                    "SYMBOLICA_LICENSE": "synthetic-stage-secret",
                    "SYMBOLICA_LICENSE_SERVER": "synthetic-stage-server-secret",
                }
            )
            environment.pop("HYPERBOLICA_SUBTROPICA_VERSION", None)

            race_stage = root / "race-stage"
            race_marker = root / "race-triggered"
            race_environment = dict(environment)
            race_environment.update(
                {
                    "STAGE_RACE_OUTPUT": str(race_stage),
                    "STAGE_RACE_MARKER": str(race_marker),
                }
            )
            raced = subprocess.run(
                ["bash", str(STAGE_SCRIPT), str(race_stage)],
                cwd=REPOSITORY,
                env=race_environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
                timeout=10,
            )
            self.assertEqual(raced.returncode, 2, raced.stderr.decode())
            self.assertEqual(
                (race_stage / "hyperflint").read_text(encoding="utf-8"),
                "unrelated during build\n",
            )
            self.assertFalse((race_stage / "stage-metadata.json").exists())

            completed = subprocess.run(
                ["bash", str(STAGE_SCRIPT), str(stage)],
                cwd=REPOSITORY,
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
                timeout=10,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr.decode())
            self.assertNotIn(b"synthetic-stage-secret", completed.stdout)
            self.assertNotIn(b"synthetic-stage-secret", completed.stderr)
            self.assertNotIn(b"synthetic-stage-server-secret", completed.stdout)
            self.assertNotIn(b"synthetic-stage-server-secret", completed.stderr)

            expected = {
                "hyperflint",
                backend,
                adapter,
                "include/hyperbolica/c_abi.h",
                "include/hyperbolica_librarylink.h",
                "licenses/LICENSE",
                "licenses/DISTRIBUTION-LICENSE.md",
                "licenses/SYMBOLICA-License.md",
                "stage-metadata.json",
            }
            actual = {
                str(path.relative_to(stage))
                for path in stage.rglob("*")
                if path.is_file()
            }
            self.assertEqual(actual, expected)
            self.assertEqual(
                (stage / "licenses" / "SYMBOLICA-License.md").read_bytes(),
                (REPOSITORY / "vendor" / "symbolica" / "License.md").read_bytes(),
            )

            metadata = json.loads((stage / "stage-metadata.json").read_bytes())
            self.assertEqual(metadata["artifact_kind"], "local-only-development-stage")
            self.assertEqual(metadata["hyperbolica_hf_version"], "0.1.0.0")
            self.assertEqual(metadata["subtropica_compatibility_version"], "1.2.13")
            self.assertIs(metadata["redistributable"], False)

            invocations = cargo_log.read_text(encoding="utf-8").splitlines()
            self.assertEqual(len(invocations), 4)
            self.assertTrue(all("--locked --release" in line for line in invocations))
            for offset in (0, 2):
                self.assertTrue(invocations[offset].startswith("|"))
                self.assertTrue(invocations[offset + 1].startswith("1.2.13|"))


if __name__ == "__main__":
    unittest.main()

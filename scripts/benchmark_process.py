#!/usr/bin/env python3
"""Measure one subprocess without depending on either computer algebra system.

The child receives the selected request on standard input. Its standard output
and standard error are written to explicit files, leaving this program's
standard output available for one compact JSON measurement record.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import resource
import signal
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import BinaryIO, Sequence


class ProcessMeasurementError(ValueError):
    """An invalid measurement request that should be reported to the caller."""


ENVIRONMENT_NAME_PATTERN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")


def _environment_name(value: str, option: str) -> str:
    if not ENVIRONMENT_NAME_PATTERN.fullmatch(value):
        raise ProcessMeasurementError(
            f"invalid {option} name {value!r}; expected a shell environment name"
        )
    return value


def _positive_float(value: str) -> float:
    parsed = float(value)
    if not math.isfinite(parsed) or parsed <= 0:
        raise argparse.ArgumentTypeError("must be finite and greater than zero")
    return parsed


def _nonnegative_float(value: str) -> float:
    parsed = float(value)
    if not math.isfinite(parsed) or parsed < 0:
        raise argparse.ArgumentTypeError("must be finite and non-negative")
    return parsed


def _environment(
    entries: Sequence[str],
    removals: Sequence[str],
    inherited_names: Sequence[str],
    *,
    clear: bool,
) -> dict[str, str]:
    environment = {} if clear else dict(os.environ)
    for name_value in inherited_names:
        name = _environment_name(name_value, "--inherit-env-var")
        if name not in os.environ:
            raise ProcessMeasurementError(
                f"parent environment variable {name!r} requested by "
                "--inherit-env-var is not set"
            )
        environment[name] = os.environ[name]
    for name in removals:
        name = _environment_name(name, "--unset-env")
        environment.pop(name, None)
    for entry in entries:
        name, separator, value = entry.partition("=")
        if not separator:
            raise ProcessMeasurementError(
                f"invalid --env value {entry!r}; expected NAME=VALUE"
            )
        name = _environment_name(name, "--env")
        environment[name] = value
    return environment


def _request_bytes(arguments: argparse.Namespace) -> bytes:
    if arguments.request is not None:
        request = arguments.request.encode("utf-8")
    elif arguments.request_file is not None:
        request = arguments.request_file.read_bytes()
    else:
        request = sys.stdin.buffer.read()
    if not arguments.exact_request and not request.endswith(b"\n"):
        request += b"\n"
    return request


def _max_rss_bytes(usage: resource.struct_rusage) -> int:
    # Darwin reports bytes. Linux and the BSDs report KiB for ru_maxrss.
    multiplier = 1 if sys.platform == "darwin" else 1024
    return int(usage.ru_maxrss) * multiplier


def _kill_group(process_group: int, selected_signal: signal.Signals) -> None:
    try:
        os.killpg(process_group, selected_signal)
    except ProcessLookupError:
        return


def measure_process(
    command: Sequence[str],
    request: bytes,
    stdout_handle: BinaryIO,
    stderr_handle: BinaryIO,
    *,
    timeout_seconds: float,
    terminate_grace_seconds: float,
    cwd: Path | None,
    environment: dict[str, str],
) -> dict[str, int | bool]:
    """Run one process and return monotonic wall time and wait4 resource use."""

    if not command:
        raise ProcessMeasurementError("a command is required after '--'")
    if not hasattr(os, "wait4"):
        raise ProcessMeasurementError("POSIX os.wait4 is required")

    with tempfile.TemporaryFile(mode="w+b") as request_handle:
        request_handle.write(request)
        request_handle.seek(0)
        started_ns = time.perf_counter_ns()
        try:
            process = subprocess.Popen(  # noqa: S603 - caller explicitly selects command.
                list(command),
                stdin=request_handle,
                stdout=stdout_handle,
                stderr=stderr_handle,
                cwd=cwd,
                env=environment,
                start_new_session=True,
            )
        except OSError as error:
            raise ProcessMeasurementError(
                f"could not start {command[0]!r}: {error}"
            ) from error

        timed_out = threading.Event()
        finished = threading.Event()
        force_kill_timer: threading.Timer | None = None

        def force_kill() -> None:
            if not finished.is_set():
                _kill_group(process.pid, signal.SIGKILL)

        def terminate() -> None:
            nonlocal force_kill_timer
            if finished.is_set():
                return
            timed_out.set()
            _kill_group(process.pid, signal.SIGTERM)
            if terminate_grace_seconds == 0:
                force_kill()
            else:
                force_kill_timer = threading.Timer(terminate_grace_seconds, force_kill)
                force_kill_timer.daemon = True
                force_kill_timer.start()

        timeout_timer = threading.Timer(timeout_seconds, terminate)
        timeout_timer.daemon = True
        timeout_timer.start()
        try:
            _, wait_status, usage = os.wait4(process.pid, 0)
        except BaseException:
            _kill_group(process.pid, signal.SIGKILL)
            _, wait_status, usage = os.wait4(process.pid, 0)
            process.returncode = os.waitstatus_to_exitcode(wait_status)
            raise
        finally:
            finished.set()
            timeout_timer.cancel()
            if force_kill_timer is not None:
                force_kill_timer.cancel()

        elapsed_ns = time.perf_counter_ns() - started_ns
        exit_code = os.waitstatus_to_exitcode(wait_status)
        # Popen must know that the child has already been reaped directly by wait4.
        process.returncode = exit_code
        return {
            "elapsed_ns": elapsed_ns,
            "user_ns": round(usage.ru_utime * 1_000_000_000),
            "sys_ns": round(usage.ru_stime * 1_000_000_000),
            "max_rss_bytes": _max_rss_bytes(usage),
            "exit_code": exit_code,
            "timed_out": timed_out.is_set(),
        }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Run one backend invocation with a monotonic clock and POSIX wait4; "
            "write a compact JSON measurement to stdout."
        ),
        epilog=(
            "REQUEST defaults to this program's stdin. The child always receives "
            "the request through stdin. By default child failure is represented in "
            "JSON without changing this program's exit status; use --propagate-exit "
            "for conventional shell status propagation."
        ),
    )
    request_group = parser.add_mutually_exclusive_group()
    request_group.add_argument("--request", help="literal UTF-8 request")
    request_group.add_argument(
        "--request-file", type=Path, help="file containing the request"
    )
    parser.add_argument(
        "--exact-request",
        action="store_true",
        help="do not append a trailing newline when one is absent",
    )
    parser.add_argument("--stdout", required=True, type=Path, help="child stdout path")
    parser.add_argument("--stderr", required=True, type=Path, help="child stderr path")
    parser.add_argument(
        "--timeout-seconds", required=True, type=_positive_float, help="hard timeout"
    )
    parser.add_argument(
        "--terminate-grace-seconds",
        type=_nonnegative_float,
        default=0.25,
        help="delay between SIGTERM and SIGKILL (default: 0.25)",
    )
    parser.add_argument("--cwd", type=Path, help="child working directory")
    parser.add_argument(
        "--env", action="append", default=[], metavar="NAME=VALUE", help="set child env"
    )
    parser.add_argument(
        "--unset-env",
        action="append",
        default=[],
        metavar="NAME",
        help="remove child env",
    )
    parser.add_argument(
        "--inherit-env-var",
        action="append",
        default=[],
        metavar="NAME",
        help="copy one named parent variable into the child environment",
    )
    environment_group = parser.add_mutually_exclusive_group()
    environment_group.add_argument(
        "--clear-env",
        action="store_true",
        help=(
            "start from an empty child environment before applying --env and "
            "--inherit-env-var"
        ),
    )
    environment_group.add_argument(
        "--inherit-env",
        dest="clear_env",
        action="store_false",
        help="inherit the parent environment (default)",
    )
    parser.set_defaults(clear_env=False)
    parser.add_argument(
        "--propagate-exit",
        action="store_true",
        help="return 124 on timeout or the child's nonzero exit status",
    )
    parser.add_argument(
        "command", nargs=argparse.REMAINDER, help="command and arguments, after '--'"
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    parser = _parser()
    arguments = parser.parse_args(argv)
    command = list(arguments.command)
    if command and command[0] == "--":
        command.pop(0)
    try:
        if arguments.stdout.resolve() == arguments.stderr.resolve():
            raise ProcessMeasurementError("--stdout and --stderr must be distinct")
        request = _request_bytes(arguments)
        environment = _environment(
            arguments.env,
            arguments.unset_env,
            arguments.inherit_env_var,
            clear=arguments.clear_env,
        )
        arguments.stdout.parent.mkdir(parents=True, exist_ok=True)
        arguments.stderr.parent.mkdir(parents=True, exist_ok=True)
        with (
            arguments.stdout.open("wb") as stdout_handle,
            arguments.stderr.open("wb") as stderr_handle,
        ):
            result = measure_process(
                command,
                request,
                stdout_handle,
                stderr_handle,
                timeout_seconds=arguments.timeout_seconds,
                terminate_grace_seconds=arguments.terminate_grace_seconds,
                cwd=arguments.cwd,
                environment=environment,
            )
    except ProcessMeasurementError as error:
        parser.error(str(error))

    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    if not arguments.propagate_exit:
        return 0
    if result["timed_out"]:
        return 124
    exit_code = int(result["exit_code"])
    return exit_code if 0 <= exit_code <= 125 else 1


if __name__ == "__main__":
    raise SystemExit(main())

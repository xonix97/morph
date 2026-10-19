from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SDK = ROOT / "sdk" / "python"


RUNNER = """
import os
import time
from morph_runtime import Runtime

runtime = Runtime(
    ["logic"],
    control_file=os.environ["MORPH_TEST_CONTROL"],
    status_file=os.environ["MORPH_TEST_STATUS"],
)
try:
    while runtime.running:
        runtime.state["ticks"] += 1
        print(runtime.call("logic", "render", runtime.state), flush=True)
        time.sleep(0.04)
finally:
    runtime.close()
"""


def write_atomic(path: Path, value: str) -> None:
    temporary = path.with_suffix(".tmp")
    temporary.write_text(value, encoding="utf-8")
    temporary.replace(path)


def wait_for(path: Path, text: str, timeout: float = 4.0) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists():
            value = path.read_text(encoding="utf-8")
            if text in value:
                return value
        time.sleep(0.03)
    raise AssertionError(f"timed out waiting for {text!r} in {path}")


class PythonRuntimeTests(unittest.TestCase):
    def start_worker(self, directory: Path) -> tuple[subprocess.Popen[str], Path, Path]:
        control = directory / "control.request"
        status = directory / "status.json"
        write_atomic(control, "READY")
        runner = directory / "runner.py"
        runner.write_text(RUNNER, encoding="utf-8")
        environment = os.environ.copy()
        environment["PYTHONPATH"] = os.pathsep.join([str(SDK), str(directory)])
        environment["MORPH_TEST_CONTROL"] = str(control)
        environment["MORPH_TEST_STATUS"] = str(status)
        environment["MORPH_SESSION"] = "python-test-session"
        process = subprocess.Popen(
            [sys.executable, str(runner)],
            cwd=directory,
            env=environment,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        wait_for(status, '"status": "ok"')
        return process, control, status

    def stop_worker(self, process: subprocess.Popen[str], control: Path) -> tuple[str, str]:
        if process.poll() is None:
            write_atomic(control, "STOP")
        stdout, stderr = process.communicate(timeout=4)
        return stdout, stderr

    def test_successful_swap_preserves_runtime_state_and_pid(self) -> None:
        with tempfile.TemporaryDirectory() as raw_directory:
            directory = Path(raw_directory)
            logic = directory / "logic.py"
            logic.write_text(
                "MORPH_RELOAD_VERSION = 1\n"
                "def render(state):\n"
                "    return f'v1 ticks={state[\"ticks\"]}'\n",
                encoding="utf-8",
            )
            process, control, status = self.start_worker(directory)
            try:
                time.sleep(0.2)
                logic.write_text(
                    "MORPH_RELOAD_VERSION = 2\n"
                    "def render(state):\n"
                    "    return f'v2 ticks={state[\"ticks\"]}'\n",
                    encoding="utf-8",
                )
                write_atomic(control, "RELOAD 1")
                wait_for(status, '"generation": 1')
                time.sleep(0.15)
                stdout, stderr = self.stop_worker(process, control)
            except Exception:
                self.stop_worker(process, control)
                raise

            self.assertEqual(stderr, "")
            self.assertIn("v1 ticks=", stdout)
            self.assertIn("v2 ticks=", stdout)
            self.assertGreater(stdout.index("v2 ticks="), stdout.index("v1 ticks="))
            self.assertEqual(process.returncode, 0)

    def test_invalid_candidate_keeps_previous_module_active(self) -> None:
        with tempfile.TemporaryDirectory() as raw_directory:
            directory = Path(raw_directory)
            logic = directory / "logic.py"
            logic.write_text(
                "MORPH_RELOAD_VERSION = 1\n"
                "def render(state):\n"
                "    return 'stable'\n",
                encoding="utf-8",
            )
            process, control, status = self.start_worker(directory)
            try:
                logic.write_text("def this_is_not_valid(:\n", encoding="utf-8")
                write_atomic(control, "RELOAD 1")
                wait_for(status, '"status": "error"')
                time.sleep(0.15)
                stdout, _ = self.stop_worker(process, control)
            except Exception:
                self.stop_worker(process, control)
                raise

            self.assertIn("stable", stdout)
            self.assertNotIn("PYTHON_RELOAD", stdout)
            self.assertEqual(process.returncode, 0)

    def test_invalid_render_signature_is_rejected_before_activation(self) -> None:
        with tempfile.TemporaryDirectory() as raw_directory:
            directory = Path(raw_directory)
            logic = directory / "logic.py"
            logic.write_text(
                "MORPH_RELOAD_VERSION = 1\n"
                "def render(state):\n"
                "    return 'stable'\n",
                encoding="utf-8",
            )
            process, control, status = self.start_worker(directory)
            try:
                logic.write_text(
                    "MORPH_RELOAD_VERSION = 2\n"
                    "def render():\n"
                    "    return 'bad'\n",
                    encoding="utf-8",
                )
                write_atomic(control, "RELOAD 1")
                wait_for(status, '"status": "error"')
                time.sleep(0.15)
                stdout, _ = self.stop_worker(process, control)
            except Exception:
                self.stop_worker(process, control)
                raise

            self.assertIn("stable", stdout)
            self.assertNotIn("bad", stdout)
            self.assertEqual(process.returncode, 0)

    def test_old_import_reference_stays_old_while_runtime_slot_changes(self) -> None:
        sys.path.insert(0, str(SDK))
        try:
            from morph_runtime import Runtime

            with tempfile.TemporaryDirectory() as raw_directory:
                directory = Path(raw_directory)
                module_name = "logic_reference_test"
                logic = directory / f"{module_name}.py"
                logic.write_text(
                    "MORPH_RELOAD_VERSION = 1\n"
                    "def render(state):\n"
                    "    return 'v1'\n",
                    encoding="utf-8",
                )
                sys.path.insert(0, str(directory))
                control = directory / "control.request"
                status = directory / "status.json"
                write_atomic(control, "READY")
                runtime = Runtime([module_name], control_file=control, status_file=status)
                try:
                    old_reference = runtime.slots[module_name].module.render
                    logic.write_text(
                        "MORPH_RELOAD_VERSION = 2\n"
                        "def render(state):\n"
                        "    return 'v2'\n",
                        encoding="utf-8",
                    )
                    write_atomic(control, "RELOAD 1")
                    wait_for(status, '"generation": 1')
                    self.assertEqual(old_reference({}), "v1")
                    self.assertEqual(runtime.call(module_name, "render", {}), "v2")
                finally:
                    write_atomic(control, "STOP")
                    runtime.close()
                    sys.modules.pop(module_name, None)
                    sys.path.remove(str(directory))
        finally:
            sys.path.remove(str(SDK))


if __name__ == "__main__":
    unittest.main()

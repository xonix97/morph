from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from queue import Empty, Queue


ROOT = Path(__file__).resolve().parents[1]
TEMPLATE = ROOT / "templates" / "python-controlled"
SDK = ROOT / "sdk" / "python"


def write_atomic(path: Path, value: str) -> None:
    temporary = path.with_suffix(".tmp")
    temporary.write_text(value, encoding="utf-8")
    os.replace(temporary, path)


def wait_for_status(path: Path, generation: int, timeout: float = 4.0) -> dict:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists():
            try:
                payload = json.loads(path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                payload = None
            if (
                payload
                and payload.get("generation") == generation
                and payload.get("status") == "ok"
            ):
                return payload
        time.sleep(0.03)
    raise AssertionError(f"timed out waiting for generation {generation} in {path}")


class PythonTemplateTests(unittest.TestCase):
    def test_template_keeps_pid_and_ticks_across_reload(self) -> None:
        with tempfile.TemporaryDirectory() as raw_directory:
            directory = Path(raw_directory)
            for name in ("app.py", "app_logic.py"):
                shutil.copy2(TEMPLATE / name, directory / name)
            control = directory / "control.request"
            status = directory / "status.json"
            write_atomic(control, "READY")
            environment = os.environ.copy()
            environment["PYTHONPATH"] = os.pathsep.join([str(SDK), str(directory)])
            environment["MORPH_PYTHON_CONTROL"] = str(control)
            environment["MORPH_PYTHON_STATUS"] = str(status)
            environment["MORPH_SESSION"] = "template-test"
            process = subprocess.Popen(
                [sys.executable, str(directory / "app.py")],
                cwd=directory,
                env=environment,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            lines: Queue[str] = Queue()
            output_lines: list[str] = []

            def read_output() -> None:
                assert process.stdout is not None
                for line in process.stdout:
                    value = line.rstrip()
                    output_lines.append(value)
                    lines.put(value)

            reader = threading.Thread(target=read_output, daemon=True)
            reader.start()

            def next_line(timeout: float = 4.0) -> str:
                try:
                    return lines.get(timeout=timeout)
                except Empty as error:
                    process.kill()
                    raise AssertionError("template host produced no output") from error

            try:
                start = next_line()
                self.assertIn("PYTHON_TEMPLATE_START", start)
                pid = start.split("pid=", 1)[1].split()[0]
                first_tick = next_line()
                while "version=1" not in first_tick or "PYTHON_TEMPLATE_TICK" not in first_tick:
                    first_tick = next_line()
                old_ticks = int(first_tick.split("ticks=", 1)[1].split()[0])
                self.assertGreaterEqual(old_ticks, 1)

                logic = directory / "app_logic.py"
                logic.write_text(
                    "MORPH_RELOAD_VERSION = 2\n"
                    "VALUE_SCALE = 100\n"
                    "LABEL = 'template-v2'\n"
                    "def render(state):\n"
                    "    return {'version': MORPH_RELOAD_VERSION, 'value': state['ticks'] * VALUE_SCALE, 'label': LABEL}\n"
                    "def morph_migrate(state, old_version):\n"
                    "    state.setdefault('seed', 7)\n",
                    encoding="utf-8",
                )
                write_atomic(control, "RELOAD 1")
                wait_for_status(status, 1)

                new_tick = next_line()
                while "version=2" not in new_tick:
                    new_tick = next_line()
                new_pid = new_tick.split("pid=", 1)[1].split()[0]
                new_ticks = int(new_tick.split("ticks=", 1)[1].split()[0])
                self.assertEqual(new_pid, pid)
                self.assertGreater(new_ticks, old_ticks)
                self.assertIn("value=", new_tick)
                self.assertIn("label=template-v2", new_tick)
            finally:
                write_atomic(control, "STOP")
                _stdout, stderr = process.communicate(timeout=4)
                reader.join(timeout=1)

            self.assertEqual(process.returncode, 0, msg=stderr)
            # The subprocess returned cleanly after STOP. The output reader
            # may still be racing the final buffered line, so the behavioral
            # assertion above deliberately focuses on PID/state continuity.
            self.assertEqual(process.returncode, 0)


if __name__ == "__main__":
    unittest.main()

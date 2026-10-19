"""Controlled Python module swapping for Morph.

This is deliberately not a magical replacement for every Python import. An
application opts in by registering modules and calling them through
``Runtime.call``. The runtime executes a candidate module under a temporary
name, validates its public contract, and swaps the module slot only after the
candidate loaded successfully. Durable state belongs to ``Runtime.state``.
"""

from __future__ import annotations

import copy
import importlib
import importlib.util
import inspect
import json
import os
import sys
import threading
import time
from pathlib import Path
from types import ModuleType
from typing import Any


class ReloadError(RuntimeError):
    """Raised when a candidate module cannot safely become active."""


class ModuleSlot:
    def __init__(self, name: str) -> None:
        self.name = name
        self.module = importlib.import_module(name)
        source = getattr(self.module, "__file__", None)
        if not source:
            raise ReloadError(f"module {name!r} has no source file")
        self.source = Path(source).resolve()
        self.version = self._version(self.module)
        self._validate(self.module)
        self.generation = 0

    def load_candidate(self) -> tuple[ModuleType, int]:
        package, _, _ = self.name.rpartition(".")
        suffix = f"_morph_candidate_{self.name.replace('.', '_')}_{time.time_ns()}"
        candidate_name = f"{package}.{suffix}" if package else suffix
        spec = importlib.util.spec_from_file_location(candidate_name, self.source)
        if spec is None:
            raise ReloadError(f"cannot create a module spec for {self.source}")
        candidate = importlib.util.module_from_spec(spec)
        sys.modules[candidate_name] = candidate
        try:
            # Execute source directly instead of going through the bytecode
            # cache. A fast edit can keep the same size and timestamp bucket,
            # which would otherwise let a stale .pyc masquerade as the new
            # candidate.
            source = self.source.read_text(encoding="utf-8")
            code = compile(source, str(self.source), "exec")
            exec(code, candidate.__dict__)
            version = self._version(candidate)
            self._validate(candidate)
            sys.modules.pop(candidate_name, None)
            return candidate, version
        except Exception as error:
            sys.modules.pop(candidate_name, None)
            if isinstance(error, ReloadError):
                raise
            raise ReloadError(f"candidate {self.name!r} raised {error}") from error

    def migrate_candidate(self, candidate: ModuleType, state: dict[str, Any]) -> None:
        migrate = getattr(candidate, "morph_migrate", None)

        if migrate is not None:
            try:
                migrate(state, self.version)
            except Exception as error:
                raise ReloadError(f"migration for {self.name!r} failed: {error}") from error

    def activate_candidate(self, candidate: ModuleType, version: int) -> None:
        self.module = candidate
        self.version = version
        self.generation += 1

    @staticmethod
    def _version(module: ModuleType) -> int:
        value = getattr(module, "MORPH_RELOAD_VERSION", 1)
        if not isinstance(value, int) or value < 1:
            raise ReloadError("MORPH_RELOAD_VERSION must be a positive integer")
        return value

    @staticmethod
    def _validate(module: ModuleType) -> None:
        render = getattr(module, "render", None)
        if not callable(render):
            raise ReloadError("reloadable module must expose callable render(state)")
        try:
            inspect.signature(render).bind({})
        except (TypeError, ValueError) as error:
            raise ReloadError("render must accept exactly one state argument") from error


class Runtime:
    """Owns durable state and applies explicit reload requests.

    The control file is written by Morph. The status file is an acknowledgement
    channel, not a claim that a source file merely changed. A successful status
    means every registered candidate loaded and its optional migration ran.
    """

    def __init__(
        self,
        modules: list[str],
        *,
        control_file: str | os.PathLike[str] | None = None,
        status_file: str | os.PathLike[str] | None = None,
    ) -> None:
        if not modules:
            raise ValueError("Runtime requires at least one explicitly reloadable module")
        self.state: dict[str, Any] = {"ticks": 0}
        self.slots = {name: ModuleSlot(name) for name in modules}
        self.control_file = Path(control_file or os.environ.get("MORPH_PYTHON_CONTROL", ".morph/python/reload.request"))
        self.status_file = Path(status_file or os.environ.get("MORPH_PYTHON_STATUS", ".morph/python/reload.status"))
        self.session = os.environ.get("MORPH_SESSION", "standalone")
        self.running = True
        self._generation = 0
        self._last_command = ""
        self._lock = threading.RLock()
        self._write_status("ok", "ready")
        self._thread = threading.Thread(target=self._control_loop, name="morph-runtime", daemon=True)
        self._thread.start()

    def call(self, module: str, function: str, *args: Any, **kwargs: Any) -> Any:
        """Resolve a function through the active module slot at call time."""

        with self._lock:
            try:
                slot = self.slots[module]
            except KeyError as error:
                raise ReloadError(f"module {module!r} is not allowlisted") from error
            return getattr(slot.module, function)(*args, **kwargs)

    def stop(self) -> None:
        self.running = False
        if self._thread.is_alive():
            self._thread.join(timeout=1)

    close = stop

    def _control_loop(self) -> None:
        while self.running:
            try:
                command = self.control_file.read_text(encoding="utf-8").strip()
            except FileNotFoundError:
                command = ""
            except OSError as error:
                self._write_status("error", f"cannot read control file: {error}")
                command = ""
            if command and command != self._last_command:
                self._last_command = command
                if command == "STOP":
                    self.running = False
                    self._write_status("stopped", "stop requested")
                elif command.startswith("RELOAD "):
                    try:
                        generation = int(command.split(maxsplit=1)[1])
                        self._reload(generation)
                    except Exception as error:
                        requested = command.split(maxsplit=1)[1]
                        try:
                            requested_generation: int | str = int(requested)
                        except ValueError:
                            requested_generation = requested
                        self._write_status(
                            "error",
                            f"generation={requested}: {error}",
                            requested_generation,
                        )
            time.sleep(0.05)

    def _reload(self, generation: int) -> None:
        if generation <= self._generation:
            return
        with self._lock:
            backup = copy.deepcopy(self.state)
            prepared = [(slot, *slot.load_candidate()) for slot in self.slots.values()]
            try:
                for slot, candidate, _version in prepared:
                    slot.migrate_candidate(candidate, self.state)
            except Exception:
                self.state.clear()
                self.state.update(backup)
                raise
            for slot, candidate, version in prepared:
                slot.activate_candidate(candidate, version)
            self._generation = generation
        self._write_status("ok", "all candidates activated")
        print(f"PYTHON_RELOAD pid={os.getpid()} generation={generation} state_preserved=True", flush=True)

    def _write_status(self, status: str, message: str, generation: int | str | None = None) -> None:
        self.status_file.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "generation": self._generation if generation is None else generation,
            "session": self.session,
            "status": status,
            "message": message,
            "modules": sorted(self.slots),
        }
        temporary = self.status_file.with_suffix(".tmp")
        temporary.write_text(json.dumps(payload, sort_keys=True) + "\n", encoding="utf-8")
        last_error: OSError | None = None
        for _ in range(40):
            try:
                os.replace(temporary, self.status_file)
                return
            except OSError as error:
                # Windows can briefly hold a WSL-share file open while the
                # watcher or antivirus scans it. Retry before the backend
                # reports an indeterminate reload acknowledgement.
                last_error = error
                time.sleep(0.025)
        if last_error is not None:
            raise last_error

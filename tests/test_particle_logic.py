from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "demos" / "particles" / "particle_logic.py"


def load_particle_logic():
    spec = importlib.util.spec_from_file_location("particle_logic_test", MODULE_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load {MODULE_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class ParticleLogicTests(unittest.TestCase):
    def test_update_advances_host_owned_state_without_graphics_imports(self) -> None:
        logic = load_particle_logic()
        particle = [1.0, 0.0, 0.0, 0.0, 0.4, 0.0, 0.25]
        before = particle.copy()
        state = {"frame": 10, "elapsed": 2.0, "particles": [particle]}

        logic.update(state, 0.016)

        self.assertEqual(state["frame"], 11)
        self.assertGreater(state["elapsed"], 2.0)
        self.assertNotEqual(state["particles"][0], before)

    def test_migration_adds_defaults_without_replacing_particles(self) -> None:
        logic = load_particle_logic()
        particles = [[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]]
        state = {"particles": particles}

        logic.morph_migrate(state, 0)

        self.assertIs(state["particles"], particles)
        self.assertEqual(state["frame"], 0)
        self.assertEqual(state["elapsed"], 0.0)

    def test_host_metadata_fallback_keeps_a_none_renderer_running(self) -> None:
        sys.path.insert(0, str(ROOT / "sdk" / "python"))
        sys.path.insert(0, str(ROOT / "demos" / "particles"))
        try:
            import app
        finally:
            sys.path.pop(0)
            sys.path.pop(0)

        state = {"particles": [[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]]}
        metadata = app.render_metadata(None, state)

        self.assertEqual(metadata["version"], "custom")
        self.assertEqual(metadata["particle_count"], 1)

    def test_initial_state_anchor_is_stable_for_the_same_particle_seed(self) -> None:
        sys.path.insert(0, str(ROOT / "sdk" / "python"))
        sys.path.insert(0, str(ROOT / "demos" / "particles"))
        try:
            import app
        finally:
            sys.path.pop(0)
            sys.path.pop(0)

        first = app.make_particles(8, seed=7)
        second = app.make_particles(8, seed=7)
        self.assertEqual(app.initial_state_anchor(first), app.initial_state_anchor(second))


if __name__ == "__main__":
    unittest.main()

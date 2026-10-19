from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class InitCommandTests(unittest.TestCase):
    def test_init_scaffolds_and_protects_existing_files(self) -> None:
        with tempfile.TemporaryDirectory() as raw_directory:
            destination = Path(raw_directory) / "my-app"
            first = subprocess.run(
                [
                    "cargo",
                    "run",
                    "--quiet",
                    "--locked",
                    "--offline",
                    "--",
                    "init",
                    str(destination),
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(first.returncode, 0, first.stderr)
            self.assertTrue((destination / "app.py").is_file())
            self.assertTrue((destination / "morph.toml").is_file())

            (destination / "app.py").write_text("keep this file\n", encoding="utf-8")
            second = subprocess.run(
                [
                    "cargo",
                    "run",
                    "--quiet",
                    "--locked",
                    "--offline",
                    "--",
                    "init",
                    str(destination),
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(second.returncode, 0)
            self.assertIn("--force", second.stderr)
            self.assertEqual(
                (destination / "app.py").read_text(encoding="utf-8"),
                "keep this file\n",
            )


if __name__ == "__main__":
    unittest.main()

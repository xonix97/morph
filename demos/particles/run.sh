#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
VENV="$SCRIPT_DIR/.venv"
PYTHON="${PYTHON:-python3.11}"

if ! command -v "$PYTHON" >/dev/null 2>&1; then
    printf 'Python 3.11 is required; set PYTHON to a compatible interpreter.\n' >&2
    exit 1
fi

if [[ ! -x "$VENV/bin/python" ]]; then
    "$PYTHON" -m venv "$VENV"
fi

"$VENV/bin/python" -m pip install --disable-pip-version-check \
    -r "$SCRIPT_DIR/requirements.lock"

export PATH="$VENV/bin:$PATH"
export PYTHONPATH="$REPO_ROOT/sdk/python:$SCRIPT_DIR${PYTHONPATH:+:$PYTHONPATH}"

exec cargo run --manifest-path "$REPO_ROOT/Cargo.toml" \
    --locked --offline -- dev "$SCRIPT_DIR" "$@"

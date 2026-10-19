"""Reloadable application behavior.

Keep durable state in Runtime.state in app.py. Module-level values here are
implementation settings and are intentionally replaced on reload.
"""

from __future__ import annotations


MORPH_RELOAD_VERSION = 1
VALUE_SCALE = 1
LABEL = "template-v1"


def render(state: dict) -> dict:
    return {
        "version": MORPH_RELOAD_VERSION,
        "value": state["ticks"] * VALUE_SCALE,
        "label": LABEL,
    }


def morph_migrate(state: dict, old_version: int) -> None:
    """Example migration hook for future state schema changes."""

    state.setdefault("seed", 7)

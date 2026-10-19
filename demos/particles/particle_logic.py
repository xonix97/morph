"""Reloadable particle simulation and OpenGL draw implementation.

Only implementation details belong in this module. The particle list, frame
counter, elapsed time, and random seed belong to the long-lived host's
``Runtime.state`` and therefore survive a successful reload.
"""

from __future__ import annotations

import math


MORPH_RELOAD_VERSION = 1

# Edit these values while the demo is running. Morph reloads this module and
# the next frame uses the new values without recreating the particle state.
PARTICLE_COLOR = (0.12, 0.05, 1.0)
BACKGROUND_COLOR = (0.008, 0.012, 0.035, 1.0)
POINT_SIZE = 3.5
SWIRL_STRENGTH = 1.15
RADIAL_FORCE = 0.34
DRAG = 0.985


def update(state: dict, dt: float) -> None:
    """Advance the durable particle state by one frame."""

    dt = min(max(float(dt), 0.0), 0.05)
    state["elapsed"] += dt
    state["frame"] += 1

    # Each particle is [x, y, z, vx, vy, vz, phase]. The list is deliberately
    # plain Python data so Runtime can deepcopy it for reload rollback.
    for particle in state["particles"]:
        x, y, z, vx, vy, vz, phase = particle
        radius = math.sqrt(x * x + y * y + z * z) + 0.001
        wobble = math.sin(state["elapsed"] * 1.7 + phase) * 0.18

        # A small central pull plus a tangent force creates a moving 3D cloud.
        pull = -RADIAL_FORCE * (radius - 2.2)
        vx += (x / radius * pull - y * SWIRL_STRENGTH + wobble * z) * dt
        vy += (y / radius * pull + x * SWIRL_STRENGTH - wobble * z) * dt
        vz += (z / radius * pull + wobble * x * 0.4) * dt

        vx *= DRAG
        vy *= DRAG
        vz *= DRAG
        x += vx * dt
        y += vy * dt
        z += vz * dt

        # Keep the cloud bounded without resetting the simulation state.
        if radius > 4.8:
            scale = 2.0 / radius
            x *= scale
            y *= scale
            z *= scale

        particle[0:6] = [x, y, z, vx, vy, vz]


def render(state: dict) -> dict:
    """Draw the current state using the active OpenGL context."""

    # Import on the render thread, not at module import time. Morph loads a
    # candidate module on its control thread, while pygame keeps the OpenGL
    # context on the application's main thread.
    from OpenGL import GL

    GL.glClearColor(*BACKGROUND_COLOR)
    GL.glPointSize(POINT_SIZE)
    GL.glBegin(GL.GL_POINTS)
    GL.glColor3f(*PARTICLE_COLOR)
    for x, y, z, _vx, _vy, _vz, _phase in state["particles"]:
        GL.glVertex3f(x, y, z)
    GL.glEnd()

    return {
        "version": MORPH_RELOAD_VERSION,
        "point_size": POINT_SIZE,
        "particle_count": len(state["particles"]),
    }


def morph_migrate(state: dict, old_version: int) -> None:
    """Keep older state shapes usable when the renderer version changes."""

    if old_version < 1:
        state.setdefault("elapsed", 0.0)
        state.setdefault("frame", 0)

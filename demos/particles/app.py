"""Long-lived pygame/OpenGL host for Morph's 3D particle demo."""

from __future__ import annotations

import math
import hashlib
import os
import random
import uuid

from morph_runtime import Runtime


MODULE = "particle_logic"
WINDOW_SIZE = (1100, 720)
PARTICLE_COUNT = 1800


def make_particles(count: int, seed: int) -> list[list[float]]:
    random_source = random.Random(seed)
    particles: list[list[float]] = []
    for _ in range(count):
        angle = random_source.random() * math.tau
        elevation = random_source.uniform(-1.0, 1.0)
        radius = random_source.uniform(0.25, 3.8)
        horizontal = math.sqrt(max(0.0, 1.0 - elevation * elevation))
        x = math.cos(angle) * horizontal * radius
        y = math.sin(angle) * horizontal * radius
        z = elevation * radius
        particles.append(
            [
                x,
                y,
                z,
                random_source.uniform(-0.12, 0.12),
                random_source.uniform(-0.12, 0.12),
                random_source.uniform(-0.12, 0.12),
                random_source.random() * math.tau,
            ]
        )
    return particles


def configure_projection(gl, width: int, height: int) -> None:
    aspect = max(width, 1) / max(height, 1)
    near = 0.1
    far = 100.0
    top = near * math.tan(math.radians(60.0) / 2.0)
    right = top * aspect

    gl.glViewport(0, 0, width, height)
    gl.glMatrixMode(gl.GL_PROJECTION)
    gl.glLoadIdentity()
    gl.glFrustum(-right, right, -top, top, near, far)
    gl.glMatrixMode(gl.GL_MODELVIEW)


def render_metadata(render_info, state: dict) -> dict:
    """Keep the host alive if a custom renderer returns no metadata.

    Drawing is the reloadable module's primary job. A user may edit ``render``
    to return ``None`` while experimenting; that should not tear down the
    simulation or lose its state just because the window title cannot show a
    custom logic version.
    """

    if isinstance(render_info, dict):
        return render_info
    return {
        "version": "custom",
        "point_size": None,
        "particle_count": len(state["particles"]),
    }


def initial_state_anchor(particles: list[list[float]]) -> str:
    """Return a stable fingerprint for the host-owned initial particle state."""

    sample = repr(particles[:8]).encode("utf-8")
    return hashlib.sha256(sample).hexdigest()[:10]


def main() -> None:
    import pygame

    runtime = Runtime([MODULE])
    runtime.state.update(
        {
            "seed": 7,
            "frame": 0,
            "elapsed": 0.0,
            "particles": make_particles(PARTICLE_COUNT, seed=7),
        }
    )
    runtime.state["world_id"] = uuid.uuid4().hex[:10]
    runtime.state["state_anchor"] = initial_state_anchor(runtime.state["particles"])
    try:
        # Acknowledge Morph before opening the graphics window. This gives the
        # watcher a useful startup result even if a machine has an OpenGL or
        # display-driver problem, instead of only reporting a timeout.
        print(
            f"PYTHON_PARTICLES_RUNTIME_READY pid={os.getpid()} "
            f"particles={PARTICLE_COUNT} seed={runtime.state['seed']} "
            f"world={runtime.state['world_id']} anchor={runtime.state['state_anchor']}",
            flush=True,
        )
        pygame.init()
        pygame.display.gl_set_attribute(pygame.GL_DOUBLEBUFFER, 1)
        pygame.display.set_caption("Morph 3D particles — starting")
        pygame.display.set_mode(
            WINDOW_SIZE,
            pygame.OPENGL | pygame.DOUBLEBUF | pygame.RESIZABLE,
        )
        # Import OpenGL only after pygame has created the context. This keeps
        # the host importable for tests and makes context ownership explicit.
        from OpenGL import GL

        configure_projection(GL, *WINDOW_SIZE)
        GL.glEnable(GL.GL_DEPTH_TEST)
        clock = pygame.time.Clock()
        last_logic_version = None

        print(
            f"PYTHON_PARTICLES_START pid={os.getpid()} "
            f"particles={PARTICLE_COUNT} seed={runtime.state['seed']} "
            f"world={runtime.state['world_id']} anchor={runtime.state['state_anchor']}",
            flush=True,
        )
        while runtime.running:
            for event in pygame.event.get():
                if event.type == pygame.QUIT:
                    runtime.stop()
                elif event.type == pygame.VIDEORESIZE:
                    configure_projection(GL, event.w, event.h)

            dt = min(clock.tick(60) / 1000.0, 0.05)
            runtime.call(MODULE, "update", runtime.state, dt)

            GL.glClear(GL.GL_COLOR_BUFFER_BIT | GL.GL_DEPTH_BUFFER_BIT)
            GL.glLoadIdentity()
            GL.glTranslatef(0.0, 0.0, -11.0)
            GL.glRotatef(runtime.state["elapsed"] * 9.0, 0.0, 1.0, 0.0)
            GL.glRotatef(runtime.state["elapsed"] * 4.0, 1.0, 0.0, 0.0)
            render_info = render_metadata(
                runtime.call(MODULE, "render", runtime.state), runtime.state
            )
            pygame.display.flip()

            logic_version = render_info["version"]
            frame = runtime.state["frame"]
            pygame.display.set_caption(
                "Morph 3D particles | "
                f"pid {os.getpid()} | frame {frame} | "
                f"elapsed {runtime.state['elapsed']:.1f}s | "
                f"logic v{logic_version} | "
                f"particles {render_info['particle_count']} | "
                f"world {runtime.state['world_id']}"
            )
            if logic_version != last_logic_version:
                print(
                    f"PYTHON_PARTICLES_LOGIC pid={os.getpid()} "
                    f"version={logic_version} frame={frame} "
                    f"elapsed={runtime.state['elapsed']:.2f} "
                    f"particles={render_info['particle_count']} "
                    f"world={runtime.state['world_id']} "
                    f"anchor={runtime.state['state_anchor']}",
                    flush=True,
                )
                last_logic_version = logic_version
    finally:
        runtime.close()
        pygame.quit()
        print(
            f"PYTHON_PARTICLES_STOP pid={os.getpid()} "
            f"frame={runtime.state['frame']} "
            f"elapsed={runtime.state['elapsed']:.2f}",
            flush=True,
        )


if __name__ == "__main__":
    main()

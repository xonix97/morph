# Hackathon demo plan

## One-line pitch

**Morph hot-reloads validated behavior while keeping the live application
state, so developers can iterate without restarting the world they are tuning.**

## The 90-second demo

Use the Python/OpenGL particle demo as the visual proof:

```powershell
.\run.ps1 -Particles
```

1. Show the particle window title: PID, frame, elapsed time, logic version,
   particle count, and world ID.
2. Say: “The host owns the window and simulation state. The reloadable module
   owns the particle behavior.”
3. Edit `demos/particles/particle_logic.py`:

   ```python
   MORPH_RELOAD_VERSION = 2
   PARTICLE_COLOR = (1.0, 0.18, 0.08)
   SWIRL_STRENGTH = 3.0
   ```

4. Save the file. Point out that the color/motion changes, while the PID,
   frame, elapsed time, particle count, and world ID continue.
5. Show:

   ```text
   PYTHON_RELOAD pid=... generation=1 state_preserved=True
   PYTHON_PARTICLES_LOGIC ... version=2 frame=... world=... anchor=...
   ```

6. Introduce a temporary syntax error and save. The old renderer continues and
   Morph reports a rejected candidate.
7. Finish: “Morph does not claim universal hot reload. It proves two explicit
   lifecycle boundaries today: controlled Python modules and versioned C
   shared libraries/DLLs.”

## Headless fallback

If a judge cannot run a graphics context:

```console
make test
```

The deterministic demos prove the same mechanism:

- C generation 1 → generation 2 in one host PID;
- host counter continuity;
- malformed DLL rejection;
- Python generation 1 → generation 2 in one worker PID;
- Python state continuity;
- syntax-error rejection.

## What to claim

Claim:

- validated in-process implementation swaps;
- deliberate state boundaries;
- last-good-generation behavior;
- C and Python integrations;
- detection-only reporting for unsupported languages.

Do not claim:

- universal hot reload;
- arbitrary native executable replacement;
- sandboxing or crash-proof native plugins;
- automatic migration of arbitrary objects;
- Rust, C++, Go, or JavaScript/TypeScript reload support.

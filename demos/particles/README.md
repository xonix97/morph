# Python + OpenGL 3D particle demo

This demo makes Morph's main idea visible:

```text
without Morph: edit code -> restart the app -> simulation state resets
with Morph:    edit code -> swap the renderer -> simulation state continues
```

The window contains a moving 3D particle cloud. The long-lived `app.py` host
owns the pygame window, OpenGL context, frame counter, elapsed time, and every
particle's position and velocity. `particle_logic.py` is the reloadable module:
it owns the update/render implementation and tunable values such as color,
point size, and swirl strength.

## Windows quick start

From PowerShell at the repository root:

```powershell
.\demos\particles\run.ps1
```

On Linux or macOS, use the POSIX launcher:

```sh
./demos/particles/run.sh
```

The launcher creates `demos/particles/.venv`, installs the pinned packages,
and starts the Morph watcher. It supports the WSL checkout path used by this
repository (`\\wsl$\Ubuntu\...`) by placing Cargo's build directory on a
local Windows volume.

If PowerShell blocks the script for the current session:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
```

To force dependency installation again:

```powershell
.\demos\particles\run.ps1 -Install
```

To run without Morph, for comparison:

```powershell
.\demos\particles\run.ps1 -Direct
```

The direct mode is useful for seeing the reset behavior after a normal
restart. The Morph mode is the state-preserving test.

## Test state-preserving reload

1. Start the Morph version:

   ```powershell
   .\demos\particles\run.ps1
   ```

2. Wait for the window title to show a non-zero frame and elapsed time, for
   example:

   ```text
   Morph 3D particles | pid 1234 | frame 3585 | elapsed 59.8s | logic v1 | particles 1800
   ```

3. Open `demos/particles/particle_logic.py` in another editor and change:

   ```python
   MORPH_RELOAD_VERSION = 1
   PARTICLE_COLOR = (0.12, 0.75, 1.0)
   ```

   to something visibly different:

   ```python
   MORPH_RELOAD_VERSION = 2
   PARTICLE_COLOR = (1.0, 0.18, 0.08)
   ```

   You can also change `POINT_SIZE` or `SWIRL_STRENGTH`. Save the file.

4. The terminal should report a new Python module generation. The window
   should change color/behavior while keeping the same PID, particle count,
   elapsed time, and continuing frame number. The particles do not return to
   their initial arrangement.

Expected terminal evidence:

```text
PYTHON_PARTICLES_START pid=1234 particles=1800 seed=7 world=ab12cd34ef anchor=9f0c2a1b77
PYTHON_PARTICLES_LOGIC pid=1234 version=1 frame=1 elapsed=0.02 particles=1800 world=ab12cd34ef anchor=9f0c2a1b77
PYTHON_RELOAD pid=1234 generation=1 state_preserved=True
PYTHON_PARTICLES_LOGIC pid=1234 version=2 frame=3586 elapsed=59.82 particles=1800 world=ab12cd34ef anchor=9f0c2a1b77
```

The title, same PID, same `world` ID, unchanged `anchor`, and the
`PYTHON_RELOAD` line are the proof that the application did not restart. A
normal restart would show a new PID, frame `1`, elapsed time near zero, and a
new world ID.

5. To test rejection without losing the running state, temporarily introduce
   a syntax error in `particle_logic.py`, save it, and watch the terminal.
   Morph should report a rejected candidate while the old particle renderer
   continues to animate. Fix the syntax error afterward.

Stop with the window close button or `Ctrl+C` in the terminal.

## Why the state survives

`Runtime.state` contains only ordinary Python data:

- `particles`: positions, velocities, and phases;
- `frame` and `elapsed`;
- the deterministic seed;
- a process-local `world` ID and initial-state `anchor` for visible continuity
  evidence.

The OpenGL context and pygame window are deliberately not placed in
`Runtime.state`; they belong to the long-lived host thread. The reloadable
module calls OpenGL only from `render(state)`, which the host invokes on the
thread where the context is current.

Do not use `from particle_logic import render` in the host. Morph resolves the
active function through:

```python
runtime.call("particle_logic", "render", runtime.state)
```

That is what makes the new renderer visible without restarting the process.

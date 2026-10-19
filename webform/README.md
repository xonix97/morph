# Morph landing page

A static, dependency-free product site for Morph. It includes an interactive canvas particle world that simulates the flagship state-preserving hot-reload proof.

## Run locally

From this directory:

```bash
python3 -m http.server 4173
```

Open http://localhost:4173.

The demo controls are intentionally local and deterministic:

- **Change behavior** changes the particle palette/motion and increments the logic generation while keeping the world ID, frame, elapsed time, and state anchor.
- **Break candidate** simulates a failed candidate; the last good generation stays active.

The visual is a product-storytelling layer for the real implementation in `../omni`.

# Python controlled reload demo

The Python adapter does not pretend that `importlib.reload()` updates every
existing reference. The application opts into Morph explicitly:

- `Runtime.state` owns durable state;
- `Runtime.call("app_logic", "render", state)` resolves the active module at
  call time;
- a candidate is loaded under a temporary module name before activation;
- `MORPH_RELOAD_VERSION` and `render(state)` are validated;
- an optional `morph_migrate(state, old_version)` can update state;
- failures leave the previous module active and are written to a status file.

Run the end-to-end demo from the repository root:

```console
cargo run --locked --offline -- demo python
```

Or start the actual watcher and edit `app_logic.py` in another terminal:

```console
cargo run --locked --offline -- dev demos/python
```

This is in-process module swapping, not a process restart. References obtained
with `from app_logic import render` are intentionally outside the supported
contract because they remain bound to the old function object. The bundled
demo also sends a syntax-error candidate after the successful `python-v2`
swap and verifies that the worker keeps the last valid module active.

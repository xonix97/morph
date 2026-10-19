# Morph Python controlled-reload template

This is the smallest reusable Python pattern for Morph:

```text
the host owns state
the module owns behavior
Morph swaps the module without restarting the host
```

`app.py` owns the process, PID, tick counter, and seed. `app_logic.py` owns
the reloadable implementation. The `morph.toml` file tells Morph to reload
only `app_logic.py`.

## Run from this checkout

From the repository root:

```console
cargo run --locked --offline -- detect templates/python-controlled --verbose
cargo run --locked --offline -- doctor templates/python-controlled
cargo run --locked --offline -- dev templates/python-controlled
```

Morph adds this checkout's `sdk/python` directory to `PYTHONPATH` when it
starts the host. For a direct run without Morph, set `PYTHONPATH` first:

PowerShell:

```powershell
$env:PYTHONPATH = "$PWD\sdk\python;$PWD\templates\python-controlled"
python .\templates\python-controlled\app.py
```

Unix shell:

```sh
PYTHONPATH="$PWD/sdk/python:$PWD/templates/python-controlled" \
  python3 templates/python-controlled/app.py
```

The template uses only the Python standard library. No package install is
required.

## See state survive a reload

Start Morph:

```console
cargo run --locked --offline -- dev templates/python-controlled
```

Then edit `templates/python-controlled/app_logic.py`:

```python
MORPH_RELOAD_VERSION = 2
VALUE_SCALE = 100
LABEL = "template-v2"
```

Save the file. The terminal should show the same PID and a continuing tick
counter, while the value and label change:

```text
PYTHON_TEMPLATE_TICK pid=1234 version=1 ticks=20 value=20 label=template-v1
PYTHON_RELOAD pid=1234 generation=1 state_preserved=True
PYTHON_TEMPLATE_TICK pid=1234 version=2 ticks=21 value=2100 label=template-v2
```

The tick count is in `Runtime.state`, so it does not reset. `VALUE_SCALE` and
`LABEL` are module implementation values, so the new generation replaces
them.

To test rejection, temporarily introduce a syntax error in `app_logic.py`.
Morph will leave the previous module active and the host will keep counting.
Fix the file afterward.

## Rules for adapting the template

- Put durable, deepcopy-compatible data in `runtime.state`.
- Call reloadable functions through `runtime.call("app_logic", ...)`.
- Do not use `from app_logic import render`; that reference will stay old.
- Keep host setup, event loops, windows, sockets, and external resources in
  `app.py`.
- Increment `MORPH_RELOAD_VERSION` and use `morph_migrate` when the state shape
  changes.
- Changes to `app.py` or `morph.toml` require restarting the host; only
  `app_logic.py` is configured for automatic reload.

To use the template outside this repository, copy `sdk/python/morph_runtime.py`
alongside the application or install/package the SDK, then make it importable
through `PYTHONPATH` or the application's normal Python packaging.

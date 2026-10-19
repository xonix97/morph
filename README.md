# Morph

## Hot-reload the behavior. Keep the live world.

Morph changes validated application behavior without restarting the running
process or losing deliberately host-owned state. It currently proves that
contract with a controlled Python runtime, a C shared-library/DLL boundary,
and a 3D particle simulation.

Morph is a capability-aware hot-reload engine for development workflows. It
gives a project one CLI while keeping the language-specific safety boundary
explicit:

```console
cargo run --locked --offline -- detect .
cargo run --locked --offline -- demo all
```

Morph is **not** a promise that arbitrary native memory, executable layouts,
Python references, or every framework can be replaced safely. It detects what
the project contains, selects only an integration that is configured and
available, and reports the exact boundary when it cannot reload a change.

## What works today

| Area | Status | Mechanism |
| --- | --- | --- |
| C | Implemented reference backend on Unix-like systems and Windows | Rebuild a shared library/DLL, validate a versioned ABI in a long-lived host, and activate a new generation while host-owned state continues. |
| Python | Implemented controlled backend | Load an allowlisted module under a temporary name, validate it, optionally migrate state, and swap the module slot in a running `morph_runtime.Runtime` process. |
| C++ | Detection only | Reports C++ projects and explains that no C++ ABI/reload policy is claimed. |
| Rust | Detection only | Reports Cargo projects; no unsafe Rust code replacement is claimed. |
| JavaScript/TypeScript | Detection only | Reports package/framework signals; framework-specific HMR is not silently delegated. |
| Go | Detection only | Reports Go modules; no binary/plugin compatibility claim is made. |

The C and Python adapters are real but intentionally narrow. A project must
opt into the integration boundary in `morph.toml`; an ordinary executable or
uncontrolled Python application is not marked reloadable merely because its
source file has changed.

## Installation and prerequisites

Morph is a Rust binary with no third-party Rust dependencies.

The reproducible core toolchain is pinned in the repository:

- Rust/Cargo `1.99.0` via `rust-toolchain.toml`;
- Python `3.11.2` via `.python-version`;
- a Cargo lockfile with no external Rust dependencies;
- a standard-library-only core Python runtime.

Install:

```console
cargo install --path . --locked
```

Or run from a checkout:

```console
cargo run --locked --offline -- detect .
```

Rustup reads `rust-toolchain.toml` automatically. On a clean Linux checkout,
install the system tools once, then run the canonical offline check:

```console
sudo apt-get update
sudo apt-get install --yes build-essential make python3.11 python3.11-venv
make test
```

`make test` is the single project check. It runs Cargo in locked/offline mode,
the Python test suite, and both deterministic C/Python demos. The optional
pygame/OpenGL particle demo is host-only and is not required by the core test.

General prerequisites:

- Rust/Cargo 1.99.0 (selected by `rust-toolchain.toml`);
- Python 3.11.2 for the core tests and standard-library template;
- a C compiler (`cc`, `gcc`, `clang`, or `g++`) for the C adapter/demo; Zig's
  `zig cc` driver is also accepted when the standard compiler names are
  unavailable;
- Unix `dlopen`/`dlsym` or Windows `LoadLibrary`/`GetProcAddress` support for
  the included reference C host.

The included host implements both the Unix and Windows dynamic-loader APIs.
On Windows, use a MinGW/MSYS2 `g++` that is available on `PATH`; Morph builds
`.dll` generations and validates the exported `morph_plugin_v1` ABI symbol.

The optional particle demo uses a separate project-local environment and pins
`pygame==2.6.1` and `PyOpenGL==3.1.10` in
`demos/particles/requirements.lock`. It is intentionally not part of the
offline core test or the Linux container image because it requires a graphical
host and OpenGL context.

## CLI

### Detect a project

```console
morph detect [PATH] [--verbose]
```

Detection uses multiple signals rather than trusting one filename. It reports:

- project root and nested manifest roots;
- detected languages and build systems;
- framework hints and available runtimes;
- selected backend and confidence/ambiguity;
- capabilities, missing tools, and unsafe categories.

Examples:

```console
morph detect .
morph detect demos/c --verbose
morph detect demos/python
```

If a project contains conflicting language signals or lacks an integration
boundary, Morph selects `detection-only`. It does not randomly pick a backend.

### Check prerequisites

```console
morph doctor [PATH] [--verbose]
```

`doctor` checks the tools relevant to the detected languages and the selected
backend. It exits nonzero when the selected integration cannot run.

### Start development mode

```console
morph dev demos/python
morph dev demos/c --verbose
```

Morph starts the configured host, watches only configured or relevant source
paths, debounces changes, and forwards a backend-specific reload request. A
build failure or rejected activation leaves the last working generation in
place. Press Ctrl-C to stop the development process and its child host.

### List backend capabilities

```console
morph backends
```

This command describes actual implementation status rather than listing
planned adapters as if they were complete.

### Run reproducible demonstrations

```console
morph demo c
morph demo python
morph demo all
```

The demos use temporary copies and do not modify tracked source files. Each
demo first applies a valid generation, then sends an invalid candidate: the C
demo submits a shared library without `morph_plugin_v1`, while the Python demo
submits a syntax error. Both must keep the last working generation active.

### Windows quick start

With Rust/Cargo and MinGW/MSYS2 `g++` on `PATH`, run from PowerShell:

```powershell
.\run.ps1
```

That builds Morph and runs the Windows C/DLL demo. To start the file watcher
instead:

```powershell
.\run.ps1 -Dev
```

For the state-preserving Python/OpenGL particle demo, use:

```powershell
.\run.ps1 -Particles
```

For the dependency-free Python template:

```powershell
.\run.ps1 -Template
```

To scaffold the same template into a new application directory:

```powershell
morph init my-app
cd my-app
morph dev .
```

The command refuses to overwrite any generated file unless you explicitly
pass `--force`.

The particle-specific launcher is also available at
`demos/particles/run.ps1` and accepts `-Install` or `-Direct`. The root
launcher forwards `-Install` as well.

The script also handles the common `\\wsl$\Ubuntu\...` checkout path: Cargo
build artifacts are redirected to a local Windows directory because Windows
does not support Cargo's incremental lock files reliably on the WSL network
share.

If PowerShell blocks local scripts, allow this checkout for the current
PowerShell session with `Set-ExecutionPolicy -Scope Process Bypass` and run
the script again. The script is a launcher; the native C ABI host is still
Morph's `internal-c-host` process.

## C integration

An ordinary C executable is not safely hot-reloadable by Morph. The supported
boundary is a host process plus a replaceable shared library.

The host includes `sdk/c/morph_reload.h` and exposes a descriptor:

```c
typedef struct MorphPluginV1 {
    uint32_t abi_version;
    const char *name;
    const char *(*render)(uint64_t host_state);
} MorphPluginV1;

MORPH_PLUGIN_EXPORT const MorphPluginV1 *morph_plugin_v1(void);
```

The host owns durable state and passes a simple value into the module. For the
bundled example, that value is a counter that starts at `1` and continues
while the plugin is rebuilt. Each reload builds a uniquely named library
generation. The reference host:

1. loads the new file with `dlopen` on Unix or `LoadLibraryW` on Windows;
2. resolves `morph_plugin_v1` with `dlsym` or `GetProcAddress`;
3. validates the ABI version and required function;
4. switches only after validation;
5. keeps old library handles mapped rather than unloading code that might
   still be referenced.

This means a counter at `3585` remains at `3585`/`3586` after a successful
reload; it does not restart at `1`. The counter must live in the host boundary,
as it does here. A standalone C program with `static int count` inside the
replaced DLL cannot have that memory preserved automatically: move the state
to the host or add an explicit serialize/migrate contract.

That last choice is conservative and can consume memory over a long session.
It avoids the more dangerous claim that arbitrary old threads and callbacks
are quiescent. A production host can add a quiescence protocol before
unloading generations. The ABI is a trusted native-code boundary: invalid
plugin pointers or a crashing plugin can still crash the host. Morph does not
sandbox native libraries.

`demos/c/morph.toml` shows the minimal configuration:

```toml
[morph]
backend = "c-native"

[c]
plugin_source = "plugin/plugin.c"
plugin_output = ".morph/c/plugin"
host_command = ["{morph}", "internal-c-host", "--library", "{library}", "--control", "{control}", "--status", "{status}"]
status_file = ".morph/c/reload.status"
watch = ["plugin/plugin.c", "../../sdk/c/morph_reload.h"]
```

Supported command substitutions are `{root}`, `{morph}`, `{source}`,
`{output}`, `{library}`, `{control}`, and `{status}`. A custom host must
implement the same ABI, consume the control file atomically, and write a
generation acknowledgement containing its PID to the configured status file.
Morph does not overwrite source files or edit the host configuration.

## Python integration

Python's normal import model makes blind `importlib.reload()` unsafe: existing
`from module import function` references and class instances do not magically
become new objects. Morph therefore requires an explicit controlled runtime in
`sdk/python/morph_runtime.py`.

The supported application pattern is:

```python
from morph_runtime import Runtime

runtime = Runtime(["app_logic"])
value = runtime.call("app_logic", "render", runtime.state)
```

The runtime:

- allowlists modules;
- loads a candidate under a temporary module name;
- validates `MORPH_RELOAD_VERSION` and `render(state)`;
- optionally calls `morph_migrate(state, old_version)`;
- swaps the module slot only after validation;
- keeps durable state in `Runtime.state`;
- acknowledges success or failure through a status file.

The following are intentionally unsupported:

- `from app_logic import render` references that must update automatically;
- arbitrary module side effects that need rollback;
- changing the shape of objects already held outside `Runtime.state`;
- native extensions that are not safe to reload;
- claiming success when the worker only restarted.

`demos/python/morph.toml` configures the adapter:

```toml
[morph]
backend = "python-controlled"

[python]
command = ["python3", "app.py"]
modules = ["app_logic"]
control_file = ".morph/python/reload.request"
status_file = ".morph/python/reload.status"
watch = ["app_logic.py"]
```

Morph adds the checkout's `sdk/python` directory to `PYTHONPATH` when running
the configured development command. Only reloadable implementation files
belong in `python.watch`; host files, configuration, and the runtime SDK
require a session restart when changed. A deployed application should vendor
or package the SDK explicitly rather than relying on a source checkout.

### Python + OpenGL state demo

`demos/particles/` is a visual version of the same contract. It uses a
long-lived pygame/OpenGL host and keeps particle positions, velocities, frame
count, and elapsed time in `Runtime.state`. `particle_logic.py` is reloadable,
so changing its color, point size, or force changes the live 3D particle cloud
without resetting the window or simulation. On Windows:

```powershell
.\run.ps1 -Particles
```

The launcher creates a project-local virtual environment and installs the
pinned `pygame` and `PyOpenGL` packages from
`demos/particles/requirements.lock`. See `demos/particles/README.md` for the
reload experiment and the direct-run comparison.
Its configuration also declares `imports = ["pygame", "OpenGL.GL"]`, so
`morph detect` and `morph doctor` report missing graphics dependencies before
the watcher starts instead of waiting for a runtime timeout.

The same demo has a POSIX launcher at `demos/particles/run.sh`. The concise
judge-facing pitch and 90-second walkthrough are in
`docs/hackathon-demo.md`.

### Reusable Python template

For a dependency-free Python application, copy or adapt
`templates/python-controlled/`. It contains a long-lived host, a reloadable
logic module, a minimal `morph.toml`, and exact instructions for proving that
the host PID and `Runtime.state` continue across an edit:

```console
cargo run --locked --offline -- dev templates/python-controlled
```

The template is the recommended starting point for a real Python application;
the particle demo adds graphics while preserving the same state boundary.

## Architecture

```text
CLI
 ├── detect.rs       project signals, manifests, toolchains, backend choice
 ├── config.rs       small explicit morph.toml integration contract
 ├── watch.rs        polling watcher, filtering, debounce, snapshots
 └── backend.rs      stable backend/session interface
      ├── c_backend.rs       compile generation + host activation request
      └── python_backend.rs  controlled worker request + acknowledgement

runtime host
  └── c_host.rs       reference Unix/Windows loader for the C ABI demo

SDKs
 ├── sdk/c/morph_reload.h
 └── sdk/python/morph_runtime.py
```

The core does not know how to reload C symbols or Python modules. It forwards
debounced changes to a backend session and reports a typed outcome:

- `applied` — the backend acknowledged activation;
- `rejected` — the candidate was not activated;
- `build-failed` — the working generation remains active;
- `restart-required` — Morph refuses to restart automatically;
- `unsupported` — the backend cannot safely perform the operation;
- `indeterminate` — activation may have happened but its acknowledgement was
  lost, so Morph refuses to claim success.

## Backend matrix and current limits

| Language | Detection | Automatic selection | Applies changes | State guarantee |
| --- | --- | --- | --- | --- |
| C | Yes | With `[c]` integration | Yes, shared-library generation | Host-owned state only; old code remains mapped |
| C++ | Yes | No | No | No claim |
| Rust | Yes | No | No | No claim about Rust ABI/layout |
| Python | Yes | With `[python]` integration | Yes, allowlisted module slots | `Runtime.state` plus optional migration |
| JavaScript/TypeScript | Yes | No | No | Use framework HMR directly until a Morph adapter exists |
| Go | Yes | No | No | Go plugin/runtime compatibility is not assumed |

This matrix is deliberately narrower than Morph's vision. Adding a backend
requires an actual lifecycle implementation, a capability description, failure
tests, and a reproducible demonstration. It is not enough to add a file
extension or print “reloaded.”

## Tests

Run the one canonical offline project check:

```console
make test
```

It runs the locked/offline Rust tests, the Python test suite, and the
deterministic C/Python demonstrations. The equivalent individual commands are:

```console
cargo test --locked --offline
python3.11 -m unittest discover -s tests -v
cargo run --locked --offline -- demo all
```

The test suite covers detection signals in Rust, generated-file filtering and
debouncing primitives, Python state preservation, successful controlled module
activation, failed candidate activation, and stale-reference behavior. The
end-to-end C test is the `morph demo c` workflow because it requires the local
C compiler and dynamic loader available on the development machine.
The template test also checks that its PID and tick state continue across a
real module-generation swap.

## Build and deploy

Morph is a development CLI, not a production server. A reproducible
distribution is either the locked release binary or the Linux container:

```console
cargo build --release --locked --offline
./target/release/morph demo all
```

Build and run the container:

```console
docker build -t morph:0.1.0 .
docker run --rm morph:0.1.0
```

The `Dockerfile` installs the pinned core system tools, builds with Cargo
offline, runs `make test` during the image build, and defaults to the
headless-safe deterministic demos. The pygame/OpenGL particle demo remains a
host-run Windows/Linux desktop workflow.

Do not commit credentials. The repository ignores `.env` files, private keys,
certificates, and credential directories; `.env.example` contains only the
documented no-secret policy.

## Troubleshooting

### `detection-only` appears

Run `morph detect PATH --verbose`. If multiple languages are present, add a
`morph.toml` and choose one explicit integration. For C, configure
`plugin_source` and a `host_command`; for Python, configure `command` and an
explicit `modules` list.

### C build fails

Run `morph doctor demos/c`. On Windows, install MinGW/MSYS2 and put the
directory containing `g++.exe` on `PATH`. On Unix, install `cc`, `gcc`, or
`clang`; Zig's `zig cc` driver is also accepted. Then verify that the plugin
exports `morph_plugin_v1` and uses the ABI header. A compiler error does not
destroy the last loaded generation.

### Python reload is rejected

Inspect `.morph/python/reload.status`. Syntax errors, a missing `render(state)`
function, an invalid `MORPH_RELOAD_VERSION`, or a failed migration leave the
previous module active. Replace `from module import name` with
`runtime.call("module", "name", ...)`.

### A project uses a framework with its own HMR

Morph currently reports JavaScript/TypeScript as detection-only. Use the
framework's established HMR mechanism until a backend can prove its own
capabilities and state semantics.

## Implementing a new backend

1. Add detection signals and a clear `BackendInfo` in `src/detect.rs`.
2. Define a real integration boundary in `morph.toml`.
3. Implement `Backend` and `BackendSession` in a separate module.
4. Return `BuildFailed`, `Rejected`, or `RestartRequired` instead of claiming
   activation when safety checks fail.
5. Add temporary-project tests for detection, missing tools, build failures,
   state behavior, and unsupported changes.
6. Add a demo that shows the same process or an explicitly documented restart
   boundary, with a measurable state/result check.
7. Update the matrix and limitations in this README.

## Project status

Implemented:

- Rust CLI and backend contract;
- multi-signal detection for C/C++, Rust, Python, JavaScript/TypeScript, and Go;
- ambiguity and missing-tool reporting;
- polling watcher with generated-directory filtering and debounce;
- C shared-library/DLL generation backend and Unix/Windows reference host;
- Python controlled module swap runtime and acknowledgement protocol;
- Python + OpenGL 3D particle state-preservation demonstration;
- reusable dependency-free Python controlled-reload template;
- deterministic C and Python demonstrations;
- unit tests and Python integration tests.

Not implemented yet:

- C++/Rust/JavaScript/TypeScript/Go reload adapters;
- OS-native filesystem event APIs instead of polling;
- automatic state migration for arbitrary user objects;
- remote build isolation, sandboxing, or production deployment management.

Morph is useful now as a safe foundation and two real reference integrations,
not as a universal claim that every program can be hot-reloaded.

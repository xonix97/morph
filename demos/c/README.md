# C reload boundary demo

This directory demonstrates the only C workflow Morph claims to support in
the first release: a long-lived host process loads a shared library (or DLL on
Windows) through a versioned ABI. The host owns `host_state`, while the library
owns only its rendering implementation.

From the repository root:

```console
cargo run --locked --offline -- demo c
```

On a supported Unix or Windows development machine, the output contains one
PID across two library generations, for example:

```text
MORPH_C_HOST_START pid=1234 ...
MORPH_C_RELOAD pid=1234 generation=1 host_state=1 ...
MORPH_C_TICK pid=1234 state=3 value=version=1 count=3
MORPH_C_RELOAD pid=1234 generation=2 host_state=3585 ...
MORPH_C_TICK pid=1234 state=3585 value=version=2 count=3585
```

The demo changes a copy in a temporary directory. It does not modify the
tracked source file. After the valid `version=2` generation is active, it also
submits a shared library that does not export `morph_plugin_v1`; the host
reports the rejection and continues rendering `version=2`.

`morph dev demos/c` uses the same ABI and starts the reference host; changing
`plugin/plugin.c` while it runs requests another generation. On Windows, put
the MinGW/MSYS2 directory containing `g++.exe` on `PATH`, then run
`..\..\run.ps1 -Dev` from PowerShell. A normal Unix C compiler is preferred
on Unix, but the implementation also accepts the `zig cc` driver when the
standard compiler names are unavailable.

The counter belongs to the long-lived host, not the reloadable plugin. To see
state continuity yourself:

1. Start `..\..\run.ps1 -Dev` in one PowerShell window.
2. Let the `count=` value advance, or wait until your real host reaches a
   meaningful state such as `3585`.
3. Edit `plugin/plugin.c` in another window, for example change
   `MORPH_DEMO_VERSION 1` to `MORPH_DEMO_VERSION 2`, and save it.
4. Watch for `MORPH_C_RELOAD ... host_state=...` followed by ticks with the
   same continuing count and the new plugin behavior.

If the counter is declared as a global or `static` inside the reloadable C
library instead, it belongs to that generation and will not be preserved by
this ABI. Put durable state in the host and pass it through `render`.

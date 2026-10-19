# Morph architecture

Morph has four independent concerns:

1. **Detection** collects manifests, source extensions, framework hints, and
   available tools. It produces a `ProjectProfile` and does not start code.
2. **Watching** snapshots only configured/relevant files, filters generated
   directories, and emits a debounced set of changed paths.
3. **Backend orchestration** owns build, candidate validation, activation,
   acknowledgement, rollback/rejection, and child lifecycle.
4. **Runtime boundaries** are supplied by the project: a C host ABI or the
   Python `Runtime` module-slot protocol.

The `Backend` trait is intentionally small:

```rust
trait Backend {
    fn id(&self) -> &'static str;
    fn start(&self, context: BackendContext) -> MorphResult<Box<dyn BackendSession>>;
}

trait BackendSession {
    fn watch_paths(&self) -> Vec<PathBuf>;
    fn reload(&mut self, changed: &[PathBuf]) -> MorphResult<ReloadReport>;
    fn is_alive(&mut self) -> MorphResult<bool>;
    fn stop(&mut self);
}
```

The contract does not expose language-specific operations to the watcher or
CLI. A new backend can therefore add its own builder, validator, process
protocol, and state model without changing the core loop.

## Safety invariants

- A failed build never changes the active C generation.
- A C host validates ABI version and function presence before switching.
- Old C library handles are kept mapped by the reference host.
- Python candidates load under a temporary name before a slot swap.
- Python durable state lives outside reloadable module globals.
- A backend must return an explicit non-success status for unsupported changes.
- Detection-only projects never enter the development loop.

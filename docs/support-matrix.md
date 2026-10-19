# Support matrix

| Backend | Project signal | Explicit setup | Runtime mechanism | Current status |
| --- | --- | --- | --- | --- |
| C native | `.c`, CMake/Make | `[c] plugin_source`, `host_command` | Versioned shared-library/DLL ABI and control file | Implemented on Unix and Windows reference hosts |
| Python controlled | `.py`, pyproject/requirements | `[python] command`, `modules` | Candidate module load and `Runtime.call` slot swap | Implemented |
| C++ | `.cpp`, CMake/Make | None | None | Detection only |
| Rust | `Cargo.toml`, `.rs` | None | None | Detection only |
| JavaScript/TypeScript | `package.json`, `tsconfig.json` | None | None | Detection only |
| Go | `go.mod`, `.go` | None | None | Detection only |

“Detection only” is a deliberate status. It means Morph can tell the user
what it sees and why it cannot safely select a reload backend. It does not mean
the project is broken or that Morph has delegated to an unverified framework
tool.

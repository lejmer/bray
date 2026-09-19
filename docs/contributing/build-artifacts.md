# Build artifacts across checkouts

Run repository commands from the Bray checkout you intend to change, or one of its Cargo member directories.
Commands select the workspace containing the current directory, even when you run a retained executable from a shared
Cargo target directory.

## Choose artifact storage

Cargo keeps intermediate artifacts and compiled dependencies in each checkout's `target/cargo`. A checkout's first
build compiles its own dependencies. Returning to that checkout reuses them without cleaning or forcing a rebuild.
Cargo's download cache and an explicitly shared LLVM installation remain shared.

`CARGO_TARGET_DIR` and `--target-dir` select final artifact destinations, including the separate native ThinLTO target.
Shared destinations contain the most recently selected checkout's outputs. Serialize builds and consumption of those
outputs, or use separate final target directories for concurrent work. Do not share `CARGO_BUILD_BUILD_DIR` between
checkouts, because that disables intermediate isolation.

This separation prevents Cargo's relative-path and timestamp checks from reusing another checkout's source artifacts.
See [Cargo's build directory setting](https://doc.rust-lang.org/cargo/reference/config.html#buildbuild-dir) and the
[upstream source-identity issue](https://github.com/rust-lang/cargo/issues/12516).

## Build toolchain bundles

Use the focused guides for [LLVM provisioning](llvm.md), [runtime artifacts](runtime-artifacts.md), and
[standard-library bundles](standard-library.md#build-a-bundle). The runtime and standard-library guides include the
installed paths expected beside release binaries.

Long workflows print phase starts, completions, and repeated-work counts to standard error. Child compiler progress
remains visible while machine-readable command output stays on standard output.

Return to [repository tasks](xtask.md) to choose another workflow.

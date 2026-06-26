## What we're building

We're building a compiler for a brand-new programming language. It's an opinionated language that blends ideas from Rust, Go, Mojo and Odin, but even more opinionated.

## The state of the repo

Bray is currently a greenfield project. Therefore, do not preserve backwards-compatibility unless explicitly requested. Always go for clean, coherent design changes instead of compatibility layers, migration shims, deprecated aliases, or legacy fallbacks.

## Structured messages

Do not construct user-facing English text inside compiler logic. Emit structured message IDs and typed arguments instead. User-facing text must be rendered through `bray-messages`.

## Coding conventions

Read and follow [coding-conventions.md](docs/contributing/coding-conventions.md).

## Crates

Read and follow [crates.md](docs/contributing/crates.md).

## Building

Bray uses Cargo as the build system.

Project-specific automation belongs in `xtask/`. Shell and PowerShell scripts in `scripts/` must stay thin wrappers around Cargo or `cargo xtask` commands.

Do not add a second build system, task runner, or command DSL unless explicitly requested. Prefer extending `xtask` over adding Makefiles, Justfiles, cargo-make tasks, Bazel files, or ad hoc scripts.

`cargo test` must continue to work. Additional test runners such as `cargo-nextest` may be used for faster local and CI runs, but they must not be the only way to run the test suite.

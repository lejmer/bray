## What we're building

We're building a compiler for a brand-new programming language. It's an opinionated language that blends ideas from Rust, Go, Mojo and Odin, but even more opinionated.

## The state of the repo

Bray is currently a greenfield project. Therefore, do not preserve backwards-compatibility unless explicitly requested. Always go for clean, coherent design changes instead of compatibility layers, migration shims, deprecated aliases, or legacy fallbacks.

Treat discovered gaps as part of the active PR unless folding them in would turn it into a multi-thousand-line change or a genuinely independent subsystem. 

## Simplicity and scope

Implement the smallest coherent design that satisfies the current requirements and known near-term consumers. Do not add abstraction layers, wrapper types, request/context/result types, validation surfaces, or APIs based only on speculative future needs. Forward-looking APIs are appropriate when a planned consumer and its required contract are already understood. Every new type must enforce a current or planned invariant, establish meaningful identity or ownership, or provide behavior that cannot be expressed clearly by an existing type or direct function.

Do not treat existing code as permanent. When touching an area, delete or flatten redundant layers, forwarding APIs, duplicated representations, and speculative infrastructure when a simpler implementation preserves immutability, demand-driven evaluation, safe parallelism, determinism, efficiency, correctness, reliability, and structured diagnostics. Make these simplifications as part of the relevant work instead of creating standalone cleanup issues.

Keep reviews bounded by the issue contract. Fix correctness defects, architectural violations, and mandatory convention failures, but move unrelated improvements to later work instead of repeatedly expanding or rewriting the current change.

## Structured messages

Do not construct user-facing English text inside compiler logic. Emit structured message IDs and typed arguments instead. User-facing text must be rendered through `bray-messages`.

## Coding conventions

If the task involves writing code, read and follow [coding-conventions.md](docs/contributing/coding-conventions.md).

Review blank lines only for semantic paragraph structure: separate statements when their purpose changes, and keep statements
together when they form one conceptual group. Mechanically decidable blank-line enforcement belongs to the automated style command
rather than manual agent review.

## Refactoring modules

When splitting a module into a directory of submodules, keep the original module file as a thin root. It should contain only module declarations and reexports. Move implementation details into the submodules.

Apply the same rule at every level you touch. If a nested module is split into its own submodules, give that nested module the same thin-root shape.

## Crates

If the task involves writing code, read and follow [crates.md](docs/contributing/crates.md).

## Building

Bray uses Cargo as the build system.

Project-specific automation belongs in `xtask/`. Shell and PowerShell scripts in `scripts/` must stay thin wrappers around Cargo or `cargo xtask` commands.

Do not add a second build system, task runner, or command DSL unless explicitly requested. Prefer extending `xtask` over adding Makefiles, Justfiles, cargo-make tasks, Bazel files, or ad hoc scripts.

`cargo test` must continue to work. Additional test runners such as `cargo-nextest` may be used for faster local and CI runs, but they must not be the only way to run the test suite.

After changing Rust source, running `cargo xtask style` is mandatory. It applies deterministic fixes and runs all structural
checks described in [coding-conventions.md](docs/contributing/coding-conventions.md); do not substitute the non-mutating
`cargo xtask style check` in the agent workflow.

> **Note:** If you do not change any code, you do not need to run tests, linting, or code checks.

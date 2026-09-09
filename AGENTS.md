## What we're building

We're building a compiler for a brand-new programming language. It's an opinionated language that blends ideas from Rust, Go, Mojo and Odin, but even more opinionated.

## The state of the repo

Bray is currently a greenfield project. Therefore, do not preserve backwards-compatibility unless explicitly requested. Always go for clean, coherent design changes instead of compatibility layers, migration shims, deprecated aliases, or legacy fallbacks.

Plan substantial work as coherent, independently reviewable outcomes before implementation. Reassess the split when a discovered gap introduces substantial new machinery, before the diff becomes too large for a human to review. Fix bounded defects in the active PR, and track substantial prerequisites or follow-on outcomes in dependency-ordered issues and PRs. Preserve the complete intended result across the stack, and honor explicit instructions to finish an already expanded PR in full.

Commit and push coherent, tested milestones during long tasks rather than waiting for review readiness. Label incomplete checkpoints as work in progress and retain their known gaps and verification status.

Do not dismiss a discovered defect as unrelated. Fix bounded defects in the active work; otherwise link an existing tracking issue or create one before proceeding. Treat failures in fail-fast workflows as blocking because later checks did not run.

## Simplicity and scope

Implement the smallest coherent design that satisfies the current requirements and known near-term consumers. Do not add abstraction layers, wrapper types, request/context/result types, validation surfaces, or APIs based only on speculative future needs. Forward-looking APIs are appropriate when a planned consumer and its required contract are already understood. Every new type must enforce a current or planned invariant, establish meaningful identity or ownership, or provide behavior that cannot be expressed clearly by an existing type or direct function.

Do not treat existing code as permanent. When touching an area, delete or flatten redundant layers, forwarding APIs, duplicated representations, and speculative infrastructure when a simpler implementation preserves immutability, demand-driven evaluation, safe parallelism, determinism, efficiency, correctness, reliability, and structured diagnostics. Make these simplifications as part of the relevant work instead of creating standalone cleanup issues.

Keep reviews bounded by the issue contract. Fix correctness defects, architectural violations, and mandatory convention failures, but move unrelated improvements to later work instead of repeatedly expanding or rewriting the current change.

## Structured messages

Do not construct user-facing English text inside compiler logic. Emit structured message IDs and typed arguments instead. User-facing text must be rendered through `bray-messages`.

Every failure path must preserve the most specific known cause through error and diagnostic conversions. When a useful diagnostic needs context that the successful path does not otherwise require, carry enough identity to retrieve it and perform that lookup only while constructing the failure. Put complex or repeated diagnostic-context lookup in a helper. Reserve broad categories such as `InfrastructureFailure` for failures that remain genuinely broad after the available context has been examined.

## Coding conventions

If the task involves writing code, read and follow [coding-conventions.md](docs/contributing/coding-conventions.md).

Review blank lines only for semantic paragraph structure: separate statements when their purpose changes, and keep statements together when they form one conceptual group. Mechanically decidable blank-line enforcement belongs to the automated style command rather than manual agent review.

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

After changing Rust source, running `cargo xtask style` is mandatory. It applies deterministic fixes and runs all structural checks described in [coding-conventions.md](docs/contributing/coding-conventions.md). Do not substitute the non-mutating `cargo xtask style check` in the agent workflow.

Native toolchain workflows that spawn LLVM, archiver, linker, or produced-executable processes must run outside the Codex sandbox. On Windows, sandboxed child processes can report `permission denied` even when their temporary files are redirected beneath the workspace. Treat that as an execution-environment restriction, not a compiler defect, and do not add production workarounds for it.

> **Note:** If you do not change any code, you do not need to run tests, linting, or code checks.

## Writing Bray code

Always use the `Use Bray Language` skill.

Run `cargo xtask format` before you finish a task.

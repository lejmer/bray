## What we're building

We're building a compiler for a brand-new programming language. It's an opinionated language that blends ideas from Rust, Go, Mojo and Odin, but even more opinionated.

## The state of the repo

Bray is currently a greenfield project. Therefore, do not preserve backwards-compatibility unless explicitly requested. Always go for clean, coherent design changes instead of compatibility layers, migration shims, deprecated aliases, or legacy fallbacks.

Treat discovered gaps as part of the active PR unless folding them in would turn it into a multi-thousand-line change or a genuinely independent subsystem. 

Do not dismiss a discovered defect as unrelated. Fix bounded defects in the active work; otherwise link an existing tracking issue or create one before proceeding. Treat failures in fail-fast workflows as blocking because later checks did not run.

## Simplicity and scope

Implement the smallest coherent design that satisfies the current requirements and known near-term consumers. Do not add abstraction layers, wrapper types, request/context/result types, validation surfaces, or APIs based only on speculative future needs. Forward-looking APIs are appropriate when a planned consumer and its required contract are already understood. Every new type must enforce a current or planned invariant, establish meaningful identity or ownership, or provide behavior that cannot be expressed clearly by an existing type or direct function.

Do not treat existing code as permanent. When touching an area, delete or flatten redundant layers, forwarding APIs, duplicated representations, and speculative infrastructure when a simpler implementation preserves immutability, demand-driven evaluation, safe parallelism, determinism, efficiency, correctness, reliability, and structured diagnostics. Make these simplifications as part of the relevant work instead of creating standalone cleanup issues.

Keep reviews bounded by the issue contract. Fix correctness defects, architectural violations, and mandatory convention failures, but move unrelated improvements to later work instead of repeatedly expanding or rewriting the current change.

## Design documentation

Before editing anything under `docs/design/`, read and follow [docs/design/AGENTS.md](docs/design/AGENTS.md).

## Repository approvals

Before requesting approval to push or write through repository-hosting tools, verify the exact destination, repository visibility and ownership, and authenticated account using read-only commands. Include those verified results in the approval request.

Reuse verified evidence during the task unless the destination or authentication changes. If trust is unresolved, gather the missing evidence before requesting approval.

## Structured messages

Do not construct user-facing English text inside compiler logic. Emit structured message IDs and typed arguments instead. User-facing text must be rendered through `bray-messages`.

Every failure path must preserve the most specific known cause through error and diagnostic conversions. When a useful diagnostic needs context that the successful path does not otherwise require, carry enough identity to retrieve it and perform that lookup only while constructing the failure. Put complex or repeated diagnostic-context lookup in a helper. Reserve broad categories such as `InfrastructureFailure` for failures that remain genuinely broad after the available context has been examined.

## Coding conventions

If the task involves writing code, read and follow [coding-conventions.md](docs/contributing/coding-conventions.md).

Optimize for the maintainability of the whole system, not just completion of the current task.
1. Understand the existing design before extending it. Examine how the requested behavior fits the surrounding system. Look for existing mechanisms that can express it and for design problems that would otherwise force more special cases.
2. Prefer changes that reduce the number of things we must understand. Consider representations, concepts, conversions, policies and execution paths, not only duplicated lines. Sharing helpers is useful, but does not justify unnecessary machinery.
3. Replace mechanisms completely within the chosen scope. When introducing a better representation or approach, identify what it replaces and remove the superseded machinery. Do not leave parallel implementations, forwarding layers or compatibility scaffolding without a demonstrated requirement.
4. Make code growth earn its place. New functionality may require more code. Explain what necessary capability the growth buys and why a simpler implementation is insufficient. Measure production code and tests separately. Moving code, compressing formatting, weakening behavior or deleting useful tests does not count as simplification.
5. Use implementation difficulties to reconsider the design. Unexpected layers, repeated conversions, accumulating exceptions or substantially greater scope are reasons to revisit the approach. Do not automatically solve each difficulty by adding another mechanism.
6. Keep the work bounded without protecting a poor design. Simplify the mechanisms involved in the task. Do not turn every task into a codebase-wide rewrite, but do not preserve an unnecessary mechanism merely because replacing it crosses files or modules.
7. Preserve the agreement across context changes. Keep a short task record with the behavioral requirements, baseline, design hypothesis, expected removals, growth constraints and reasons to reconsider. Re-read it after compaction. Do not silently revise these commitments to match the implementation.
8. Evaluate correctness and structural improvement separately. Passing tests, satisfying acceptance criteria and completing review are necessary. Also assess whether the resulting system has fewer unnecessary concepts and whether its remaining complexity is justified.

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

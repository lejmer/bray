# Build and verify runtime artifacts

Build a runtime bundle when preparing a toolchain installation or changing runtime packaging. Provision [LLVM](llvm.md)
first, and follow the [artifact storage rules](build-artifacts.md) when sharing build outputs.

## Build a bundle

```text
cargo xtask runtime-artifact build --output <directory> [--target <triple>] [--profile <profile>]
```

The target defaults to the compiler host and the Cargo profile defaults to `release`. The command creates a
target-specific directory beneath the output root and transactionally replaces an existing catalog for that target. To
install the host runtime beside release binaries, run:

```text
cargo xtask runtime-artifact build --output target/release/lib/bray/runtime
```

The bundle records its source, dependency, compiler, profile, target, and packaging inputs. Repeated builds authenticate
the metadata and every component archive, then reuse a current runtime without rebuilding it.

## Verify packaging and native linking

```text
cargo xtask runtime-artifact smoke-test
```

This builds the host runtime, checks deterministic partitioning and exact component symbol boundaries, audits
synchronous linker maps for leaked dependencies, and runs a native-link smoke test.

## Understand component packaging

The catalog separates memory, string, character, host, scheduler, cancellation, event, and test-host adapters. Shared
language and runtime support occupies one common archive referenced by the semantic adapters. Construction deduplicates
archive members before producing deterministic component archives, so support code is not copied into every adapter.

Metadata assigns every runtime role and capability to exactly one component per product category, records exact
component dependencies, and names any platform-service roles explicitly overridden by a selected component. When
investigating unexpected runtime size, inspect the [selected runtime artifacts in compiler
profiles](profiling.md#selected-runtime-artifacts).

Return to [repository tasks](xtask.md) to choose another workflow.

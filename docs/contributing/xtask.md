# Repository tasks

Run repository automation from the Bray checkout you intend to work on with `cargo xtask`. Provision LLVM with `cargo
llvm`, which works before compiler crates can build. These are contributor tools, separate from Bray Tack's user-facing
commands.

## Format and check a change

| Task | Command |
| --- | --- |
| Apply formatting fixes and structural style checks | `cargo xtask style` |
| Check style without changing files | `cargo xtask style check` |
| Format maintained Bray sources | `cargo xtask format` |
| Check Bray formatting without changing files | `cargo xtask format check` |
| Run structural readiness audits | `cargo xtask readiness` |
| Exercise native products | `cargo xtask composition` |

Run the mutating `cargo xtask style` after changing Rust source. Style includes Bray formatting, which covers the
standard library, its tests, examples, readiness fixtures, and recovery corpus. See [coding
conventions](coding-conventions.md) for the enforced rules and [testing](testing.md) for choosing coverage. Default
readiness audits exclude native execution.

## Build required artifacts

Provision [LLVM](llvm.md), then build the installed [runtime](runtime-artifacts.md) and
[standard-library bundles](standard-library.md#build-a-bundle) beside release binaries:

```text
cargo llvm fetch
cargo xtask runtime-artifact build --output target/release/lib/bray/runtime
cargo xtask standard-library build --output target/release/lib/bray/standard-library
```

Read [artifact storage across checkouts](build-artifacts.md) before sharing Cargo output directories or running a
retained task executable from another checkout.

## Choose a specialized workflow

| I need to... | Guide |
| --- | --- |
| Run focused audits, native fixtures, or retained-product reruns | [Testing workflows](testing.md) |
| Test or verify standard-library changes | [Standard-library checks](standard-library.md#run-focused-native-tests) |
| Check runtime packaging and linking | [Runtime verification][runtime-checks] |
| Update generated platform declarations or an SDK baseline | [OS binding generation and probes](os-bindings.md) |
| Update Unicode character tables | [Unicode table generation](standard-library.md#regenerate-unicode-tables) |
| Regenerate and validate compiler-known definitions | [Compiler-known catalog](compiler-known-catalog.md) |
| Measure a change against a performance baseline | [Performance corpus](performance.md) |
| Diagnose where compilation time goes | [Compiler profiling](profiling.md) |

[runtime-checks]: runtime-artifacts.md#verify-packaging-and-native-linking

## Inspect a package interface

Render all sections as JSON, or select sections by the stable names in the unfiltered output:

```text
cargo xtask package-interface inspect <path>
cargo xtask package-interface inspect <path> --section <name> [--section <name>]...
```

Validate the complete interface and print its validated identity as JSON:

```text
cargo xtask package-interface validate <path>
```

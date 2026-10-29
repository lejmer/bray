# Repository tasks

Run repository automation from the root of the Bray checkout you intend to work on with `cargo xtask`. Provision LLVM with `cargo
llvm`, which works before compiler crates can build. These are contributor tools, separate from Bray Tack's user-facing
commands.

## Format and check a change

| Task                                               | Command                    |
|----------------------------------------------------|----------------------------|
| Apply formatting fixes and structural style checks | `cargo xtask style`        |
| Check style without changing files                 | `cargo xtask style check`  |
| Format maintained Bray sources                     | `cargo xtask format`       |
| Check Bray formatting without changing files       | `cargo xtask format check` |
| Run structural readiness audits                    | `cargo xtask readiness`    |
| Exercise native products                           | `cargo xtask composition`  |

Run the mutating `cargo xtask style` after changing Rust source. Style includes Bray formatting, which covers the
standard library, its tests, examples, readiness fixtures, and recovery corpus. See [coding
conventions](coding-conventions.md) for the enforced rules and [testing](testing.md) for choosing coverage. Default
readiness audits exclude native execution.

## Use optimized incremental tooling

Use `cargo xtask-incremental` to run the same tasks with Cargo's opt-in `tooling` profile:

```text
cargo build --profile tooling
cargo brayc-incremental --help
cargo bray-incremental --help
cargo xtask-incremental style
cargo xtask-incremental format
```

The profile inherits release settings, enables incremental compilation, and keeps optimization level 3 and 16 codegen
units. Its first build populates a separate cache; later edits can reuse compilation work. Keep the cache for the
checkout rather than cleaning between commands. See [artifact storage](build-artifacts.md) for checkout isolation and
shared final outputs.

The workspace defaults select `bray`, `brayc`, and `xtask` together. The named-binary aliases retain that selection, so
compiler, CLI, style, format, and runtime preparation commands share dependency features. Cargo compiles only the
requested binary and its dependencies when running an alias. Nested compiler builds use the same workspace defaults.
The focused `cargo llvm` bootstrap remains independent so it can provision LLVM before compiler tools build.
Use `cargo build --workspace` to build every workspace package, including other binaries such as `brayfmt` and `bray-lsp`.

Run these shared tool commands from the checkout root. Cargo selects the enclosing package when invoked inside a
member directory, which changes the feature set. Explicit `-p` selections are still useful for focused crate work, but
can produce another dependency variant. Use `cargo test --workspace` to run every workspace package, or
`cargo test -p <crate>` for focused Rust tests. Test-only and native runtime component features intentionally remain
separate from the developer tool graph.

Keep packages, features, and profiles consistent when comparing build times. The shared workflow removes feature-set
changes between tool commands. Actual source edits still rebuild their affected crates and consumers.

`cargo xtask` continues to use the release profile. Use it for release baselines. The experimental profile changes how
the Rust tools are built; it does not change the optimization selected for Bray programs by a task or by `brayc`.
Tasks that explicitly build compiler executables with `--release` continue to use release compiler executables, even
when the task runner itself uses the `tooling` profile.

In one warm sequence on a four-CPU Linux VM with Rust 1.97.0, Cargo preparation for compiler help, CLI help, style, and
format took 0.20-0.40 seconds per command without recompiling dependencies. Style and formatting work took additional
time. A controlled compiler entry-source edit rebuilt only `brayc`, while shared compiler libraries and the CLI stayed
fresh. These observations describe command reuse after cache population, not cold build times.

Cargo places the compiler executable in `<target-dir>/tooling/brayc` (`brayc.exe` on Windows), where `<target-dir>`
defaults to the checkout's `target` directory. When invoking that compiler directly, select compatible standard-library
inputs with `--standard-library-root <bundle-root>` and native runtime inputs with `--runtime-artifact <metadata>`. Building the
compiler under a different Cargo profile does not copy [toolchain bundles](#build-required-artifacts) beside it.

### Build measurements

These measurements compare `xtask` and `brayc` together on a four-CPU, 16 GiB x86-64 Linux VM with Rust 1.97.0.
Both profiles start with empty Cargo artifact caches and reuse downloaded dependencies and the LLVM installation.
These are single-build observations, with warm filesystem caches.

| Measurement                         | Release | Tooling |
|-------------------------------------|---------|---------|
| Initial profile population          | 38m 02s | 31m 20s |
| Unchanged build                     | <1s     | <1s     |
| Shared task argument helper edit    | 5m 04s  | 4m 17s  |
| Private compiler helper edit        | 31m 21s | 7m 29s  |
| Initial Cargo intermediate storage  | 1.5 GiB | 3.7 GiB |
| Peak RSS of largest build process   | 3.0 GiB | 3.0 GiB |

The additional profile retains its own artifacts beside the release cache. The initial release build of the compiler
library takes 25m 35s, including 20m 15s of code generation. Cargo overlaps dependent builds after metadata is ready,
so individual crate durations must not be added to obtain total elapsed time.

The task edit changes one error-message literal in `reject_trailing_argument`, a generic helper used across commands.
The paired builds use identical source. The incremental build reduces elapsed time by about 15% and aggregate CPU
time by about 5% for this edit; this is not evidence that every edit will receive the same benefit.

The compiler edit changes one catalog-initialization panic-message literal in `bray-compilation`, retaining the task
edit in both profiles. Its paired source hashes match. Incremental rebuilding reduces elapsed time by about 76% and
aggregate CPU by about 75%, including the dependent compiler tools.

Both compilers type-check three existing native fixtures and produce identical executable bytes across twelve builds
of the pattern fixture. Seven interleaved elapsed samples per compiler check and formatter case show wide variation.
A separate three-sample CPU comparison has mixed case-median changes from about -7% to +6%; these observations do not
establish a consistent runtime improvement or tight equivalence.

Use the profile for repeated local edits when the rebuild savings justify its additional cache. Keep release for
performance baselines. The initial cache fill remains substantial, and the measured benefit depends on the edit.

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

| I need to...                                                    | Guide                                                                     |
|-----------------------------------------------------------------|---------------------------------------------------------------------------|
| Run focused audits, native fixtures, or retained-product reruns | [Testing workflows](testing.md)                                           |
| Test or verify standard-library changes                         | [Standard-library checks](standard-library.md#run-focused-native-tests)   |
| Check runtime packaging and linking                             | [Runtime verification][runtime-checks]                                    |
| Update generated platform declarations or an SDK baseline       | [OS binding generation and probes](os-bindings.md)                        |
| Update Unicode character tables                                 | [Unicode table generation](standard-library.md#regenerate-unicode-tables) |
| Regenerate and validate compiler-known definitions              | [Compiler-known catalog](compiler-known-catalog.md)                       |
| Measure a change against a performance baseline                 | [Performance corpus](performance.md)                                      |
| Diagnose where compilation time goes                            | [Compiler profiling](profiling.md)                                        |

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

## Compiler test tiers

Run `cargo xtask test compiler` for the budgeted fast compiler suite. Select `--tier native`, `--tier proof`,
`--tier exhaustive`, or `--tier all` for broader coverage. Add `--shard index/count` for a deterministic portion of a tier.
See [Compiler test tiers](testing.md#compiler-test-tiers) for budgets, reports, fixture isolation, and direct Cargo commands.

# Repository Tasks

Bray keeps repository-specific development automation in the `xtask` crate. Run most commands from the repository root
with `cargo xtask`. LLVM provisioning uses the dependency-light `cargo llvm` command because it must work before
compiler crates can be built. These commands are contributor and release-engineering tools rather than user-facing Bray
Tack commands.

Materially long workflows print bounded phase starts and completions to standard error. Repeated work reports a stable
item count, and child compiler progress remains visible while machine-readable command output stays on standard output.

## Style

Apply deterministic formatting fixes and run the repository's structural style checks:

```text
cargo xtask style
```

Run the same checks without changing files:

```text
cargo xtask style check
```

The mutating command is required after changing Rust source. See [Coding conventions](coding-conventions.md) for the enforced
rules.

## Bray source formatting

Format maintained Bray source in the standard library, its tests, examples, readiness fixtures, and recovery corpus:

```text
cargo xtask format
```

Check the same source set without modifying it:

```text
cargo xtask format check
```

The general `style` commands include the corresponding Bray source formatting operation.

## Compiler-known catalog

Regenerate the Rust source and digest derived from the checked-in compiler-known catalog definitions:

```text
cargo xtask compiler-known generate
```

Check whether the generated files are current without changing them:

```text
cargo xtask compiler-known generate --check
```

Check the generated files and semantically validate the complete compiler-known catalog:

```text
cargo xtask compiler-known check
```

## LLVM toolchain

Download and validate the pinned LLVM toolchain for the active Rust host:

```text
cargo llvm fetch
```

Validate the provisioned toolchain, or validate an explicitly managed installation:

```text
cargo llvm validate
cargo llvm validate --root <directory>
```

Print the supported Rust host identity selected by the LLVM manifest:

```text
cargo llvm host
```

See [LLVM toolchain](llvm.md) for provisioning policy and supported hosts.

## Package interfaces

Render a package interface as JSON. Without section filters, inspection includes every section:

```text
cargo xtask package-interface inspect <path>
cargo xtask package-interface inspect <path> --section <name> [--section <name>]...
```

Validate the complete package interface and render its validated identity as JSON:

```text
cargo xtask package-interface validate <path>
```

Section names are the stable machine-readable names printed by the unfiltered inspection output.

## Readiness audits

Run all structural readiness audits:

```text
cargo xtask readiness
```

Run one focused audit:

```text
cargo xtask readiness semantic
cargo xtask readiness diagnostics
cargo xtask readiness lowering
cargo xtask readiness memory
cargo xtask readiness codegen
cargo xtask readiness emission
cargo xtask readiness linker
```

Compile, inspect, link, and execute the native readiness fixtures separately:

```text
cargo xtask readiness native-execution
```

The default readiness command does not include native execution. See [Testing](testing.md) for the coverage fixtures and
audit contracts.

## Runtime artifacts

Build the reference runtime artifact catalog and its metadata into the toolchain runtime bundle:

```text
cargo xtask runtime-artifact build --output <directory> [--target <triple>] [--profile <profile>]
```

The command creates a target-specific directory beneath the output root and transactionally replaces an existing catalog
for that target. The catalog separates memory, string, character, host, scheduler, cancellation, event, and test-host
adapters. Shared language and runtime support occupies one common archive referenced by the semantic adapters. Artifact
construction extracts and deduplicates archive members before producing deterministic component archives, so support
code is not copied into every adapter. The metadata assigns every runtime role and capability to exactly one component
for each product category, records exact component dependencies, and names any platform-service roles explicitly
overridden by a selected component. The target defaults to the compiler host and the Cargo profile defaults to
`release`. For example, produce the installed host runtime layout expected beside release binaries with:

```text
cargo xtask runtime-artifact build --output target/release/lib/bray/runtime
```

The output records the exact source, dependency, compiler, profile, target, and packaging inputs that produced it.
Repeated builds authenticate the metadata and every component archive, then reuse a current runtime without rebuilding
it.

Run the host runtime build, deterministic partitioning, exact component symbol-boundary audit, synchronous linker-map
leakage audit, and native-link smoke test with:

```text
cargo xtask runtime-artifact smoke-test
```

## Standard library

Build the standard-library bundle from the checked-in `standard-library/` workspace:

```text
cargo xtask standard-library build --output <directory>
```

Use `--source <directory>` to select another standard-library workspace. Without `--target`, the bundle contains
artifacts for every supported native target. Use `--target <triple>` to build one target. A successful rebuild
transactionally replaces an existing bundle at the output path.

The bundle records the exact source, dependency, compiler, and packaging inputs that produced it. Repeated builds
authenticate the manifest and every published artifact, then reuse a current bundle that contains all requested
targets.

To produce the installed layout expected by Bray Tack beside release binaries in `target/release/`, run:

```text
cargo xtask standard-library build --output target/release/lib/bray/standard-library
```

Run native standard-library integration tests with:

```text
cargo xtask standard-library test
```

Native compiler workflows build and use optimized `bray` and `brayc` tools. To retain one bounded compiler trace for
every build invocation under a distinct phase directory, run:

```text
cargo xtask standard-library test --profile-output profiles/standard-library-native
```

The profiling form preserves the same test contract and streams Bray build progress while it runs. Inspect the resulting
reports
with the profile commands described in [Compiler profiling](profiling.md).

The target bundle publishes separate core, standard-stream, filesystem, and process platform archives. Each archive
records its exact native platform capabilities and direct native dependencies in the manifest. The build fails if that
inventory is incomplete or overlaps another archive.

Standard input, standard output, and standard error remain separate object leaves inside the standard-stream archive.

Build, execute, validate, and measure the compiler performance corpus with:

```text
cargo xtask performance --output <directory>
```

The command is intentionally outside ordinary unit tests. It builds one host runtime and standard-library toolchain,
performs one matched packaged-application compilation and one matched source-library compilation for Bray, Rust, and
C++, and compiles every matched workload with all three languages. Workload compilation records the complete compiler
process duration including startup and teardown, then records compiler-reported work without compiler process startup or
teardown. The command performs warmups followed by seven measured executions, validates stable output, and writes bounded
self-contained `candidate.html` and structured `candidate.json` reports. Each workload carries a fixed expected-output
contract.

The command writes phase and workload progress to standard error while keeping the `candidate.json` path as its only
standard output line.

The first run prepares a target-specific runtime and standard-library toolchain under Cargo's target directory. Later
runs reuse that toolchain when the compiler, Cargo lockfile, runtime, platform providers, standard library, and selected
target are unchanged. Changing report options such as `--warmup`, `--samples`, `--workload`, or `--output` does not
rebuild it.

Agreement between repeated samples alone is not considered validation. Use `--warmup`, `--samples`, or `--target` to
make an explicit equivalent run configuration.

The two compiler comparisons invoke the optimized Bray compiler executable, `rustc`, and `clang++` as external
processes. The application lane compiles source-equivalent applications against each language's packaged library and
runtime. The library lane compiles source-equivalent library authority from source. The report records source units and
bytes, package and module inputs, exact compiler and linker arguments, packaged-library and runtime reuse evidence,
toolchains, source digests, and elapsed time. Measured compiler invocations do not generate profiles or linker maps.
Separate untimed evidence invocations produce those records from the same source and release policy. Cross-language
syntax may differ in verbosity, so source byte counts remain comparable only while the largest source is no more than
eight times the smallest. A row with incomplete or different authority is explicitly non-comparable, records exact
reasons, and cannot publish a winner.

Every workload also builds maintained Rust and C++ runtime peers directly through `rustc` and `clang++`. The report
records each exact toolchain, optimization configuration, source digest, process duration, language-controlled duration,
artifact size, sections, and dependencies. These preparation builds are not ranked as compiler comparisons. Process and
language-controlled rounds rotate their starting language independently so Bray, Rust, and C++ do not receive a fixed
warm-cache or scheduling advantage. Every execution must produce the same validated output digest and sideeffect
contract. The HTML report states the shared semantic contract for each row. Missing peer sources, failed peer builds,
mismatched output, or incomplete peer reports fail the run instead of producing an incomplete comparison.

Workloads that would finish too close to the host timer resolution repeat inside one controlled interval. Before warmups
and measured samples, the command times three seed intervals for Bray, Rust, and C++. It chooses a shared repetition
count from the fastest language median so every implementation targets at least 100 ms. The report retains the seed
count, all calibration intervals, the target interval, the selected repetition count, the timer resolution, the raw
measured intervals, and the adjusted picosecond duration for one workload execution. The timed Bray, Rust, and C++
artifacts use that same selected count. Production executable size and process duration continue to measure ordinary
single-execution artifacts.

All three production executables use static application and language runtimes. On Windows this means the static MSVC
runtime for Bray, Rust, and C++. Target operating-system libraries may remain dynamic. The report records the policy and
exact compiler flags, then validates the produced dependency lists to reject application-runtime DLLs. Linux peers embed
their language runtimes while using target system libraries. Mach-O comparison is rejected until the C++ peer can
provide the same runtime model. Rust and C++ memory-work observations remain unavailable until equally attributed
measurement support exists for all three languages.

The corpus covers a minimal executable plus scale-sensitive byte growth, borrowed and explicitly owned text pipelines,
formatting, stream output, asynchronous execution, filesystem metadata, file output, process context, and clock access.
The text pipeline covers merged literals, UTF-8 substrings, long and repeated input, imported parsing, comparison,
hashing, and raw and escaped formatting. Representative stream-only and file-only workloads also enforce required and
forbidden linker-map provenance so resource capability boundaries remain independently retainable. Add a workload only
when it has a stable identity, deterministic output, an explicit scale and unit, and exercises a distinct implemented
cost boundary. Prefer increasing the scale of a focused workload over combining unrelated operations in one source file.

Use repeated `--workload <identity>` options for focused development runs. The selected workload set participates in the
corpus digest, so a focused report can only compare with the same focused selection.

Reports keep executable and relocatable-object sizes, per-section sizes, static linker-map provenance, dynamic library
dependencies, the complete compiler profile, and robust median and MAD execution statistics. The HTML report presents
duration in milliseconds and artifact size in KiB, with exact nanoseconds available as hover text. It reports process
wall time separately from the measured Bray root execution interval, and bases throughput on the Bray interval. Exact
byte counts appear beside rounded KiB values. Allocation, copying, and platform-call observations are tagged as measured
or unavailable. Never replace a missing observation hook with an inferred count. Section, dynamic-library, and
retained-input collections have fixed entry limits and disclose omitted counts rather than allowing reports to grow
without bound.

Every workload also runs a timing-only release artifact that records the root execution interval without memory
observation calls. Workloads with storage contracts run another untimed artifact that records successful generated
allocation and bulk-transfer events. Keeping these executions separate prevents observation file writes from affecting
the Bray interval. The incremental byte workloads use fixed 64-byte and 4096-byte storage contracts to prove logarithmic
allocation growth and linear allocation and copy work. Production workload artifacts retain no observation callbacks, so
process timing and artifact measurements remain those of the ordinary release build.

Compare an equivalent baseline and candidate with:

```text
cargo xtask performance --output <directory> --baseline <candidate.json>
```

Comparison first requires the same schema, corpus digest, target, host, compiler version, LLVM version, warmup count,
sample count, scale, units, and validated output. Runtime changes inside three combined median absolute deviations are
reported as indeterminate, not as regressions. Artifact and section sizes are deterministic and are attributed directly.
The comparison writes both `comparison.html` and `comparison.json`. The structured comparison attributes compiler
operation and metric changes, artifact and section sizes, linker-map size, added and removed static inputs and dynamic
libraries, allocation/copy observations, and selected platform-operation observations by workload. Retaining the machine
reports is the supported way to establish a baseline.

Regenerate the checked-in target-specific operating-system bindings and native probes from the pinned SDK description:

```text
cargo xtask standard-library os-bindings generate
```

The generator reads `standard-library/targets/os-bindings.json`, which lists focused sources beneath
`standard-library/targets/os-bindings`. Shared POSIX and operating-system files hold common declarations, while one file
per exact target names its pinned SDK authority, revision, header set, and target-specific declarations. The generator writes Bray declarations beneath
`standard-library/std/src/os/generated` and matching C probes beneath `standard-library/targets/probes`. Every generated
file records the complete model's SHA-256 digest. Production compilation consumes only the generated Bray declarations
and does not inspect ambient host headers.

Check that the generated bindings, probes, and recorded input digest are current without modifying them:

```text
cargo xtask standard-library os-bindings generate --check
```

Compile and run the authoritative native probe for the current host or an explicitly named matching target with:

```text
cargo xtask standard-library os-bindings probe [--target <triple>] --sdk-root <path> [--compiler-root <path>]
```

The SDK root is the exact Linux sysroot, macOS SDK directory, or Windows Kits root named by the model. The probe removes
ambient SDK include and library paths, selects that root explicitly, and verifies the recorded SDK revision before it
checks emitted constants, scalar mappings, layouts, field offsets, flexible tails, bitfield access, callable signatures,
linked symbols, thread-local storage, and dynamic lookup. A probe runs only on its exact target host. Standard-library
verification checks deterministic generated bytes and source-checks the generated Bray declarations for every supported
native target.

Windows probes also require `--compiler-root` naming the exact Visual C++ tools revision recorded by the model. This
provides the UCRT compiler headers and libraries without restoring ambient Visual Studio discovery.

SDK revisions are compatibility baselines, not rolling references to the newest release. Select a baseline from the
platform owner's published stable releases, record the exact authority and revision in the model, and retain it until a
deliberate compatibility review adopts a replacement. Linux baselines must identify a coherent kernel and libc sysroot.
The Linux baseline is the Debian 13 stable sysroot, whose release inventory pairs the Linux 6.12 LTS series with glibc
2.41 for both supported architectures. The choice is recorded against the
[Debian 13 release](https://www.debian.org/News/2025/20250809) and the active long-term status published by
[kernel.org](https://www.kernel.org/releases.html). Windows and macOS choices are recorded against the corresponding
[Microsoft SDK archive](https://learn.microsoft.com/windows/apps/windows-sdk/downloads) and
[Apple Xcode SDK table](https://developer.apple.com/xcode/system-requirements).

Verify standard-library conformance and reproducible bundle production with:

```text
cargo xtask standard-library verify
```

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

This builds the host runtime, checks deterministic partitioning and exact component symbol boundaries, and exercises
native adapters and ordinary emitted products. Synchronous executable cases cover direct entry, output, panic, returned
errors, product and thread statics, numeric arithmetic at all four IEEE widths, and cleanup with allocation denied through process shutdown. Development and release
cases exercise source and native standard-library reuse.

The synchronous product audit checks the ordinary emitter's selected runtime units and their inclusion reasons against
authenticated native units from the Bray implementation artifacts, then checks the actual linker map. An opaque Rust
archive fails even when its member and symbol names do not identify Rust. Hosted products have a `.runtime-units`
selection receipt and maps have a `.runtime-members` receipt listing retained Bray units. Product build time, process
launch time and executable bytes are observations; the direct product must remain smaller than the hosted synchronous control.

## Understand component packaging

The catalog separates the Bray bootstrap and observation implementations from the remaining host, callback, scheduler,
cancellation, event and test-host adapters. Ordinary synchronous products select Bray host, entry, reporting and cleanup
units through normal native demand resolution. Common host shutdown releases report and host storage. Products with
execution demand, including nonempty test hosts, explicitly retire their selected execution services before common
host shutdown.

Generic cancellation observation and cleanup shielding use the scoped entry provider. An ordinary synchronous entry has
no selected execution provider. Async and test providers retain and restore their control callbacks with the existing
exact-thread entry scope. The remaining Rust archives serve unfinished execution and test consumers, which stay outside the proved synchronous
host closure. Selected-precision real and complex arithmetic uses the
[numeric runtime implementation](../wip/bray-numeric-runtime-analysis.md) and shares the synchronous provenance audit.
Native thread operations that retain task-event or
atomic-wait services also retain their execution requirements; an ordinary synchronous entry does not remove reachable
service demand.

Native archives with a complete export inventory are selected lazily by the common native resolver. Unknown opaque
inputs, including compiler archives containing unsupported bitcode, retain their conservative mandatory behavior.
Precise strong definitions take precedence over redundant archives. Weak definitions remain exact when representable,
while an archive can still supply a stronger definition. LLVM spin-loop intrinsics preserve precise summaries on the
six supported targets without relaxing the treatment of arbitrary inline assembly or unknown linker directives.

Metadata assigns every runtime role and capability to exactly one component per product category, records exact
component dependencies, and names any platform-service roles explicitly overridden by a selected component. When
investigating unexpected runtime size, inspect the [selected runtime artifacts in compiler
profiles](profiling.md#selected-runtime-artifacts).

## Synchronous footprint checkpoint

The BRA-608 Linux x86-64 checkpoint compares matched release consumers against `96eca024`. Both use LLVM 22.1.8,
the same source programs and native system libraries. Executable bytes include whatever metadata each producer emits;
the separate LLVM size columns attribute loaded code/read-only data and writable data/BSS. The hosted-empty program
calls `std.thread.is_main()`; the direct control has an empty ordinary Bray entry.

| Consumer | Executable bytes, before → after | Code/read-only bytes, before → after | Data + BSS bytes, before → after | Full consumer build seconds, before → after |
| --- | ---: | ---: | ---: | ---: |
| Direct control | 6,053,024 → 5,768 | 1,725,276 → 1,185 | 26,944 → 2,257 | unrecorded → 0.46 |
| Hosted empty | 6,053,672 → 486,736 | 1,725,668 → 418,407 | 26,528 → 3,224 | 0.75 → 0.48 |
| Output | 6,118,872 → 552,072 | 1,787,156 → 479,932 | 26,460 → 3,140 | 0.87 → 0.52 |
| Panic | 6,053,520 → 486,576 | 1,725,644 → 418,399 | 26,560 → 3,224 | 0.77 → 0.49 |
| Product static | 6,122,352 → 905,648 | 1,788,428 → 816,060 | 25,216 → 2,888 | 0.93 → 0.55 |

Full consumer build time includes source loading, checking, generation and linking with already delivered libraries;
it excludes preparing the compiler and runtime bundle. Wall times are machine/load observations, not budgets.

Twenty interleaved, uninstrumented launches of each retained before/after executable give these median process
durations. They include process creation, entry, shutdown and reaping; they do not isolate runtime initialization.

| Consumer | Process milliseconds, before → after |
| --- | ---: |
| Direct control | 0.876 → 0.693 |
| Hosted empty | 0.939 → 0.695 |
| Output | 0.944 → 0.735 |
| Panic | 0.903 → 0.811 |
| Product static | 1.019 → 0.892 |

A Linux/glibc process allocator interposer observes the following traffic through shutdown. Requested bytes are
cumulative allocation requests; reserved values use `malloc_usable_size`, rather than OS virtual-memory reservation.
Loader-internal allocation and bypasses of the intercepted entry points are outside this measurement.

| Consumer | Allocation count, before → after | Requested bytes, before → after | Peak usable bytes, before → after | Live usable bytes at exit, before → after |
| --- | ---: | ---: | ---: | ---: |
| Direct control | 13 → 0 | 6,848 → 0 | 6,840 → 0 | 376 → 0 |
| Hosted empty | 13 → 7 | 6,848 → 2,016 | 6,840 → 1,960 | 376 → 0 |
| Output | 13 → 7 | 6,848 → 2,016 | 6,840 → 1,960 | 376 → 0 |
| Panic | 14 → 8 | 6,864 → 2,032 | 6,840 → 1,984 | 376 → 0 |
| Product static | 13 → 12 | 7,136 → 6,336 | 7,128 → 6,320 | 376 → 0 |

The host owns three native TLS keys: identity, cleanup and the scoped execution-control projection. A static atomic
reader count protects control-key lookups by unattached callers; shutdown returns pending while a lookup is active.
Formation, rollback, retry and concurrent retirement tests account for every accepted key. Allocation-denial tests
separately require teardown to attempt no allocation and release all tracked runtime allocations. Final footprint and
compilation acceptance remains with [BRA-541](https://linear.app/bray-lang/issue/BRA-541) and
[BRA-542](https://linear.app/bray-lang/issue/BRA-542).

Return to [repository tasks](xtask.md) to choose another workflow.

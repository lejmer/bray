# Performance measurement contracts

Use this reference when interpreting a comparison or changing the corpus runner. For commands and baseline collection,
start with [measure performance](performance.md).

## Compiler comparisons

The two compiler comparisons invoke the optimized Bray compiler executable, `rustc`, and `clang++` as external
processes. Workload compilation records full process duration, including startup and teardown, separately from
compiler-reported work.

The application lane compiles source-equivalent applications against each language's packaged library and runtime. The
library lane compiles equivalent library code from source. The report records source units and bytes, package and module
inputs, exact compiler and linker arguments, packaged-library and runtime reuse evidence, toolchains, source digests,
and elapsed time.

Measured compiler invocations do not generate profiles or linker maps. Separate untimed evidence invocations produce
those records from the same source and release policy.

Cross-language syntax may differ in verbosity, so source byte counts remain comparable only while the largest source is
no more than eight times the smallest. A row with incomplete or different compilation inputs is explicitly
non-comparable, records exact reasons, and cannot publish a winner.

## Runtime peers and sample ordering

Every workload also builds maintained Rust and C++ runtime peers directly through `rustc` and `clang++`. The report
records each exact toolchain, optimization configuration, source digest, process duration, language-controlled duration,
artifact size, sections, and dependencies. These preparation builds are not ranked as compiler comparisons.

Process and language-controlled rounds rotate their starting language independently so Bray, Rust, and C++ do not
receive a fixed warm-cache or scheduling advantage. Every execution must produce the same validated output digest and
side-effect contract. The HTML report states the shared semantic contract for each row. Missing peer sources, failed
peer builds, mismatched output, or incomplete peer reports fail the run instead of producing an incomplete comparison.

## Calibrate short workloads

Workloads that would finish too close to the host timer resolution repeat inside one controlled interval. Before warmups
and measured samples, the command times three seed intervals for Bray, Rust, and C++. It chooses a shared repetition
count from the fastest language median so every implementation targets at least 100 ms. The report retains the seed
count, all calibration intervals, the target interval, the selected repetition count, the timer resolution, the raw
measured intervals, and the adjusted picosecond duration for one workload execution. The timed Bray, Rust, and C++
artifacts use that same selected count. Production executable size and process duration continue to measure ordinary
single-execution artifacts.

## Platform runtime policy

Windows production executables use the system MSVC and UCRT DLLs. Bray runtime archives and C dependencies explicitly
disable static CRT selection, the standard-library platform bindings use UCRT import libraries, and Rust and C++ peers
select the same dynamic policy. This avoids relying on CRT startup and thread-local initialization that Bray's private
executable entry does not run. The report records the policy and exact compiler flags, then validates PE imports and
linker-map inputs to reject retained static CRT archives. Linux peers embed their language runtimes while using target
system libraries. Mach-O comparison is rejected until the C++ peer can provide the same runtime model.

Rust and C++ memory-work observations remain unavailable until equally attributed measurement support exists for all
three languages.

## Report measurements

Reports keep executable and relocatable-object sizes, per-section sizes, logical optimization-partition provenance,
physical static linker-map inputs, dynamic library dependencies, the complete compiler profile, and robust median and
median absolute deviation execution statistics. Logical provenance remains stable when cross-module optimization emits a
retained provider definition from a product module. It combines optimization-partition identities, platform-provider
families derived from exact retained service symbols, and exact archive identities for definitions that remain in their
input archive. Physical inputs continue to describe the files observed by the linker.

The HTML report presents duration in milliseconds and artifact size in KiB, with exact nanoseconds available as hover
text. It reports process wall time separately from the measured Bray root execution interval, and bases throughput on
the Bray interval. Exact byte counts appear beside rounded KiB values.

Allocation, copying, and platform-call observations are tagged as measured or unavailable. Never replace a missing
observation hook with an inferred count.

Section, dynamic-library, and retained-input collections have fixed entry limits and disclose omitted counts rather than
allowing reports to grow without bound.

## Interpreting a smaller demand graph with a larger executable

Compare a dead dependency probe with a minimal host and a live dependency before treating its binary size change as
growth in generated application code. Linker maps can reveal a different provider for the same symbol: removing an
application definition may cause the linker to select a precompiled archive member with more transitive dependencies.
Retaining a dead call solely to get a smaller executable would hide that runtime cost. Attribute it and decide from a
representative corpus.

An initial Windows x86-64 probe showed this pattern. After pruning a call in a false branch, its demand graph shrank
but its executable grew from 629,760 B to 704,000 B. The pruned program pulled in a native runtime bootstrap archive
member that also defined a product host. The compiler assigned that host to the first codegen unit, even when the unit
defined no static storage. Assigning the host to a unit with a static definition lets ordinary executables omit the
unused host while retaining it for products with statics.

Five warm direct `brayc --release` samples per row alternated the baseline and corrected compilers against the same
rebuilt runtime and standard-library bundles, source and manifests:

| Program | Baseline median / size | Corrected median / size |
| --- | ---: | ---: |
| Minimal synchronous host | 422.20 ms / 266,752 B | 423.33 ms / 266,752 B |
| Live `std.io.print` | 751.92 ms / 300,032 B | 751.65 ms / 300,032 B |
| False branch containing `std.io.print` | 849.08 ms / 273,920 B | 438.86 ms / 266,752 B |

All three programs preserve their exit status and output. The corrected artifact preserves the compile time gain from
pruning and reduces the pruned executable below the baseline size.

## Timing and memory observation

Every workload also runs a timing-only release artifact that records the root execution interval without memory
observation calls. Workloads with storage contracts run another untimed artifact that records successful generated
allocation and bulk-transfer events. Keeping these executions separate prevents observation file writes from affecting
the Bray interval. The incremental byte workloads use fixed 64-byte and 4096-byte storage contracts to prove logarithmic
allocation growth and linear allocation and copy work. Production workload artifacts retain no observation callbacks, so
process timing and artifact measurements remain those of the ordinary release build.

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

## Dependent-build comparisons

The corpus application lane consumes already published libraries. It does not measure the transition from producing a
library to starting its consumer. The library lane measures a separate source build. Do not combine their durations to
claim a complete-workspace duration or use one condition as the baseline for the other.

For a dependent-build regression, retain two separate conditions using the existing compiler and project commands:

- **Published consumer.** Invoke `brayc build` with the recorded consumer arguments and unchanged, already published
  dependency artifacts. Measure the compiler process, including its linker and publication work.
- **Immediate pipeline.** Invoke `bray build` for the same workspace and product. Keep dependency production and the
  immediate importing build inside one measured workspace interval. Record library, consumer and workspace durations
  separately. When isolating a producer change, keep the consumer executable fixed and invoke it immediately after each
  producer, with no intervening inspection or cache preparation.

Freeze source, manifests, compiler executable digests and build configuration, target, effective options, workers,
installed runtime/library artifacts and output/cache paths before comparing. Preserve exact compiler arguments and
artifact digests beside the samples. An artifact match establishes equal inputs or output bytes, but does not establish
matched host load or cache conditions.

Run a warmup for every condition, then at least five unprofiled samples. Rotate baseline and candidate invocation order
within each condition rather than collecting all baseline samples before all candidate samples. Preserve every sample,
including warmups and outliers, with its condition, compiler identity, position in the round and elapsed duration. Do not
rewrite dependencies between published-consumer samples. Keep artifact checks and profiling outside measured intervals,
and keep all producer work inside the pipeline interval.

Record concurrent build/editor activity and cache preparation for each batch. A background build, compiler rebuild or
artifact publication can change the conditions of later samples even when every compiler argument is identical. If host
activity changes during a round, retain and label that round, then collect another complete matched batch. Do not remove
individual slow samples, insert waits, restore redundant artifact reads or slow a producer to make a comparison pass.

Use separate summary-profile invocations and `bray profile compare` to examine import work, native demand, cache outcomes,
staging and linking. Compare counts as well as durations, and do not add inclusive operation times. A difference between
published-consumer and pipeline results supports investigating the producer handoff. A slowdown shared by unchanged
consumer executables across both conditions supports investigating host/cache drift. Neither observation alone identifies
a particular filesystem filter, background service or clock policy.

Report the matched medians for library, consumer and complete workspace against the protected implementation baseline.
A consumer improvement does not excuse a library or workspace regression. Keep the historical samples, the new matched
samples and any observed host disturbance distinct when recording an attribution.

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

## Timing and memory observation

Every workload also runs a timing-only release artifact that records the root execution interval without memory
observation calls. Workloads with storage contracts run another untimed artifact that records successful generated
allocation and bulk-transfer events. Keeping these executions separate prevents observation file writes from affecting
the Bray interval. The incremental byte workloads use fixed 64-byte and 4096-byte storage contracts to prove logarithmic
allocation growth and linear allocation and copy work. Production workload artifacts retain no observation callbacks, so
process timing and artifact measurements remain those of the ordinary release build.

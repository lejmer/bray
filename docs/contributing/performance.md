# Measure performance

Use the performance corpus to compare compiler time, executable size, and runtime work across Bray, Rust, and C++.
For a slow compiler invocation, start with [compiler profiling](profiling.md) instead.

## Prepare and run

Run on a supported native host with [LLVM](llvm.md), `rustc`, `clang++`, and a native linker available. The runner
executes its products locally. `--target <triple>` must match the current host. See the
[platform runtime policy](performance-reference.md#platform-runtime-policy) for comparison restrictions.

```text
cargo xtask performance --output profiles/performance-baseline
```

This is separate from ordinary unit tests. It builds the host runtime and standard-library toolchain, compiles matched
application and library comparisons, and builds and runs each workload in all three languages. By default it performs
two warmups and seven measured executions, validates output, and writes `candidate.html` and `candidate.json` under the
output directory. Open the HTML report to inspect results and retain the JSON report for later comparisons.

Progress goes to standard error. The only standard output line is the `candidate.json` path.

The first run prepares a target-specific toolchain under Cargo's target directory. Later runs reuse it when the compiler,
Cargo lockfile, runtime, temporal provider, standard library, and target are unchanged. Changing report options does not
rebuild that toolchain. Follow the [artifact storage rules](build-artifacts.md) when working across checkouts.

## Focus a run

Use repeated `--workload <identity>` options to select workloads from the
[corpus inventory](../../xtask/src/performance/corpus.rs):

```text
cargo xtask performance --output profiles/performance-focused --workload <identity> --warmup 2 --samples 7
```

Both counts must be positive. Keep workload selection, warmups, samples, and target identical between baseline and
candidate. The selected workloads participate in the corpus digest, so a focused report only compares with the same
selection. Every workload has a fixed expected-output contract. Repeated samples agreeing with each other is not enough
to validate a run.

## Compare a change

Keep baseline and candidate in separate directories so the new report does not overwrite the baseline:

```text
cargo xtask performance --output profiles/performance-candidate --baseline profiles/performance-baseline/candidate.json
```

Comparison first requires the same schema, corpus digest, target, host, compiler version, LLVM version, warmup count,
sample count, scale, units, and validated output. Runtime changes inside three combined median absolute deviations are
reported as indeterminate, not as regressions. Artifact and section sizes are deterministic and are attributed directly.
The comparison writes both `comparison.html` and `comparison.json`. The structured comparison attributes compiler
operation and metric changes, artifact and section sizes, linker-map size, added and removed static inputs and dynamic
libraries, allocation/copy observations, and selected platform-operation observations by workload. Retaining the machine
reports is the supported way to establish a baseline.

For the meaning and limits of measurements, see [compiler comparisons](performance-reference.md#compiler-comparisons),
[sample ordering](performance-reference.md#runtime-peers-and-sample-ordering),
[short-workload calibration](performance-reference.md#calibrate-short-workloads), and
[report measurements](performance-reference.md#report-measurements).

## Extend the corpus

Add a workload only when it has a stable identity, deterministic output, an explicit scale and unit, and exercises a
distinct implemented cost boundary. Prefer increasing the scale of a focused workload over combining unrelated
operations in one source file. Check the [corpus inventory](../../xtask/src/performance/corpus.rs) for existing coverage.

Representative stream-only and file-only workloads also enforce required and forbidden logical optimization provenance,
so resource capability boundaries remain independently retainable. See the
[report reference](performance-reference.md#report-measurements) when changing these expectations.

Keep [timing and memory observations](performance-reference.md#timing-and-memory-observation) separate, and preserve
cross-language validation and comparability rules when adding or changing a workload.

Return to [repository tasks](xtask.md) to choose another workflow.

# Measure performance

Use the performance corpus to compare compiler time, executable size, and runtime work across Bray, Rust, and C++. For a
slow compiler invocation, start with [compiler profiling](profiling.md) instead.

## Prepare and run

Run on a supported native host with [LLVM](llvm.md), `rustc`, `clang++`, and a native linker available. The runner
executes its products locally. `--target <triple>` must match the current host. See the [platform runtime
policy](performance-reference.md#platform-runtime-policy) for comparison restrictions.

```text
cargo xtask performance --output profiles/performance-baseline
```

This is separate from ordinary unit tests. It builds the host runtime and standard-library toolchain, compiles matched
application and library comparisons, and builds and runs each workload in all three languages. By default it performs
two warmups and seven measured executions, validates output, and writes `candidate.html` and `candidate.json` under the
output directory. Open the HTML report to inspect results and retain the JSON report for later comparisons.

Progress goes to standard error. The only standard output line is the `candidate.json` path.

The first run prepares a target-specific toolchain under Cargo's target directory. Later runs reuse it when the
compiler producer, Cargo lockfile, runtime, temporal provider, standard library, target, and resolved native tools are
unchanged. Preparation identities fingerprint selected tool contents, including replacements at the same path. Changing report
options does not rebuild that toolchain. Follow the [artifact storage rules](build-artifacts.md) when working across
checkouts.

## Focus a run

Use repeated `--workload <identity>` options to select workloads from the [corpus
inventory](../../xtask/src/performance/corpus.rs):

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

Comparison first requires the same schema, corpus digest, target, host and host class, worker budget, compiler version,
LLVM and Rust versions, runtime and standard-library identities, warmup count, sample count, scale, units, and validated
output. Runtime changes inside three combined median absolute deviations are
reported as indeterminate, not as regressions. Artifact and section sizes are deterministic and are attributed directly.
These comparison assessments describe observations. Use the regression gate below to decide whether a change has a
confirmed merge-base regression.
The comparison writes both `comparison.html` and `comparison.json`. The structured comparison attributes compiler
operation and metric changes, artifact and section sizes, linker-map size, added and removed static inputs and dynamic
libraries, allocation/copy observations, and selected platform-operation observations by workload. Retaining the machine
reports is the supported way to establish a baseline.

The compiler timing rows record individual unprofiled invocations, with separate profiled evidence invocations. The
`--warmup` and `--samples` options control workload execution samples, not repeated compiler builds. Diagnose a
producer/consumer timing change with the [dependent-build comparison procedure](performance-reference.md#dependent-build-comparisons)
before attributing it to compiler code.

## Gate a pull request

Use `cargo xtask performance gate` to evaluate retained reports from the existing runner. It does not run benchmarks.
Commit the candidate before measuring. Resolve the pull request's current merge base with `git merge-base origin/develop
HEAD`, measure that exact clean commit, and measure the clean candidate. Give both runs the same `--host-class <label>`
describing the controlled machine class and scheduling environment. Retain the source-identified reports, toolchain, and
measurement artifacts. Refresh the base ref before gating. A stale global baseline cannot replace the computed merge base.

Gating starts with merge bases that contain the schema-2 measurement producer. A merge base predating that producer,
including this gate's introduction, cannot supply an accepted baseline; retain its absolute reports and assess the change
manually. After the producer lands, measure both revisions with their own checkout's runner. Do not backfill missing
identity fields, relabel an older report, or run the candidate runner against an older checkout: the runner embeds its
compiler, so that would misidentify the measured implementation.

For compiler comparisons, hold the imported standard library and runtime fixed. An ordinary measurement prepares these
inputs automatically. Retain complete copies of its standard-library bundle and runtime package, using the paths recorded
in the report's compiler invocations, outside any cache that subsequent runs may rebuild. Then give both checkout-local
runners the same absolute `--standard-library <bundle-directory>` and `--runtime <metadata.brayrt>` paths:

```text
cargo xtask performance --output profiles/base --host-class controlled-host --standard-library /retained/standard-library --runtime /retained/runtime/bray-runtime.brayrt
```

Run the equivalent command in the candidate checkout with a separate output directory. These two options must be supplied
together. They select prebuilt inputs without rebuilding them; the existing compiler and linker consumers still validate
their artifact contents, target, and runtime ABI. Keep the retained directories immutable throughout both runs and any
confirmation. The runtime's producer identity includes compiler source inputs, so automatically rebuilding it separately
for each revision would change its identity even when runtime instructions happen to be identical. A compiler change that
cannot consume the shared artifacts is an incomparable experiment; retain its whole-toolchain evidence separately.

```text
cargo xtask performance gate --base-ref origin/develop --baseline profiles/base/candidate.json --candidate profiles/candidate/candidate.json --output profiles/gate
```

Reports must match corpus, compiler mode, target, LLVM and Rust toolchain, optimization, runtime linkage, runtime input
contents, standard-library manifest, worker budget, host class, and sampling configuration. Report schema 2 records these
identities and measurement start time. Earlier reports need fresh measurements from a revision containing that producer.
A runtime or bundle mismatch is an incomparable experiment and fails identity validation. Establish equivalent inputs
before calling that difference a candidate regression. Keep whole-toolchain migration evidence separately when its inputs
intentionally differ.

The repository owns the limits in [performance-policy.json](../../xtask/performance-policy.json). The initial structural
policy permits no executable-size increase for each named workload. Additional deterministic limits use exact metric
names: `object.bytes`, `executable.section.<name>.bytes`, `compiler.operation.<name>.executions`,
`compiler.metric.<name>`, and `observation.<name>`. Each limit declares `maximum_increase` and an optional `maximum_value`.
Operation limits count executions, including zero for a known operation absent from the sparse statistics. Compiler
metric units must match and describe counts or bytes. The detailed comparison's operation rows remain timing attribution;
timing metrics cannot become structural limits.
A configured metric without comparable evidence fails validation. Unconfigured metrics remain visible as attribution
and do not silently become merge limits. No aggregate improvement can excuse an individual workload's failure.

Timing decisions always name `pass`, `review-required`, or `confirmed-regression`. The policy uses medians, three combined
median absolute deviations, at least seven samples, and raw intervals at least 10,000 timer-resolution units long. A
positive delta, including a 2 percent miss, needs review on its first observation. Insufficient or noisy evidence also
needs review. There is no universal percentage cutoff. Timing can block only when an independently measured baseline and
candidate pair both exceed their noise and adjusted resolution bounds:

```text
cargo xtask performance gate --base-ref origin/develop --baseline profiles/base/candidate.json --candidate profiles/candidate/candidate.json --confirmation-baseline profiles/base-confirmation/candidate.json --confirmation-candidate profiles/candidate-confirmation/candidate.json --output profiles/gate-confirmed
```

Confirmation must retain the same commits and equivalent environment, with distinct measurement start times. A copied
report cannot confirm itself. Rerun only the focused workload set needed for the hypothesis, keeping both sides matched.
Unchanged inputs do not require repeated full-corpus runs. Single compiler observations can pass or need review, but
cannot establish a confirmed timing regression. Execution sampling does not supply missing compiler repetitions.

The gate writes `gate.json`, `gate.html`, and the existing detailed comparison reports. It also retains baseline and
candidate JSON/HTML reports with absolute peer measurements, plus both confirmation JSON reports when supplied.
It exits unsuccessfully for
unwaived confirmed regressions or either report's conformance failure. Review-required evidence remains visible and
requires human assessment without an automatic noisy-timing block. A successful regression gate does not establish the
absolute health or competitive targets in BRA-540, BRA-541, BRA-542, or BRA-428. Retain their candidate peer and resource
reports separately. Their failures cannot be waived by this policy.

### Time-limited waivers

Add a waiver to the repository policy only with a linked PR approval. Declare one exact `workload`, `metric`, and
`target`, numerical `maximum_increase` and `maximum_value` in that metric's recorded units, `rationale`, `owner`, Linear
`issue` URL, `approved_by`, `approval` URL, `approved_on`, and `expires_on`. Dates use `YYYY-MM-DD` in UTC. Expiry is exclusive
at the start of the stated day, and the initial policy allows at most 30 days after approval. No wildcard scope, unknown
field, missing issue or approval, duplicate scope, expired approval, or exceeded bound is accepted.

With any waiver present, the gate requires `curl`, network access to `https://api.linear.app/graphql`, and
`LINEAR_API_KEY`. It checks every tracking issue live and fails closed on missing access, archived or closed issues, or
invalid responses. The key is supplied over stdin and is not included in command arguments or reports. No network or
credential is needed when the policy has no waivers. The report lists all active waivers and retains the original
regression state. Waivers cannot suppress conformance failures or apply to another workload, metric, or target.

### Early complete-consumer checkpoint

Before expanding a cross-cutting compiler or runtime migration, establish a matched baseline and migrate the smallest
real caller/callee, including cleanup and report release. Include a difficult ownership or abnormal-outcome case where
relevant. Preserve its correctness evidence even if the implementation strategy is rejected. An unused ABI foundation
or isolated helper does not establish this checkpoint.

Use the existing focused performance run to measure successful execution, compilation, and executable size. Pass
`--checkpoint <checkpoint.json>` to the gate to retain the experiment with its matched reports. The checkpoint declares
`workload`, `caller_and_callee`, `cleanup_and_report_release`, `ownership_or_failure_case`, `correctness_evidence`,
`hypothesis`, `expected_improvement`, `stopping_condition`, `attribution`, and `attribution_evidence`. All evidence fields
must be nonempty and the workload must occur in the reports. Attribution is `whole_change_only`, `generated_work`,
`retained_runtime_code`, or `runtime_operations`. Use `whole_change_only` until a cause has been isolated. Reference the
existing compiler-operation, artifact/section, allocation/copy, and runtime-operation evidence before prescribing a
redesign. Recording a checkpoint grants no waiver and does not establish final peer parity.

For the meaning and limits of measurements, see [compiler comparisons](performance-reference.md#compiler-comparisons),
[sample ordering](performance-reference.md#runtime-peers-and-sample-ordering), [short-workload
calibration](performance-reference.md#calibrate-short-workloads), and [report
measurements](performance-reference.md#report-measurements).

## Extend the corpus

Add a workload only when it has a stable identity, deterministic output, an explicit scale and unit, and exercises a
distinct implemented cost boundary. Prefer increasing the scale of a focused workload over combining unrelated
operations in one source file. Check the [corpus inventory](../../xtask/src/performance/corpus.rs) for existing
coverage.

Representative stream-only and file-only workloads also enforce required and forbidden logical optimization provenance,
so resource capability boundaries remain independently retainable. See the [report
reference](performance-reference.md#report-measurements) when changing these expectations.

Keep [timing and memory observations](performance-reference.md#timing-and-memory-observation) separate, and preserve
cross-language validation and comparability rules when adding or changing a workload.

Completed measurements that fail runtime linkage, retention, or storage conformance still publish `candidate.json` and
`candidate.html`, and a requested comparison with a structurally valid, compatible baseline. These reports show the
failures and observed dynamic dependencies, disable candidate winner highlighting, and qualify size and peer comparisons
as observations rather than accepted conformance. The command exits unsuccessfully if either compared report fails
conformance. Malformed or incomplete measurements, invalid provenance, and incompatible comparison identities remain
hard errors and never produce a comparison. Structurally invalid candidates are preserved only as
`candidate-unvalidated.json`. Reports carry the conformance failures with the measurements.

Linux application runtime dependencies are static for Bray, Rust, and C++. Provider metadata selects the C++ runtime
archive and static GCC unwinder, and native product planning preserves those static requirements through linking.
Operating-system libraries may remain dynamic. This linkage contract does not establish the competitive runtime footprint
goals tracked by BRA-404.

Return to [repository tasks](xtask.md) to choose another workflow.

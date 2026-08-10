# Compiler profiling

Bray can profile the compiler work performed by `check`, `build`, `run`, `test`, and source inspection commands. Profiling is
opt in. An ordinary invocation does not collect timings, counters, events, or report data.

Use profiling to answer three questions:

- Where does compilation time go
- How much work does a compilation request create
- Did a compiler change alter time, query behavior, or generated work

## Collect a summary

Run a command with `--profile=summary` to print a human-readable report after each compiler invocation:

```text
bray --profile=summary build
```

The same flag works with other compiler-backed commands:

```text
bray --profile=summary check
bray --profile=summary run
bray --profile=summary test
bray --profile=summary inspect syntax --source-id 0
```

Commands that do not invoke the compiler cannot be profiled. Project inspection is one example because Bray reads the project
model without starting the compiler.

Summary mode is the normal starting point. It records aggregate timings, query statistics, and unit measurements. It has less
overhead and produces less data than trace mode.

## Save a report

Add `--profile-output` when the report must survive the invocation:

```text
bray --profile=summary --profile-output profiles/current build
```

The value is a directory. Relative paths are resolved from the selected workspace root. Bray creates the directory and writes one
compact JSON report for each compiler invocation. Report names identify the package, product, target, and action:

```text
profiles/current/hello_world-application-x86_64-pc-windows-msvc-build.json
```

A command that compiles dependencies, multiple products, or multiple targets can create several reports. Repeating the same action
for the same package, product, and target in the same output directory replaces that report, so use separate directories for
baseline and candidate runs. The repository ignores the `profiles/` directory.

The JSON artifact is a compact interchange format. Use Bray to read it rather than opening it manually:

```text
bray profile show profiles/current/hello_world-application-x86_64-pc-windows-msvc-build.json
```

`profile show` validates the schema and descriptor references, then renders the same bounded human report used during collection.
It does not compile the project and can run outside the project that produced the report.

## Read the summary

Every report begins with its compilation identity:

```text
Compiler profile: hello_world/application (x86_64-pc-windows-msvc)
```

Only compare results with the same package, product, target, action, build configuration, toolchain, and worker count.

### Time breakdown

`Elapsed` is wall-clock time inside the compiler profiling session. The surrounding Bray build can take longer because process
startup, project planning, and other build-tool work are outside that session.

`Summed worker self time` adds self time from every compiler worker. It can exceed elapsed time when workers run concurrently.
The indented categories divide that worker time by the kind of work observed:

- `Active compiler work` is in-process compiler work
- `Scheduler queue` is time waiting for compiler worker capacity
- `Dependency wait` is time waiting for a query evaluation owned by another task
- `External tools` is time spent in processes such as the linker or archiver

Do not add operation totals to elapsed time. Operations can be nested and parallel workers can overlap. Treat elapsed time as the
latency seen by the invocation and worker self time as the amount of occupied worker time.

### Query totals

The `Queries` line summarizes demand-driven compiler queries:

- `requests` counts every query request
- `evaluations` counts computations that actually ran
- `hits` and `Hit rate` show requests served from an already published value
- `misses` counts requests that found no published value
- `waits` counts requests that waited for an evaluation already running elsewhere

Several requests can wait on one evaluation, so misses do not have to equal evaluations. A high hit rate alone does not prove that
querying is cheap. The evaluated queries may still dominate the compilation.

When cross-snapshot reuse or invalidation occurs, the summary also reports reused values and invalidations on a `Snapshots` line.

### Top operations

`Top operations by worker self time` ranks up to ten operation kinds by exclusive same-thread work:

- `Calls` is the number of observed executions
- `Total` is inclusive time, including nested operations on the same thread
- `Self` excludes nested profiled operations on the same thread
- `Maximum` is the longest single execution

Start with `Self` when looking for the compiler work that consumes worker capacity. Use `Maximum` to spot a slow individual unit.
Use `Total` to understand an operation's whole subtree, while remembering that nested and cross-thread work can overlap other rows.
Orchestration operations can therefore have large inclusive totals without representing additional elapsed time.

### Top queries

`Top queries by evaluation time` ranks up to ten query kinds by inclusive evaluation time:

- `Requests` shows total demand for the query kind
- `Evals` shows how often its computation ran
- `Hit rate` shows the portion served from published values
- `Evaluation` is inclusive time spent evaluating it
- `Wait` is time callers spent waiting for another task to publish it

Evaluation time is inclusive. A parent query includes work performed by child queries, and parallel work can overlap. Use the table
to find expensive query families, then correlate them with the operation and unit tables instead of summing its rows.

### Compilation units and artifacts

The final table describes the amount of work and output produced by the invocation. Depending on the command, it can include source
units and bytes, syntax tokens, declarations, bound units, checked bodies, MIR units and operations, code generation instances and
units, interface sections and bytes, link inputs, emitted artifacts, and emitted bytes.

These measurements distinguish a slow implementation from unexpectedly large work. Useful relationships include:

- Code generation instances compared with code generation units, which reveals partitioning and fragmentation
- Link inputs compared with code generation output, which reveals linker fan-in
- Emitted bytes compared with source and MIR size, which catches unexpectedly large artifacts
- Checked bodies and MIR units compared with declarations, which shows how much source became executable work

Large changes in these counts often explain timing changes more directly than a timing row does.

### Selected runtime artifacts

Builds that select runtime components list each stable component identity and authenticated archive size. This makes runtime
selection attributable without opening the compact report or inspecting linker inputs. A synchronous program that needs no
runtime-owned role or capability has no selected-runtime section. Unexpected identities identify an over-broad requirement or
component ownership declaration. Unexpected bytes identify a packaging or partitioning regression even when the final linker later
discards unused archive members.

Compare both the identities and their sizes. A component can be selected correctly while its physical archive grows because shared
support was duplicated into it. Conversely, a smaller archive set is not an improvement if a required semantic component has
disappeared.

## Compare two reports

Collect reports into separate directories while holding the command and environment constant:

```text
bray --profile=summary --profile-output profiles/baseline build
bray --profile=summary --profile-output profiles/candidate build
```

Compare the matching report files:

```text
bray profile compare profiles/baseline/hello_world-application-x86_64-pc-windows-msvc-build.json profiles/candidate/hello_world-application-x86_64-pc-windows-msvc-build.json
```

The comparison reports before, after, absolute change, and percentage change for elapsed and worker-time categories. It also ranks
the largest operation self-time changes and query evaluation-time changes, then lists changed unit and artifact measurements.

Bray rejects incompatible reports. Package, product, target, schema, and shared descriptor meanings must agree. Reports should also
come from equivalent actions and build environments even where that context is controlled outside the report schema.

Timing is noisy. Run each case several times, compare representative runs, and investigate changes that remain larger than normal
variation. Keep these inputs fixed:

- Bray and compiler build configuration
- Project build configuration such as debug or release
- Target and selected product
- `--cpu-count`
- Toolchain and linker
- Machine load and filesystem state as far as practical
- Profiling mode

## Collect a trace

Trace mode adds a bounded event timeline to all summary data:

```text
bray --profile=trace --profile-output profiles/trace build
```

Trace mode requires `--profile-output`. A report retains at most 65,536 events per compiler invocation and records how many further
events were dropped. `bray profile show` renders the aggregate summary plus retained and dropped event counts. The event timeline in
the JSON report supports detailed scheduling, dependency, and concurrency analysis.

Use trace mode after a summary points to queueing, dependency waits, inconsistent latency, or overlapping work that aggregate totals
cannot explain. Trace collection has more overhead than summary collection, so compare trace runs only with other trace runs.

## Diagnosis workflow

Use this sequence for a compiler performance investigation:

1. Reproduce the slow command with `--profile=summary`.
2. Compare compiler `Elapsed` with Bray's overall command time to locate compiler work versus outer build-tool overhead.
3. Find the dominant operation by `Self` time and check whether one call dominates its `Maximum`.
4. Correlate that operation with the top query families.
5. Check unit and artifact counts for unnecessary work or fragmentation.
6. Save a baseline report before changing code.
7. Collect several candidate reports under the same conditions and use `bray profile compare`.
8. Collect a trace only when aggregate evidence cannot explain scheduling or concurrency behavior.

For example, many code generation instances and units, many link inputs, dominant `codegen generate` self time, and substantial
`link` time together point to code generation partitioning rather than parsing or semantic analysis. A large scheduler queue instead
points to worker-capacity pressure. Large dependency wait with few evaluations points to contention around expensive shared queries.

Profiling measures the compiler while changing it slightly. Draw conclusions from stable differences across equivalent profiled runs,
not from a single measurement or from comparisons between profiled and unprofiled commands.

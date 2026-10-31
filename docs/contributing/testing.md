# Testing

Follow the [unit-test placement rules](coding-conventions.md#tests). Use integration tests for repository contracts that
cross module or crate boundaries.

See [Repository tasks](xtask.md) for common commands and specialized workflows.

Use [Compiler profiling](profiling.md) to diagnose compiler performance and compare compilation runs.

## Compiler test tiers

The default `bray-compilation` suite is the fast behavior tier. Its reference execution target is **120 seconds** on a
development host with four available CPU workers and the pinned LLVM toolchain. Wall time is reported rather than used
as a portable pass/fail threshold. Compilation is measured separately. The workspace uses an optimized compiler test
profile and retains Cargo's incremental cache for local rebuilds. The task harness remains unoptimized in the test
profile.

Run the measured suite with stable Cargo and Rust's standard test harness:

```text
cargo xtask test compiler
```

No additional test runner is required. `cargo test -p bray-compilation` runs the same fast test selection through
Cargo's ordinary harness. Focused Cargo filters continue to work. The command records preparation and execution
separately and returns failure for incomplete coverage or failed tests. A generous command timeout contains hangs.
The default is 30 minutes and can be changed with `--timeout-seconds`, so slower or busy machines do not fail a speed
threshold. Exceeding the reference target is recorded in the report and printed without failing the suite.

Expensive tests carry an explicit `#[ignore = "tier:..."]` reason. They remain executable coverage:

| Tier         | Coverage                                                                            | Reference execution target per shard |
|--------------|-------------------------------------------------------------------------------------|--------------------------------------|
| `fast`       | Small semantic, query, lowering, and orchestration checks                           | 120 seconds                          |
| `native`     | LLVM generation, native emission, linking, and execution integration                | 300 seconds                          |
| `proof`      | Production bootstrap and synchronization proofs, dense ownership and storage stress | 600 seconds                          |
| `exhaustive` | Full native target matrices                                                         | 600 seconds                          |

```text
cargo xtask test compiler --tier native
cargo xtask test compiler --tier proof
cargo xtask test compiler --tier exhaustive
cargo xtask test compiler --tier all
cargo xtask test compiler --tier proof --shard 2/4
```

The `all` tier has a 600 second reference target. To run everything directly with Cargo, use
`cargo test -p bray-compilation -- --include-ignored`. Use `--ignored` to run only the explicit slow tiers. A focused
ignored test also needs `--ignored` or `--include-ignored`.

Shards divide the sorted `(binary, test name)` inventory by ordinal modulo the shard count. The shard union covers the
selected tier exactly once. Pull requests run fast and native coverage. Proof and exhaustive
coverage runs nightly on `develop` and on manual workflow dispatch, with four deterministic shards per tier. To check
those tiers before merging a relevant change, run the documented commands locally or dispatch the full workflow for
that branch once the workflow exists on the default branch. Do not remove a test or shrink its stress inputs to meet
the reference target. Put substantial native, proof, stress, or target-matrix work in its owning tier and keep a focused
behavior test in `fast` where appropriate. New ignored tests must carry one of these tier tags or discovery fails.

The routine PR path avoids repeating full production proofs and cross-target matrices for every update. CI uses the
`ci` Cargo profile, which retains test optimization and disables debug information. The runner is built with
`cargo build --profile ci -p xtask --no-default-features --bin xtask`. Disabling the default `full` feature keeps compiler
libraries and LLVM bindings out of the runner's dependencies. Its test command uses `--profile ci` so discovery reuses
the prepared compiler-test binaries. Local commands retain the normal test profile and debug information.
Cargo outputs and prepared LLVM toolchains are cached, including Cargo outputs after test failures. Prepared LLVM
toolchains are cached before compiler tests and validated when restored. Download archives remain available for
prepared-cache misses. See [LLVM provisioning](llvm.md) for the cache identity and reuse contract.
Superseded PR runs are cancelled. The nightly run also refreshes the default branch's cache for subsequent PRs.
Nightly and manual full runs use a separate concurrency group. Cold-cache setup and
compilation remain visible in the workflow's preparation steps and are separate from test execution times.

Reports remain under `target/compiler-tests/<tier>/<index>-of-<count>/`, respecting `CARGO_TARGET_DIR`. They include the
complete selection, raw log, JUnit report, and a JSON timing report with the slowest cases first. CI retains these
artifacts on success and failure. Test durations are observed serial completion spans, including harness and process
startup overhead for the first test in a batch. They are useful for identifying expensive cases, rather than precise
microbenchmarks. Each shard runs up to four bounded argument batches concurrently. Batches contain at most 16 tests and
workers take the next available batch. Each batch runs one test at a time so completed fixture work can be shared and
per-test completion spans remain attributable. Compiler worker-count and parallel-demand tests still exercise their own
requested worker budgets.

Production host, synchronization, and standard-library fixtures share parsed and discovered source snapshots per target.
Identical host requests also share completed bound and proof results. Changed probes fork immutable snapshots through
the compiler's normal dependency invalidation. They retain separate diagnostics and cannot alter the baseline. Cache
initialization uses a separate one-time cell per target and does not hold the inventory lock during compilation. Tests
that inspect cold-query behavior, cancellation, or worker scheduling continue to construct fresh compilations.
Regression checks use deterministic work: concurrent and repeated probe requests construct exactly one production
fixture per target, and repeated production symbol, checking, and lowering demands perform zero additional query
evaluations. These checks are independent of CPU speed and system load.

The proof tier's `production_fixture_phase_timings` case records ordinary compiler summary profiles after parsing,
symbol binding, checking, and lowering. The tier runner retains these under `phases/`. These checkpoints are cumulative
and include query evaluation and reuse counters. A final `reuse` checkpoint records the no-additional-work assertion.
Use [Compiler profiling](profiling.md) to interpret the reports.

A reference run on 2026-10-29 with a four-CPU Linux quota completed 1,911 fast tests in 57.89 seconds. All 277 slow
tests passed in the four-shard CI arrangement in a further 8 minutes 44 seconds, covering all 2,188 compiled tests
without missing or duplicated identities. This is about ten minutes for complete execution across the tiers. A single
`--tier all` run completed the same 2,188 tests in 8 minutes 8 seconds. The first optimized test-profile build took
7 minutes 7 seconds with dependencies already available. Build time is reported separately from the timing targets and
operational timeout.

## Test a changed Rust crate

Use nextest to run the tests for the crate you changed:

```text
cargo nextest run -p <crate>
```

> `cargo test` must also remain supported, as required by the repository rules.

Use `cargo test --workspace` to include every workspace package. Plain `cargo test` at the checkout root selects the
workspace's default developer tool packages.

## Testing Bray programs

Use `bray test` to compile and run tests written in Bray. This workflow matters when developing the compiler because it
exercises the complete path from Bray source to a runnable test executable. The command builds every selected Bray test
target, records its executable and catalog as one retained generation, and runs the selected entries:

```text
bray test [selection options]
```

Use the same selection with `--no-build` when the Bray test targets have already been built. This reruns them without
compilation, emission, or linking:

```text
bray test --no-build [selection options]
```

A no-build rerun succeeds only when every selected test target has a current retained generation whose source, project,
compiler, toolchain, standard-library, runtime, and protocol identities still match. A mismatch identifies the stale
identity category and requires a normal `bray test` invocation to publish a matching generation. JSON reports identify
the retained generations and state whether compilation, emission, or linking occurred during the command.

## Native composition tests

Run the small native corpus before broad standard-library conformance:

```text
cargo xtask composition
cargo xtask composition --case cleanup
cargo xtask composition --case cleanup --no-build
```

The cases cover synchronous calls, a compiled standard-library call, async runtime-role selection, box replacement and
scope cleanup, selected-entry filtering, typed error reporting, and compiler requests larger than the Windows command
line limit. Each case uses the production package, compiler, linker, native test host, catalog, and JSON report paths.
The synchronous case also checks rejection of changed compiler, toolchain, standard-library, runtime, input, protocol,
host, and catalog identities. Identity probes restore the retained product before returning.

A normal run prepares cached runtime and standard-library bundles, builds each selected product, and verifies an
unchanged no-build rerun. `--no-build` skips compiler-tool and bundle builds and uses the ordinary retained-product
checks. It fails when the selected product has not been built or an input has changed.

Products and JSON reports remain under the Cargo target directory's `composition` directory. Use `--output <directory>`
to select another persistent location, and pass the same location on reruns. Failures include the failed command's
structured diagnostics and the available compiler, toolchain, runtime, standard-library, host, catalog, and generation
identities. The first invocation can build compiler tools and bundles. Later invocations reuse their existing caches.

The suite requires a supported native host with [LLVM](llvm.md) and a native linker configured.

## Standard-library test completeness

Run the native API plans and outcome checks with:

```text
cargo xtask standard-library test --part api --part outcomes
```

The audit derives expected tests from the original product sources, module availability for the selected native target,
and declared `@test` functions. It checks the published catalog from the copied test workspace against those independent
facts before accepting reports. New source files and tests automatically join the full sequential and parallel plans.
The audit prints target exclusions and checks exact identities, source anchors, execution metadata, and catalog digests.

Focused plans declare their intended identities beside their execution oracles. Update those local expectations when
deliberately widening a plan. Outcome plans must cover every applicable outcome test exactly once, so a new case also
needs an execution oracle. There is no global expected-test count to update.

The suite requires a supported native host with LLVM and a native linker configured. Source-inventory unit tests cover
all supported target profiles, while execution checks run on the current host.

## Readiness audits

Repository-wide readiness audits are development checks rather than ordinary behavioral tests. Run every audit
explicitly with:

```text
cargo xtask readiness
```

For example, run one structural audit or the separate native execution fixtures:

```text
cargo xtask readiness semantic
cargo xtask readiness native-execution
```

The default readiness command excludes native execution. Pass `parser`, `semantic`, `diagnostics`, `lowering`, `memory`,
`codegen`, `emission`, or `linker` to run one structural audit. The command parses the Rust workspace once and shares
that corpus across every selected audit. Pass `native-execution` to compile Bray fixtures twice, inspect their native
objects, and execute the linked products. Ordinary Rust test runs do not run these comparatively expensive
repository-wide and toolchain checks.

Dependency audits inspect Cargo's normal and build declarations across all targets using package identities, including
renamed and inherited dependencies. Development-only fixtures are excluded from production boundaries. The lowering
audit also requires `bray-ir` as a normal dependency.

The native execution fixtures live under `xtask/fixtures/native-execution/`. The startup fixture verifies deterministic
objects and executables, the expected ELF structure and direct-call relocation, and a successful process exit. The ABI
fixture exports a Bray function under an exact C-compatible symbol. A checked-in target-native host calls that symbol
and exits with the returned value so the audit observes behavior across the native call boundary.

Keep these fixtures independent of standard-library I/O so the compiler-to-process boundary can be validated before a
target standard library is available.

## Semantic coverage fixture

`xtask/fixtures/readiness/semantic-coverage.json` is the machine-checked pre-lowering semantic coverage inventory. It is
a fixture, not design documentation. Its rule-family entries name production owners, structured-diagnostic source tests,
valid source tests, imported or compiler-known boundary tests, and published query results. Its cross-cutting entries
cover target and product variation, lazy demand, scheduling, recovery, cancellation, bounded work, and deterministic
diagnostics.

Update the fixture when a semantic rule family or its owning query changes. Every source-test anchor must identify an
executable test in `bray-binder` or `bray-compilation`. Boundary-test anchors may identify focused tests in other
compiler crates. The semantic readiness audit rejects missing rows, placeholders, stale code anchors, and non-executable
test anchors.

## Lowering coverage fixture

`xtask/fixtures/readiness/lowering-coverage.json` is the machine-checked lowering boundary inventory. It maps every
bound-expression variant, structured-expression kind, pattern form, bound-unit root, and required semantic input to its
production lowering owner and an executable test. Cross-cutting rows cover MIR validation, lazy publication,
worker-count determinism, and recovery. The lowering readiness audit directly enforces the MIR-only codegen dependency
boundary.

Update the fixture whenever one of those closed enums or its lowering owner changes. The lowering readiness audit
rejects missing, duplicate, placeholder, stale production, and non-executable test anchors.

Changes to the checker-publication boundary need focused tests in `bray-lowering` for input identity, target and unit
kind contracts, duplicate lookup indexes, direct sharing, unreachable exits, and no-cleanup exits. Add cross-crate tests
in `bray-compilation` when the behavior depends on real checker output, such as nested exits, propagation, cancellation,
task operations, or retained suspension dependencies. These tests establish that lowering consumes one complete
published input and does not recreate semantic decisions locally.

Generated-helper tests belong at the completed MIR boundary because helper identity depends on final MIR operation
shape. Test exhaustive operation-to-helper derivation in `bray-ir` and exact helper and runtime-symbol mapping coverage
in `bray-codegen`.

## Diagnostic coverage

Every `DiagnosticKind` must have an executable test that passes the actual produced bag and the exact expected kind to
`assert_goal_state_diagnostic_kind` at its production or conversion boundary. Every distinct kind from a producer path
needs its own exact assertion. `cargo xtask readiness diagnostics` rejects kinds without that evidence. Generic bag-wide
assertions, catalog tests, and fabricated `Diagnostic::new` values do not satisfy the producer-coverage requirement.

`DiagnosticKind::ALL` is generated from the same declaration that defines the enum. Use it for whole-catalog invariants
rather than maintaining a separate diagnostic list. The `bray-messages` renderer tests must render the complete
inventory so every locale catalog branch is exercised.

## Code generation coverage fixture

`xtask/fixtures/readiness/codegen-coverage.json` is the machine-checked code generation boundary inventory. It maps
every MIR unit kind, operation kind, terminator kind, runtime ABI role, and backend artifact kind to the production code
that translates, serializes, or deliberately rejects it. Cross-cutting rows identify executable tests for backend
capability failures, exact target and runtime mappings, cancellation, lazy publication, narrow artifact requests, and
deterministic serial and parallel demand.

Update the fixture whenever one of those closed enums or its backend handling changes. The code generation readiness
audit rejects missing, duplicate, placeholder, stale production, and non-executable test anchors. It also guards the
backend-neutral dependency and public API boundaries of `bray-codegen` and `bray-compilation`.

## Emission coverage fixture

`xtask/fixtures/readiness/emission-coverage.json` is the machine-checked artifact-emission contract inventory. It maps
immutable planning, deterministic contribution merging, output validation and publication failures, cancellation
boundaries, bounded work, runtime compatibility, executable-host artifacts, and structured shutdown to production owners
and executable tests. The emission readiness audit rejects missing, reordered, placeholder, stale production, and
non-executable test anchors. It also guards the emitter boundary against source-semantic, MIR,
compilation-orchestration, and driver dependencies.

## Linker coverage fixture

`xtask/fixtures/readiness/linker-coverage.json` is the machine-checked native-linking contract inventory. It maps plan
validation, driver selection, deterministic invocation, product, output, failure, cancellation, and executable-host
contracts to their production owners and executable tests. The linker readiness audit rejects missing, duplicate,
placeholder, stale production, and non-executable test anchors. It also guards the typed linker boundary against
source-semantic dependencies and interpretations.

## Parser coverage

[The parser coverage fixture](../../crates/bray-parser/tests/fixtures/parser-coverage.md) maps grammar nonterminals to parser and executable test anchors. Update it with
changes to the grammar or those anchors. Check it with `cargo xtask readiness parser`, which also runs as part of
default readiness and reuses its parsed Rust corpus. Backticked function anchors resolve within the parser crate. Cargo
target roots and declared Rust modules establish source inclusion and logical owners, including literal `#[path]` module
names. Test anchors must identify actual `#[test]` functions, and ambiguous names need their module path. Parser
annotations such as dispatch, shared helpers, precedence and token sets accompany real function anchors. Missing
production functions and stale, ambiguous or helper-only test references fail with their coverage row and anchor.

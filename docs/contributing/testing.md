# Testing

Keep unit tests near the production code they exercise. Use integration tests for repository contracts that cross module
or crate boundaries.

See [Repository tasks](xtask.md) for the complete development-command reference.

Use [Compiler profiling](profiling.md) to diagnose compiler performance and compare compilation runs.

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

## Readiness audits

Repository-wide readiness audits are development checks rather than ordinary behavioral tests. Run every audit
explicitly with:

```text
cargo xtask readiness
```

Pass `semantic`, `diagnostics`, `lowering`, `codegen`, `emission`, or `linker` to run one structural audit. The command
parses the Rust workspace once and shares that corpus across every selected audit. Pass `native-execution` to compile
Bray fixtures twice, inspect their native objects, and execute the linked products. Normal `cargo test` does not run
these comparatively expensive repository-wide and toolchain checks.

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

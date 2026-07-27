# Testing

Keep unit tests near the production code they exercise. Use integration tests for repository contracts that cross module or crate
boundaries.

## Semantic coverage fixture

`crates/bray-compilation/tests/fixtures/semantic-coverage.json` is the machine-checked pre-lowering semantic coverage inventory. It is
a test fixture, not design documentation. Its rule-family entries name production owners, structured-diagnostic source tests, valid
source tests, imported or compiler-known boundary tests, and published facts or queries. Its cross-cutting entries cover target and
product variation, lazy demand, scheduling, recovery, cancellation, bounded work, and deterministic diagnostics.

Update the fixture when a semantic rule family or its owning query changes. Every source-test anchor must identify an executable test
in `bray-binder` or `bray-compilation`. Boundary-test anchors may identify focused tests in other compiler crates. The integration test
rejects missing rows, placeholders, stale code anchors, and non-executable test anchors.

## Diagnostic coverage

Every `DiagnosticKind` must have an executable test that directly references the kind at its production or conversion boundary.
`crates/bray-compilation/tests/semantic_readiness.rs` parses workspace test functions and rejects any diagnostic kind without that
evidence. Generic catalog tests do not satisfy the producer-coverage requirement.

`DiagnosticKind::ALL` is generated from the same declaration that defines the enum. Use it for whole-catalog invariants rather than
maintaining a separate diagnostic list. The `bray-messages` renderer tests must render the complete inventory so every locale catalog
branch is exercised.

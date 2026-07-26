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

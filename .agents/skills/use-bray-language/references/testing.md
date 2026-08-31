# Testing

**Specification:** [Test products and entries](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/test-products-and-entries.md)

Use `@test` on a module declaration to make that contribution test-only. Mark module-level test functions with bare `@test`; use `@test(serial)` only when process-wide state requires command-wide exclusion. Execution order is unspecified, parallel tests may overlap, and shared state still requires ordinary synchronization.

```bray
@test
module package.tests;

@test
func parses_minimal_input() -> Result<unit, ParseError>
{
    let value = try parse(minimal_input());
    assert(value.count() == 1);
    return Ok(unit);
}
```

A test function has no receiver, generics, or parameters; returns `unit` or `Result<unit, E>`; may be `async`; and follows ordinary visibility, internal-use, trust, ownership, and capability rules. Integration-test products consume the tested library through its public compiled contract. Use test-only modules for reusable fixtures and helpers; only functions marked `@test` become entries.

Name tests for observable behavior. Arrange the smallest local fixture, exercise the same API boundary its caller uses, and assert results rather than implementation steps. Cover successful, rejected, and boundary behavior where each is part of the contract; keep unrelated behaviors in separate tests. Prefer ordinary local state and deterministic inputs, and isolate process, clock, filesystem, scheduler, or platform state behind the narrowest available fixture.

Use `assert(condition[, message])` for boolean invariants. Use `std.testing.assert_ok` and `assert_error` for `Result`, `assert_present` and `assert_absent` for nullable values, and `assert_completed`, `assert_panicked`, `assert_cancelled`, or `assert_not_completed` for `RunResult`. Payload-selecting helpers return the selected value. Keep an explicit match only when the test must inspect a more specific domain variant after selecting the outer state.

Returning `unit` or `Ok(unit)` passes; `Error`, panic, or run cancellation produces the corresponding failed or cancelled outcome. Run the selected test product with `bray test`.

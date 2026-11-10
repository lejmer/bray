# Result and run result propagation expressions

Syntax follows the [expression roots and precedence structure grammar](../syntax-grammar.md#expression-roots-and-precedence-structure).

The result propagation expression is:

```bray
try expression
```

The operand is evaluated exactly once.

The operand must have type `Result<T, E>` or `RunResult<T>`.

The operand type selects the propagation behavior. Expected result type does not select a propagation behavior or
overload.

`try` unwraps exactly one layer.

`try` is a propagation expression. It does not begin a try-catch statement, install an exception handler, or create a
resumable exception region.

For an operand of type `Result<T, E>`, the normal continuation has type `T`.

If the operand is the `Result.Ok` variant, `try` evaluates to its `value` payload.

If the operand is the `Result.Error` variant, `try` propagates its `error` payload to the nearest result
propagation boundary that admits its error type as a `Result.Error` value.

A result propagation boundary for a `Result.Error` outcome is:

- a callable execution scope whose result type is `Result<R, F>`,
- a single-yield region whose result type is `Result<R, F>`.

In either case, `E` must meet [error propagation conversion](#error-propagation-conversion) for `F`.

For an operand of type `RunResult<T>`, the normal continuation has type `T`.

If the operand is the `RunResult.Completed` variant, `try` evaluates to its `value` payload.

If the operand is the `RunResult.Panicked` variant, `try` forwards its `report` payload into panic propagation in the
current run.

If the operand is `RunResult.Cancelled`, `try` forwards cancellation as the cancellation outcome of the current run.

Every executing Bray operation belongs to one current run. The current run is one of:

- the executable or test root run,
- a runtime-scheduled task run,
- a standard-library native-thread run,
- a standard-library child-process run using the conforming Bray process protocol,
- another trusted execution root whose contract establishes the same run-outcome semantics.

Ordinary synchronous calls and direct awaits do not create a new run. Run-result forwarding can therefore leave nested
callable and block scopes while resolving their ordinary lifecycle obligations, just as panic or cancellation
propagation already does. It does not search for a handler, select a result type, return a `RunResult` value from an
intervening callable, or resume an abandoned continuation.

Forwarding `RunResult.Panicked(report)` continues panic propagation with the existing report rather than constructing a
new panic payload. An enclosing `catch` in the current run can convert that propagation into `Result.Error(report)`.
Forwarding `RunResult.Cancelled` enters the current run's cancellation cleanup even when the cancellation originated in
a different child run.

`try` on `RunResult<T>` has the panic and cancellation effects of those forwarding paths. It is rejected in a context
that cannot participate in run execution, including constant and predicate evaluation.

## Error propagation conversion

A `Result.Error` payload can propagate to a boundary when a built-in conversion exists from the source error type to
the boundary's error type. This includes identity, lossless scalar conversion, callable contract
conversion, and recursive built-in tuple, array, nullable, and tuple-to-complex conversion under
[conversion expressions](conversion-expressions.md#built-in-conversion).
The search proceeds through enclosing result boundaries from nearest to farthest and selects the first that meets
this rule. It does not select a `ConvertTo` implementation. If no boundary meets the rule, the program must map the error
explicitly before propagation.

This relation applies only to error propagation. It does not widen ordinary assignment, argument, or branch result
acceptance.

## Propagation paths

On the propagation path, the current control-flow path has no normal continuation and has type `never`.

Propagation follows the same ownership, destruction, finalization, capability, and effect rules as an explicit exit to
the target boundary.

`try` is not valid in predicate expressions, contract expressions, guard expressions, or pattern contexts.

`try await expression` means `try (await expression)`.

```bray
func load_user(pos id: UserId) -> Result<User, LoadError>
{
    let row = try db.fetch_user(id);
    let user = try decode_user(row);

    return Ok(user);
}
```

```bray
async func main_work(pos task: Task<Result<User, LoadError>>) -> Result<unit, LoadError>
{
    let result = try await task.join();
    let user = try result;

    use(user);
    return Ok(unit);
}
```

In `main_work`, the first `try` unwraps `RunResult.Completed` or forwards the child task's panic or cancellation into
the current run. The second `try` unwraps `Result.Ok` or propagates the recoverable `LoadError` to the callable's
lexical result boundary. Each `try` removes exactly one semantic layer.

Explicitly returning or storing `RunResult<T>` remains valid. It is used when the caller wants to inspect or preserve
the child-run outcome instead of forwarding it.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Nullable and absence expressions](nullable-and-absence-expressions.md)
- Next: [Conditional expressions](conditional-expressions.md)

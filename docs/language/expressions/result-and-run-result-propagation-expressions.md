# Result and run result propagation expressions

The result propagation expression is:

```bray
try expression
```

The operand is evaluated exactly once.

The operand must have type `Result<T, E>` or `RunResult<T>`.

The operand type selects the propagation behavior. Expected result type does not select a propagation behavior or overload.

`try` unwraps exactly one layer.

For an operand of type `Result<T, E>`, the normal continuation has type `T`.

If the operand is the `Result.Ok` variant, `try` evaluates to its `value` payload.

If the operand is the `Result.Error` variant, `try` propagates its `error` payload to the nearest compatible result propagation
boundary as a `Result.Error` value.

A result propagation boundary for a `Result.Error` outcome is:

- a callable execution scope whose result type is `Result<R, F>` where `E` is compatible with `F`,
- a single-yield region whose result type is `Result<R, F>` where `E` is compatible with `F`,
- a callable execution scope whose result type is `RunResult<Result<R, F>>` where `E` is compatible with `F`,
- a single-yield region whose result type is `RunResult<Result<R, F>>` where `E` is compatible with `F`.

When `Result.Error` propagates to a `RunResult<Result<R, F>>` boundary, the supplied boundary value is
`RunResult.Completed(Result.Error(error))`.

For an operand of type `RunResult<T>`, the normal continuation has type `T`.

If the operand is the `RunResult.Completed` variant, `try` evaluates to its `value` payload.

If the operand is the `RunResult.Panicked` variant, `try` propagates its `report` payload to the nearest compatible run-result
propagation boundary as a `RunResult.Panicked` value.

If the operand is `RunResult.Cancelled`, `try` propagates `RunResult.Cancelled` to the nearest compatible run-result propagation
boundary.

A run-result propagation boundary is:

- a callable execution scope whose result type is `RunResult<R>`,
- a single-yield region whose result type is `RunResult<R>`.

The propagated `RunResult.Panicked` or `RunResult.Cancelled` value does not depend on the boundary's success type.

If no compatible propagation boundary is available, `try` is rejected.

Nested callable execution scopes and nested yield-capable regions create their own propagation boundaries when their result type is
compatible with the propagated outcome.

Error payload types must be compatible through ordinary type compatibility. If the source error type does not fit the boundary
error type, the program must map the error explicitly before propagation.

On the propagation path, the current control-flow path has no normal continuation and has type `never`.

Propagation follows the same ownership, destruction, finalization, capability, and effect rules as an explicit exit to the target
boundary.

`try` is not valid in predicate expressions, contract expressions, guard expressions, or pattern contexts.

`try await expression` means `try (await expression)`.

```bray
func load_user(pos id: UserId) -> Result<User, LoadError>
{
    let row = try db.fetch_user(id);
    let user = try decode_user(row);

    return Result.Ok(user);
}
```

```bray
async func collect(pos task: Task<Result<User, LoadError>>) -> RunResult<Result<User, LoadError>>
{
    let result = try await task.join();
    let user = try result;

    return RunResult.Completed(Result.Ok(user));
}
```

In the `collect` example, `await task.join()` has type `RunResult<Result<User, LoadError>>`.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Nullable and absence expressions](nullable-and-absence-expressions.md)
- Next: [Conditional expressions](conditional-expressions.md)

# With expressions

A `with` expression enters scoped lifecycle behavior for a resource and evaluates a body while the scoped capability is
active.

Expression syntax is defined in [With expressions](../expressions/with-expressions.md).

The initializer must produce a resource value or access path whose type has an applicable `enter` lifecycle declaration.
The binding's type annotation, when present, applies to the resulting scoped capability and does not select the `enter`
declaration. Pattern bindings are visible only inside the `with` body.

At execution, the initializer is evaluated once. The selected `enter` creates the capability matched by the pattern,
then the block body executes. In value-producing context, the body is a single-yield region. Its produced value is held
as the pending `with` expression result until scope exit completes.

The matching `exit` lifecycle declaration runs on every path leaving the `with` body.

The `exit` lifecycle declaration runs for:

- normal completion,
- `yield`,
- `return`,
- `break`,
- `continue`,
- nullable propagation,
- result propagation,
- run-result propagation,
- panic propagation,
- cancellation,
- any other control-flow exit from the body.

The scoped capability remains live while `exit` runs, before ordinary local destruction caused by leaving the body.
After exit completes, the original body result or control-flow outcome continues.

If `exit` returns an error while a `return`, `break`, `continue`, propagation, panic, or cancellation outcome is pending,
the pending outcome continues and the exit error is retained as a suppressed incident. The error does not replace the
pending transfer. When the body completes normally or yields its value, an exit error contributes to the `with`
expression's failure contract.

If `enter` does not complete successfully, the pattern is not matched, the `with` body is not evaluated, and `exit` does
not run.

The scoped bindings, borrows from them, and capabilities derived from them cannot escape the `with` body unless the
selected `exit` contract explicitly transfers that obligation.

Checking the selected entry, body, and exit enforces their ownership, borrowing, mutation-authority, initialization,
capability, and effect requirements. The expression retains their task, destruction, and finalization obligations under
[scoped use](scoped-use.md). Conditions about scoped bindings remain subject to
[dependency invalidation](../contracts-and-trust/contract-reasoning.md) when those bindings or capabilities end.

## Scoped borrow escape example

Example: assume an ordinary synchronous resource whose infallible `enter` returns an owned guard with a `value: u8`
field. Its infallible `exit` resolves that guard. These are alternative body fragments, each with a fresh owned
`resource` and no other lifecycle obligations.

Copying the byte out is accepted:

```bray
let value: u8 = with guard = resource
{
    yield guard.value;
};
```

Returning a borrow of the guard's field is rejected:

```bray
let value: &u8 = with guard = resource
{
    yield &guard.value;
};
```

The guard ends before the expression's continuation. The copied byte has its own value, while the borrow would depend
on storage that has ended. [Scope exits and ownership boundaries](../ownership-and-borrowing/scope-exits-and-ownership-boundaries.md)
require the destination to preserve that dependency.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Scoped use](scoped-use.md)
- Next: [Partial values and replacement](partial-values-and-replacement.md)

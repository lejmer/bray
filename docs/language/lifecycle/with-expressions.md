# With expressions

A `with` expression enters scoped lifecycle behavior for a resource and evaluates a body while the scoped capability is active.

Expression syntax is defined in [With expressions](../expressions/with-expressions.md).

The initializer expression is evaluated once.

The initializer must produce a resource value or access path whose type has an applicable `enter` lifecycle declaration.

The selected `enter` lifecycle declaration creates the scoped capability value matched by the pattern.

The type annotation on the binding side, when present, applies to the scoped capability value produced by `enter`.

The type annotation does not participate in `enter` lifecycle declaration selection.

The scoped bindings introduced by the pattern are visible only inside the `with` body.

The `with` body is a block expression.

In value-producing context, the `with` body is a single-yield region and supplies the `with` expression result.

When the `with` body produces a value, that value is held as the pending `with` expression result until scope exit completes.

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

The scoped capability remains live while `exit` runs.

`exit` runs before ordinary local destruction caused by leaving the `with` body.

After `exit` completes, the original body result or control-flow outcome continues.

If `enter` does not complete successfully, the pattern is not matched, the `with` body is not evaluated, and `exit` does not run.

The scoped bindings, borrows from them, and capabilities derived from them cannot escape the `with` body unless the selected `exit` contract explicitly transfers that obligation.

A `with` expression participates in ownership, borrowing, mutation authority, initialization, destruction, finalization, capability checking, effect checking, task-obligation checking, and condition refinement.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Scoped use](scoped-use.md)
- Next: [Partial values and replacement](partial-values-and-replacement.md)

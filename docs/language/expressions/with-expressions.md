# With expressions

A **with expression** enters scoped lifecycle behavior for a resource and evaluates a body while the scoped capability is active.

With expressions use `with`.

```bray
with file = File.open(path)
{
    file.write(bytes);
}
```

A with expression has the form:

```bray
with pattern = initializer
{
    ...
}
```

The binding side can include a type annotation:

```bray
with file: File = File.open(path)
{
    file.write(bytes);
}
```

The binding side is an irrefutable pattern.

```bray
with (file, metadata) = open_with_metadata(path)
{
    ...
}
```

The initializer expression is evaluated once.

The initializer must produce a resource value or access path whose type has an applicable `enter` lifecycle declaration.

The selected `enter` lifecycle declaration creates the scoped capability value matched by the pattern.

The type annotation, when present, applies to the scoped capability value produced by `enter`.

The type annotation does not participate in `enter` lifecycle declaration selection.

The scoped bindings introduced by the pattern are visible only inside the with body.

The with body is a block expression.

In value-producing context, the with body is a single-yield region and supplies the with expression result.

```bray
let count: usize = with file = File.open(path)
{
    yield file.count_lines();
};
```

When the with body produces a value, that value is held as the pending with expression result until scope exit completes.

The matching `exit` lifecycle declaration runs on every path leaving the with body.

The `exit` lifecycle declaration runs for normal completion, `yield`, `return`, `break`, `continue`, nullable propagation, result
propagation, run-result propagation, panic propagation, cancellation, and any other control-flow exit from the body.

The scoped capability remains live while `exit` runs.

`exit` runs before ordinary local destruction caused by leaving the with body.

After `exit` completes, the original body result or control-flow outcome continues.

If `enter` does not complete successfully, the pattern is not matched, the with body is not evaluated, and `exit` does not run.

Fallible `enter` or `exit` behavior contributes its failure contract to the with expression.

Asynchronous `enter` or `exit` behavior contributes its execution contract to the with expression.

The surrounding context must be able to satisfy the with expression's type, failure, execution, effect, capability, and lifecycle
contract.

The scoped bindings, borrows from them, and capabilities derived from them cannot escape the with body unless the selected `exit`
lifecycle contract explicitly transfers that obligation.

A `with` expression participates in ownership, borrowing, mutation authority, initialization, destruction, finalization, capability
checking, effect checking, task-obligation checking, and fact-context refinement.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Loop expressions](loop-expressions.md)
- Next: [Lambda expressions and anonymous callable expressions](lambda-expressions-and-anonymous-callable-expressions.md)

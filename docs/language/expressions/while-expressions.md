# While expressions

Syntax follows the [control flow expressions grammar](../syntax-grammar.md#control-flow-expressions).

A while expression is a pre-test loop expression.

The while expression forms are:

```bray
while condition
{
    ...
}
```

```bray
while condition
{
    ...
}
else
{
    ...
}
```

The condition expression is evaluated before each attempted iteration.

`while let pattern = expression` uses a refutable structural condition with the same observing pattern mode and body
scope as [conditional pattern bindings](conditional-expressions.md). Boolean and `let` operands can form an `&&`
chain. Each reached initializer is evaluated once per attempted match, including a reached attempt that fails.
Each successful match establishes bindings for later operands and the body. Every iteration starts with fresh bindings.

```bray
while let Some(value) = iterator.next() && ready(value)
{
    process(value);
}
```

A failed match or false boolean operand is natural loop exhaustion. It selects `else` when present and contributes the same result and
ownership state as an ordinary false condition. Failed attempts introduce no bindings. Body bindings are unavailable
in `else`, the next initializer, and after the loop. `continue` starts a new attempt and `break` bypasses `else`.

The condition expression must have type `bool`.

Bray does not define truthy or falsy conversion for while conditions.

Parentheses around the condition are ordinary expression grouping and are not required by while syntax.

If the condition evaluates to `true`, the body is evaluated once.

If the condition evaluates to `false`, the else body is selected when one is present.

If the condition evaluates to `false` and no else body is present, the while expression completes as `unit`.

After the body reaches its end, the condition is evaluated again.

The syntactic body block of a while expression belongs to the while expression's break-capable region.

The else body belongs to the same break-capable region.

A while body does not capture `yield`. A `yield` inside a while body targets the nearest enclosing yield-capable region
unless a nested yield-capable region captures it.

An else body does not capture `yield`. A `yield` inside an else body targets the nearest enclosing yield-capable region
unless a nested yield-capable region captures it.

`break value` exits the while expression and supplies the while result.

`break;` exits the while expression and supplies `unit`.

`continue` skips the rest of the current body evaluation and starts the next condition evaluation.

`continue` is not valid in the else body.

Loop paths that keep iterating do not supply a while result.

A reachable false-condition exit without an else body contributes `unit`.

An else body that completes naturally contributes `unit`.

If a while expression has result type `unit`, a false-condition exit without an else body is valid.

If a while expression has a result type other than `unit`, each reachable normal exit must use `break` to supply a value
that satisfies [value type acceptance](expression-context.md#value-type-acceptance) for that result type. A path that ends
in a `never` expression supplies no value.

An else body is required when the while expression result type is not `unit` and the false-condition path is reachable.

If the compiler proves the false-condition path unreachable, a missing else body does not contribute a normal path.

The while body is checked with the loop condition known to be true.

The else body is checked with the loop condition known to be false.

No condition is assumed to survive from one iteration to the next merely because it was true in a previous iteration.

Reachable normal while exits follow [result type merging](expression-results.md#result-type-merge) and
[flow-state merging](expression-results.md#flow-state-merge).

The state before each condition evaluation is the flow-state merge of the initial entry and reachable backedges,
including `continue` paths. Every repeated operation must be valid in that merged state.

```bray
while index < items.count()
{
    process(items.at(index));
    index += 1;
}
```

```bray
let found: Item? = while index < items.count()
{
    let item = items.at(index);

    if item.satisfies(query)
    {
        break item;
    }

    index += 1;
}
else
{
    break none;
};
```

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Conditional expressions](conditional-expressions.md)
- Next: [For expressions](for-expressions.md)

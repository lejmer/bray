# While expressions

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

If a while expression has a result type other than `unit`, every reachable normal while exit path must supply a
compatible value with `break` or end in a `never` expression.

An else body is required when the while expression result type is not `unit` and the false-condition path is reachable.

If the compiler proves the false-condition path unreachable, a missing else body does not contribute a normal path.

The while body is checked with the loop condition known to be true.

The else body is checked with the loop condition known to be false.

No condition is assumed to survive from one iteration to the next merely because it was true in a previous iteration.

The type, ownership, initialization, destruction, finalization, capability, effect, task-obligation, and available
contract state after a while expression is the merge of all reachable normal while exits.

The state at the start of a repeated condition evaluation must be coherent with the state before the first condition
evaluation.

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

    if item.matches(query)
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

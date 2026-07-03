# Loop expressions

A loop expression is a break-capable region.

The loop expression form is:

```bray
loop
{
    ...
}
```

A loop expression evaluates its body repeatedly until control leaves the loop.

Reaching the end of the loop body starts the next iteration.

The syntactic body block of a loop belongs to the loop expression's break-capable region.

A loop body does not capture `yield`; a `yield` inside a loop body targets the nearest enclosing yield-capable region unless a
nested yield-capable region captures it.

`break value` exits the loop expression and supplies the loop result.

`break;` exits the loop expression and supplies `unit`.

`continue` skips the rest of the current loop body and starts the next iteration.

Loop paths that keep iterating do not supply a loop result.

If a loop has reachable `break` expressions that target the loop, every such break value must be compatible with the loop's result
type.

`return`, `yield`, panic, nullable propagation, result propagation, run-result propagation, and other exits that target an outer
boundary leave the loop without supplying the loop result.

A loop with no reachable `break` to itself has no normal completion and has type `never`.

Loop expressions do not have an else body because they have no natural exhaustion path.

```bray
let found: Item? = loop
{
    if index >= items.count()
    {
        break none;
    }

    let item = items.at(index);

    if item.matches(query)
    {
        break item;
    }

    index = index + 1;
};
```

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [For expressions](for-expressions.md)
- Next: [With expressions](with-expressions.md)

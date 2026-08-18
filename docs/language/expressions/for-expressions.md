# For expressions

A for expression iterates over a source through the compiler-known `Iterable` and `Iterator` traits.

The for expression forms are:

```bray
for <pattern> in <source>
{
    ...
}

for <pattern> in mut <source>
{
    ...
}

for <pattern> in move <source>
{
    ...
}
```

```bray
for <pattern> in <source>
{
    ...
}
else
{
    ...
}
```

For expression source resolution follows [Iteration source resolution](iteration-source-resolution.md).

The pattern is checked against the source element type.

The pattern must be irrefutable for the source element type.

Each iteration creates fresh bindings from the pattern.

Iteration bindings are scoped to the for body.

Iteration bindings are not visible in the source expression or else body.

Iteration bindings are destroyed or ended at the end of each iteration according to ownership, borrowing, destruction,
and finalization rules.

For each produced element, the pattern is applied and the body is evaluated once.

Reaching the end of the body starts the next iteration step.

The syntactic body block of a for expression belongs to the for expression's break-capable region.

The else body belongs to the same break-capable region.

A for body does not capture `yield`. A `yield` inside a for body targets the nearest enclosing yield-capable region
unless a nested yield-capable region captures it.

An else body does not capture `yield`. A `yield` inside an else body targets the nearest enclosing yield-capable region
unless a nested yield-capable region captures it.

`break value` exits the for expression and supplies the for result.

`break;` exits the for expression and supplies `unit`.

`continue` skips the rest of the current body evaluation and starts the next iteration step.

`continue` is not valid in the else body.

If iteration reaches natural exhaustion, the else body is selected when one is present.

If iteration reaches natural exhaustion and no else body is present, the for expression completes as `unit`.

Natural exhaustion does not occur on a path that exits through `break`, `return`, `yield`, panic, nullable propagation,
result propagation, run-result propagation, or another outer-boundary exit.

An else body that completes naturally contributes `unit`.

If a for expression has result type `unit`, natural exhaustion without an else body is valid.

If a for expression has a result type other than `unit`, every reachable normal for exit path must supply a compatible
value with `break` or end in a `never` expression.

An else body is required when the for expression result type is not `unit` and natural exhaustion is reachable.

If the compiler proves natural exhaustion unreachable, a missing else body does not contribute a normal path.

The type, ownership, initialization, destruction, finalization, capability, effect, task-obligation, and available
contract state after a for expression is the merge of all reachable normal for exits.

Conditions tied to an iteration binding expire at the end of that iteration unless they are transferred into another
surviving storage location.

```bray
for item in items
{
    process(item);
}
```

```bray
let found: Item? = for item in items
{
    if item.matches(query)
    {
        break item;
    }
}
else
{
    break none;
};
```

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [While expressions](while-expressions.md)
- Next: [Loop expressions](loop-expressions.md)

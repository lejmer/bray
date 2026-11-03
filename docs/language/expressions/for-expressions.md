# For expressions

Syntax follows the [control flow expressions grammar](../syntax-grammar.md#control-flow-expressions).

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

The pattern is checked against the source element type and must be irrefutable for that type.
Each iteration creates fresh bindings scoped to the for body, which are not visible in the source expression or else body.
The bindings are destroyed or ended at the end of each iteration according to ownership, borrowing, destruction, and
finalization rules.

For each produced element, the pattern is applied and the body is evaluated once. Reaching the end of the body starts
the next iteration step.

The for body and else body belong to the for expression's break-capable region. Neither body captures `yield`. A yield
inside either body targets the nearest enclosing yield-capable region unless a nested yield-capable region captures it.

`break value` exits the for expression and supplies the for result.

`break;` exits the for expression and supplies `unit`.

`continue` skips the rest of the current body evaluation and starts the next iteration step.

`continue` is not valid in the else body.

On natural exhaustion, the else body is selected when present. Natural exhaustion without an else body completes the
for expression as `unit`. An else body that completes naturally also contributes `unit`.

Natural exhaustion does not occur on a path that exits through `break`, `return`, `yield`, panic, nullable propagation,
result propagation, run-result propagation, or another outer-boundary exit.

If a for expression has result type `unit`, natural exhaustion without an else body is valid.

If a for expression has a result type other than `unit`, each reachable normal exit must use `break` to supply a value
that satisfies [value type acceptance](expression-context.md#value-type-acceptance) for that result type. A path that ends
in a `never` expression supplies no value.

An else body is required when the for expression result type is not `unit` and natural exhaustion is reachable.

If the compiler proves natural exhaustion unreachable, a missing else body does not contribute a normal path.

Reachable normal for exits follow [result type merging](expression-results.md#result-type-merge) and
[flow-state merging](expression-results.md#flow-state-merge).

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
    if item.satisfies(query)
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

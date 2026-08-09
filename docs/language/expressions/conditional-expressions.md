# Conditional expressions

A conditional expression selects one branch according to a boolean condition.

The conditional expression forms are:

```bray
if condition
{
    ...
}
```

```bray
if condition
{
    ...
}
else
{
    ...
}
```

```bray
if condition
{
    ...
}
else if other_condition
{
    ...
}
else
{
    ...
}
```

The condition expression is evaluated exactly once.

The condition expression must have type `bool`.

Bray does not define truthy or falsy conversion for conditional conditions.

Parentheses around the condition are ordinary expression grouping and are not required by conditional syntax.

The then body and else body are block expressions.

Only the selected body is evaluated.

If the condition evaluates to `true`, the then body is selected.

If the condition evaluates to `false`, the else body is selected when one is present.

If the condition evaluates to `false` and no else body is present, the conditional expression completes as `unit`.

`else if` is syntactic nesting of another conditional expression in the else body.

When a conditional expression is used in value-producing context, each selected body is a single-yield region.

If the conditional expression result type is `unit`, a selected body can complete normally.

If the conditional expression result type is a value type other than `unit`, every reachable normal completion path in every
selected body must supply a value with `yield` or end in a `never` expression.

An else body is required when the conditional expression result type is not `unit` and the false path is reachable.

If the compiler proves the false path unreachable, a missing else body does not contribute a normal path.

All reachable normal branch exits must merge to a coherent type, ownership state, initialization state, destruction state,
finalization state, capability state, effect state, task-obligation state, and set of available contract guarantees.

A `never` branch does not contribute a value to the merged result type.

Bindings introduced inside a branch body are scoped to that branch body.

The then body receives the condition that the condition is true.

The else body receives the condition that the condition is false.

For an `else if` chain, each later condition is checked with every earlier condition in the chain known to be false.

Conditions established inside a branch body contribute after the conditional expression only when they are established by every
reachable normal branch exit and remain valid after the merged ownership and mutation state.

If a branch moves, destroys, initializes, finalizes, cancels, transfers, or changes capability state, the merged state after the
conditional expression must account for that change on every reachable normal branch path.

```bray
let grade: Grade = if score >= 90
{
    yield A;
}
else if score >= 80
{
    yield B;
}
else
{
    yield C;
};
```

```bray
if ready
{
    start();
}
```

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Result and run result propagation expressions](result-and-run-result-propagation-expressions.md)
- Next: [While expressions](while-expressions.md)

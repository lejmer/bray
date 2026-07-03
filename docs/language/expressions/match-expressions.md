# Match expressions

A **match expression** evaluates a subject expression, compares it against a sequence of pattern arms, and produces the result of the selected arm.

```bray
let area: r64 = match shape
{
    case Circle(radius)
    {
        yield math.pi * radius * radius;
    }

    case Rectangle(min, max)
    {
        yield (max.x - min.x) * (max.y - min.y);
    }

    case Empty
    {
        yield 0.0;
    }
};
```

A match expression evaluates its subject once.

A match body contains `case` arms.

Each arm has a pattern and a block expression body.

An arm can have a `when` guard.

```bray
match subject
{
    case pattern
    {
        ...
    }

    case pattern when guard
    {
        ...
    }
}
```

A guard is a boolean expression evaluated in guard context after the arm pattern structurally matches and before the arm body is selected.

Bindings introduced by an arm pattern are available in the guard and in the arm body.

In the guard, pattern bindings are available for observation.

In the arm body, pattern bindings are available according to the match operation mode.

The first arm whose pattern matches and whose guard holds is selected.

A catch-all arm uses the discard pattern.

```bray
case _
{
    yield fallback;
}
```

The selected arm body produces the match expression result.

If the match expression result type is `unit`, an arm body can complete normally.

If the match expression result type is a value type other than `unit`, every reachable normal completion path in every selected arm body supplies a value with `yield` or ends in a `never` expression.

All match arms must merge to a coherent type, ownership state, initialization state, destruction state, finalization state, capability state, and fact context.

A match expression can use refutable patterns.

A match expression over a closed union performs coverage checking against the union’s closed variant set.

A match expression over a nullable value performs coverage checking over absent state and present contained values.

Coverage analysis tracks the pattern coverage region for each arm.

An unguarded arm contributes its whole pattern coverage region.

A guarded arm contributes only the subregion where the guard is statically proven true by the shared fact and predicate system.

If the guard is statically proven false for the arm's pattern facts, the arm is unreachable.

If the guard truth is statically unknown for some part of the arm's pattern coverage region, that part can still select the arm at runtime, but it does not contribute to exhaustiveness.

Alternative patterns contribute coverage for each alternative.

Arm order is semantically meaningful.

Later arms are checked against the subject space not already definitely covered by earlier arms.

A later arm whose pattern can never be selected is unreachable.

A successful arm pattern refines the fact context for the guard and the arm body.

For union variants, refinement includes the active variant and initialized payload fields.

For nullable patterns, refinement includes present state for `?pattern` arms and absent state for `none` arms.

The default match operation mode is observe.

A consuming match uses consume mode.

```bray
let bytes = match consume buffer
{
    case Inline(data)
    {
        yield data;
    }

    case Heap(data)
    {
        yield data;
    }
};
```

In consume mode, selected payloads and fields can be moved out according to ownership rules.

A consuming match with guards evaluates structural matching and guards through observation first.

Consuming bindings are produced for the selected arm body after the guard holds.

A match expression can match through borrowed or type-form subjects when the subject type and pattern form support it.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Type-form construction expressions](type-form-construction-expressions.md)
- Next: [Yield expressions](yield-expressions.md)

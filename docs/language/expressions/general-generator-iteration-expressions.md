# General generator iteration expressions

A **general generator iteration expression** iterates over a source inside a multi-yield generator region.

The iteration form is:

```bray
each <pattern> in <source>
{
    ...
}
```

A general generator iteration expression is valid inside a generator region that accepts zero or more yielded values.

General generator source resolution follows [Iteration source resolution](iteration-source-resolution.md).

The `pattern` is checked against the source element type.

The pattern must be irrefutable for the source element type.

Each iteration creates fresh bindings from the pattern.

Iteration bindings are scoped to the iteration body.

Iteration bindings are not visible in the source expression.

Iteration bindings are destroyed or ended at the end of each iteration according to ownership, borrowing, destruction, and finalization rules.

The iteration body is a block expression in generator-iteration context.

Generator-iteration context is yield-capable when an enclosing generator region accepts yielded values.

A general generator iteration body can yield zero or more values unless the enclosing generator region imposes a stricter cardinality rule.

Each yielded value is supplied to the nearest enclosing generator region.

A yielded value must be compatible with the enclosing generator’s expected element type when one is known.

Expected generator element type can guide literal typing, variant shorthand, struct construction shorthand, box construction shorthand, conversion checking, and nested expression checking inside yielded expressions.

Nested yield-capable regions capture their own yields.

An inner `yield` supplies the inner yield-capable region.

A `yield` in the generator iteration body supplies the nearest enclosing generator region that the `yield` targets.

`continue` targets the nearest iteration region.

`break` targets the nearest iteration region and exits that iteration expression.

Because general generator iteration expressions complete as `unit`, a break that targets the iteration expression must supply
`unit`.

A general generator iteration expression can have unknown or runtime cardinality when the enclosing generator region accepts variable cardinality.

A general generator iteration expression must have statically provable cardinality when the enclosing generator region requires statically known cardinality.

Array generator regions require statically provable cardinality matching the array length.

Effects of the source expression occur once before iteration.

Effects of the iteration body occur once per executed iteration.

Finalization obligations created inside an iteration body must be completed, transferred, converted into an explicit fallback ownership form, or moved into yielded values before the iteration body exits.

Conditions established by the source expression, pattern, and iteration body are scoped according to the iteration region.

Conditions tied to an iteration binding expire at the end of that iteration unless they are transferred into a yielded value or another surviving storage location.

A general generator iteration expression participates in ownership, borrowing, mutation authority, initialization, destruction, finalization, capability checking, effect checking, and condition refinement.

The completion result of a generator iteration expression is `unit`.

The values produced by `yield` are delivered to the enclosing generator region rather than becoming the direct completion result of the iteration expression.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [General generator expressions](general-generator-expressions.md)
- Next: [Boolean fold expressions](boolean-fold-expressions.md)

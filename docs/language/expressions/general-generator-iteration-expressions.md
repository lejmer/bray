# General generator iteration expressions

Syntax follows the [generator expressions grammar](../syntax-grammar.md#generator-expressions).

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

The pattern is checked against the source element type and must be irrefutable for that type.
Each iteration creates fresh bindings scoped to the iteration body, which are not visible in the source expression.
The bindings are destroyed or ended at the end of each iteration according to ownership, borrowing, destruction, and
finalization rules.

The iteration body is a block expression in generator-iteration context, which is yield-capable when an enclosing
generator region accepts yielded values. The body can yield zero or more values unless that region imposes a stricter
cardinality rule. Each yielded value is supplied to the nearest enclosing generator region.

A yielded value must satisfy [value type acceptance](expression-context.md#value-type-acceptance) for the enclosing generator's
expected element type when one is known.

Expected generator element type can guide literal typing, variant shorthand, struct construction shorthand, box
construction shorthand, conversion checking, and nested expression checking inside yielded expressions.

Nested yield-capable regions capture their own yields. An inner `yield` supplies the inner region, while a `yield` in the
generator iteration body supplies the nearest enclosing generator region that it targets.

`continue` targets the nearest iteration region. `break` exits that region and must supply `unit`, because general
generator iteration expressions complete as `unit`. Yielded values go to the enclosing generator region rather than
becoming the iteration expression's direct completion result.

The enclosing region determines the cardinality requirement. A region accepting variable cardinality permits unknown
or runtime cardinality. A region requiring statically known cardinality requires a compiler proof. For array generator
regions, that proof must establish a count matching the array length.

Effects of the source expression occur once before iteration.

Effects of the iteration body occur once per executed iteration.

Finalization obligations created inside an iteration body must be completed, transferred, converted into an explicit
fallback ownership form, or moved into yielded values before the iteration body exits.

Conditions established by the source expression, pattern, and iteration body are scoped according to the iteration
region.

Conditions tied to an iteration binding expire at the end of that iteration unless they are transferred into a yielded
value or another surviving storage location.

The selected iteration operations and pattern mode determine ownership, borrowing, mutation authority, and
initialization of the source, cursor, and bindings. Their cleanup follows destruction and finalization rules. The
compiler checks their effects, capabilities, and conditions under [iteration source resolution](iteration-source-resolution.md)
and [pattern operation modes](../patterns/pattern-operation-modes.md).

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [General generator expressions](general-generator-expressions.md)
- Next: [Boolean fold expressions](boolean-fold-expressions.md)

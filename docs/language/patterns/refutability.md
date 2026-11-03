# Refutability

A pattern is **irrefutable** when its structure establishes that matching cannot fail for its subject type under the
rules below. A valid pattern that is not classified as irrefutable is **refutable**. This classification is separate
from exhaustiveness and flow-sensitive facts about the particular subject value.

Examples of irrefutable patterns:

```bray
_
value
mut value
{ x, y }
(x, y)
```

Binding and discard patterns are irrefutable. A remaining-fields or remaining-elements marker adds no failing test.
A compatible product, tuple, or fixed-size array pattern is irrefutable when it accounts for the subject's complete
shape and every selected subpattern is irrefutable. A compatible variant pattern is irrefutable only when its variant
is the union's sole variant and every payload subpattern is irrefutable. Grouping and box patterns inherit their
inner pattern's classification.

Examples of refutable patterns:

```bray
0
true
Circle(radius, ..)
Empty
none
?value
Error | Cancelled
```

Literal patterns, constant path patterns, nullable patterns, and alternative patterns are always classified as
refutable. A proof that a particular subject will match, or that alternatives collectively cover its type, does not
make these forms irrefutable.

A pattern context declares whether it accepts refutable patterns.

`matches`, `if let`, and `while let` accept refutable patterns without requiring exhaustive coverage. A failed
`matches` test returns `false`. A failed conditional binding selects the false branch or natural loop exhaustion.
Irrefutable patterns are also valid in these contexts, subject to the prohibition on bindings in `matches`.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Alternative patterns](alternative-patterns.md)
- Next: [Pattern contexts](pattern-contexts.md)

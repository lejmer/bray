# Refutability

A pattern is **irrefutable** when it matches every value of its subject type.

A pattern is **refutable** when it matches only some values of its subject type.

Examples of irrefutable patterns:

```bray
_
value
mut value
{ x, y }
(x, y)
```

A product or tuple pattern is irrefutable when its subpatterns are irrefutable.

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

A variant pattern is refutable when the subject union has other variants.

A literal pattern is refutable when the subject type has other possible values.

The `none` pattern and `?inner` pattern are refutable because a nullable value can be absent or present.

A pattern context declares whether it accepts refutable patterns.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Alternative patterns](alternative-patterns.md)
- Next: [Pattern contexts](pattern-contexts.md)

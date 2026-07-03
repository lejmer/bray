# Modifiers

A **modifier** is a keyword in a declaration header that changes the declaration surface or checking context.

Common declaration modifiers include:

- `public`,
- `internal`,
- `trusted`,
- `extern`,
- `const`,
- `async`,
- `static`,
- `consume`,
- `mut`.

Modifier validity depends on the declaration form and declaration context.

`public` and `internal` are visibility modifiers.

`trusted` marks declarations or declaration contexts that participate in trusted capability and trusted obligation rules.

`extern` marks a callable declaration whose implementation is supplied outside Bray source.

`const` marks a callable body as valid in constant-evaluation context.

`async` marks a callable or lifecycle declaration as asynchronous where that declaration form permits asynchronous execution.

`static`, `consume`, and `mut` participate in method receiver selection when used on callable member declarations.

`mut` before a parameter name marks a local owned parameter binding as mutable.

`mut` after `&` belongs to the borrow type form and marks mutation authority over the reached storage.

Duplicate modifiers are rejected.

Modifiers that are not valid for the declaration form are rejected.

Incompatible modifier combinations are rejected.

When grammar accepts modifiers in more than one source order, the semantic declaration surface is the same regardless of source order.

Where a declaration form defines a canonical modifier order, formatters and generated source use that order.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Directives](directives.md)
- Next: [Generic declarations and constraints](generic-declarations-and-constraints.md)

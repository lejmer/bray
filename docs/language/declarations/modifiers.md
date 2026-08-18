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

`trusted` marks declarations or declaration contexts that participate in trusted capability and trusted obligation
rules.

`extern` marks a declaration whose runtime definition or storage is supplied outside that Bray declaration. An extern
function has no Bray body. An extern static has provider-owned storage and no Bray initializer. The providing artifact
can itself contain separately compiled Bray. The selected ABI and symbol contract determine whether the boundary is
foreign.

`const` marks a callable body as valid in constant-evaluation context.

`async` marks a callable or lifecycle declaration as asynchronous where that declaration form permits asynchronous
execution.

`static` introduces address-bearing storage at module level. With `extern`, the provider owns that storage. In callable
member declarations `static` selects a static function with no receiver. `consume` and `mut` participate in method
receiver selection.

`mut` after `static` declares storage whose native symbol contract permits external mutation when suitable authority is
independently established. It grants no ambient Bray mutation authority. This form is valid only for extern or
native-symbol statics.

`mut` before a parameter name marks a local owned parameter binding as mutable.

`mut` after `&` belongs to the borrow type form and marks mutation authority over the reached storage.

Duplicate modifiers are rejected.

Modifiers that are not valid for the declaration form are rejected.

Incompatible modifier combinations are rejected.

When grammar accepts modifiers in more than one source order, the semantic declaration surface is the same regardless of
source order.

Where a declaration form defines a required modifier order, formatters and generated source use that order.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Directives](directives.md)
- Next: [Generic declarations and constraints](generic-declarations-and-constraints.md)

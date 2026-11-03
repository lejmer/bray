# Overview

An **expression** is a program element that produces a value, an access path, a control-flow outcome, or a compile-time
entity.

Bray is expression-oriented. Blocks, construction, calls, ranges, matches, generators, boolean folds, `with`,
conversions, borrows, assignments, and control operations are expression forms with types. Their use of values and
storage follows ownership, borrowing, mutation-authority, and initialization rules. Their completion can create
destruction or finalization obligations, and their effects, capabilities, and conditions constrain their use.
[Expression results](expression-results.md) and [expression context](expression-context.md) define these relationships.

Expressions are distinct from declarations and directives. A [declaration](../declarations/overview.md) introduces a
program entity, a member, or a semantic relationship. A [directive](../declarations/directives.md) starts with `@` and
is a compile-time instruction affecting interpretation, checking, target behavior, or build behavior.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Next: [Expression results](expression-results.md)

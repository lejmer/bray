# Expression effects and capabilities

The compiler checks an expression's effects and required capabilities against the authority available at that program
point.

An expression can use only the effects and capabilities available from:

- bindings,
- parameters,
- constraints,
- execution mode,
- lifecycle state,
- trusted declarations,
- contract guarantees available at the program point,
- surrounding context.

The operation determines which authority is required. Reading storage uses the observe or observe-with-internal-effects
capability, changing storage requires mutation authority, and consuming a value requires consume capability under
[borrow rules](../ownership-and-borrowing/borrow-rules.md). Finalization and scope entry or exit require the capabilities
specified by the selected [lifecycle operation](../lifecycle/lifecycle-selection.md). Async operations must satisfy their
[execution requirements](../async-and-concurrency/execution-requirements.md).

An expression that uses a trusted implementation capability must appear inside a trusted declaration with the matching
`uses(...)` clause.

An expression that depends on a trusted caller obligation must have that obligation at that program point, acknowledge
it at a trust boundary, or expose it through the surrounding declaration’s contract.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous:
  [Lambda expressions and anonymous callable expressions](lambda-expressions-and-anonymous-callable-expressions.md)
- Next: [Expression ownership](expression-ownership.md)

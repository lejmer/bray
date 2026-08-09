# Expression effects and capabilities

Expression checking includes effects and capabilities.

An expression can use only the effects and capabilities available from:

- bindings,
- parameters,
- constraints,
- execution mode,
- lifecycle state,
- trusted declarations,
- contract guarantees available at the program point,
- surrounding context.

Expressions participate in:

- observe capability,
- observe-with-internal-effects capability,
- mutation authority,
- consume capability,
- finalization capability,
- trusted capability checking,
- scope enter capability,
- scope exit capability,
- async execution capability.

An expression that uses a trusted implementation capability must appear inside a trusted declaration with the matching `uses(...)` clause.

An expression that depends on a trusted caller obligation must have that obligation at that program point, acknowledge it at a trust boundary, or expose it through the surrounding declaration’s contract.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Lambda expressions and anonymous callable expressions](lambda-expressions-and-anonymous-callable-expressions.md)
- Next: [Expression ownership](expression-ownership.md)

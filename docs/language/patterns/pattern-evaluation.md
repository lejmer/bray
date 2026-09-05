# Pattern evaluation

Pattern matching is structural, deterministic, and effect-free.

Pattern matching can inspect tags, fields, tuple elements, array elements, nullable state, and type-form structure
according to the subject type and operation mode.

Pattern matching can bind names, make structural guarantees available, and move, copy, or borrow parts according to
ownership rules.

Ordinary user code is outside pattern matching. User-defined equality, method calls, allocation, I/O, async execution,
finalization, and resource scopes belong to surrounding expressions or guards.

`matches` evaluates its subject once before testing the pattern. In an `if` or `while` condition, `&&` operands
evaluate left to right until a boolean is false or a pattern fails. Each reached `let` initializer runs once.
Later operands can use bindings from earlier successful patterns. Later `else if` initializers run only when earlier
conditions fail. Observed initializer temporaries remain alive through later operands and the selected body.
They are destroyed when the chain fails or the body exits under the ordinary lifetime rules.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Guards](guards.md)
- Next: [Summary](summary.md)

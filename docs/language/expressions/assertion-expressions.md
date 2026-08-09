# Assertion expressions

An **assertion expression** checks an ordinary boolean condition at runtime.

The syntax is:

```text
'assert' '(' boolean-expression [',' message-expression] ')'
```

The condition is an expression that must produce `bool`.

The message, when present, is an expression compatible with `string`.

Examples:

```bray
assert(index < length);
assert(index < length, "index out of bounds");
```

The condition expression is evaluated first.

If the condition evaluates to `true`, the assertion expression completes normally with `unit`.

If the condition evaluates to `false`, the assertion fails and panics.

When a failing assertion has a message expression, the message expression is evaluated after the condition fails and before the
panic is raised.

When a passing assertion has a message expression, the message expression is not evaluated.

When a failing assertion has no message expression, the panic report uses the compiler-defined assertion-failure message.

An assertion expression has type `unit` on its normal continuation.

On the failing path, the assertion expression has no normal continuation.

After a successful assertion, the asserted guarantees are available when they remain valid after the
assertion expression.

An assertion establishes ordinary guarantees, not trusted guarantees.

Condition-context rules are defined in [Contract reasoning](../contracts-and-trust/contract-reasoning.md).

An assertion expression does not satisfy trusted caller obligations and does not grant trusted implementation capabilities.

Assertion expressions participate in type checking, ownership checking, borrowing, mutation authority, effect checking,
capability checking, trusted obligation checking, and condition refinement according to the expressions they evaluate.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Trust boundary expressions](trust-boundary-expressions.md)
- Next: [Nullable and absence expressions](nullable-and-absence-expressions.md)

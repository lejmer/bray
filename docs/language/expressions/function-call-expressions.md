# Function call expressions

A **function call expression** calls a callable declaration or callable value.

```bray
add(left = 1, right = 2)
print("hello")
```

Function call arguments follow the [argument binding](arguments.md) rules.

A function call with a positional argument for a non-`pos` parameter is rejected.

```bray
add(1, 2)
```

A function call expression has a callee and an argument list.

The callee must resolve to a callable declaration or callable value.

The argument list supplies argument expressions to callable parameters by name or by permitted position.

Function call argument validation, missing default handling, and parameter-context checking follow the [argument binding](arguments.md) rules.

A callable parameter can receive an owned value, copied value, shared borrow, mutable borrow, consumed value, or other allowed argument form according to its parameter contract.

If a parameter takes an owned value, the corresponding argument is moved into the call unless the argument type is copyable or another explicit rule applies.

If a parameter takes a shared borrow, the corresponding argument must provide a compatible observable access path or borrow value.

If a parameter takes a mutable borrow, the corresponding argument must provide mutation authority and compatible exclusivity for the reached storage.

If a parameter consumes a value, the argument’s old access path becomes unavailable after the call unless reinitialized.

A call expression produces the callable’s declared result.

A call to a callable returning `unit` produces `unit`.

A call to a callable returning `never` has no normal continuation.

A call to an async callable produces an owned async computation.

Catch behavior for task and thread joins is defined in [Catch expressions](catch-expressions.md).

A call expression can use ordinary contract facts from the fact context to satisfy `requires(...)`.

A call expression can use trusted facts from the fact context to satisfy trusted requirements.

A call expression that needs a trusted caller obligation must have the obligation established in the fact context, explicitly acknowledged at a trust boundary, or exposed through the surrounding declaration’s contract.

A call expression can establish facts from the callable’s `ensures(...)` clause after successful completion.

Facts established by a call are tied to the values, storage identities, lifetimes, capabilities, and versions referenced by the ensures clause.

A call expression participates in overload resolution when the callee resolves to an overload declaration.

A call resolves to exactly one callable after name resolution, argument binding, type checking, ownership checking, capability checking, effect checking, contract checking, and overload resolution.

Overload resolution uses only arguments explicitly supplied by the caller.

Default arguments do not make an overload arm selectable.

Result type and expected type do not participate in overload resolution.

Ambiguous calls are rejected.

Call expression evaluation order is defined by the general expression evaluation order rules.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Borrow expressions](borrow-expressions.md)
- Next: [Arguments](arguments.md)

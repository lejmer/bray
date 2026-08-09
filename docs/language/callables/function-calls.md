# Function calls

A function call evaluates the callee and arguments according to Bray evaluation order.

```bray
let result = add(left = 1, right = 2);
print("hello");
```

Evaluation-order rules are defined by expression evaluation order.

A call is valid when every argument satisfies the corresponding parameter's type, ownership, borrowing, mutation, lifetime,
capability, effect, and contract requirements.

## Parameter evaluation and ownership transfer

At a call site, each argument is checked against its corresponding parameter.

Depending on the parameter, the argument may be:

- observed,
- borrowed,
- mutably borrowed,
- moved,
- copied,
- consumed,
- used to establish a contract guarantee.

Owned parameters take ownership of the argument value unless the value is copyable or another explicit rule applies.

Borrow parameters create borrow access paths.

Mutable borrow parameters require mutation authority.

Consumed values become unavailable through their old access paths unless reinitialized.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Parameters](parameters.md)
- Next: [Return and execution scopes](return-and-execution-scopes.md)

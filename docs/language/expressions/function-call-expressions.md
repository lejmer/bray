# Function call expressions

A **function call expression** calls a callable declaration or callable value.

```bray
add(left = 1, right = 2)
print("hello")
```

Function call arguments follow the [argument binding](arguments.md) rules.

Callable parameter surfaces, parameter transfer, overload selection, and caller-visible callable contracts are defined in [Function calls](../callables/function-calls.md).

A function call with a positional argument for a non-`pos` parameter is rejected.

```bray
add(1, 2)
```

A function call expression has a callee and an argument list.

An explicit generic argument list appears after the callee and before the call argument list.

```bray
identity<i32>(10)
element_count<u8, 4>(&bytes)
```

The callee must resolve to a callable declaration or callable value.

The argument list supplies argument expressions to callable parameters by name or by permitted position.

Function call argument validation, missing default handling, parameter-context checking, ownership transfer, and borrowing checks follow the [argument binding](arguments.md) rules and the selected callable contract.

A call to a synchronous callable produces the callable's declared result.

A call to a callable returning `unit` produces `unit`.

A call to a callable returning `never` has no normal continuation.

A call to an async callable whose declared result is `T` produces an owned `Future<T>`. The async body is still checked as producing
`T`. Invocation checks the callable's invocation contract and transfers its execution contract into that computation.

A call expression can use ordinary and trusted facts from the fact context to satisfy the selected callable contract.

A synchronous call expression can establish facts from the callable's `ensures(...)` clause after successful completion. Constructing
an `Future<T>` establishes no body postcondition. The postconditions of an async callable become available only after direct await
completes normally, or within a `RunResult.Completed(value)` refinement after observing a started task. Postconditions mentioning
`result` describe that completed value.

Argument evaluation, argument transfer, generic constraints, and value preconditions are invocation-time behavior for both sync and
async calls. Effects, capabilities, execution requirements, and lifecycle behavior of an async body are execution-time behavior.
They are carried by the produced `Future<T>` and checked when it is directly awaited, started, or resolved during cleanup. Merely
constructing the frame is charged only for invocation-time behavior such as argument evaluation, ownership transfer, and any frame
storage operation required by the selected representation.

A call expression participates in overload resolution when the callee resolves to an overload declaration.

A call first selects exactly one callable through name resolution, argument mapping and type compatibility, explicit generic
substitution and static constraints, target availability, and overload resolution. Ordinary call checking then validates ownership,
borrowing, mutation authority, dependency contracts, capabilities, effects, trusted obligations, and contract facts for that
selected callable.

Overload resolution rules are defined in [Function overloading](../callables/function-overloading.md).

Call expression evaluation order is defined by the general expression evaluation order rules.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Borrow expressions](borrow-expressions.md)
- Next: [Arguments](arguments.md)

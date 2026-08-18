# Static function call expressions

A **static function call expression** calls a type-level function associated with a type, trait application,
implementation, module, package, or other path-capable entity.

```bray
Point.origin()
Buffer.from_bytes(bytes)
Buffer.with_capacity<u8, 16>()
math.sin(angle)
```

A static function call has no `self` receiver.

Static functions are declared with `static func` inside trait and implementation blocks.

```bray
static func origin() -> Point
{
    return Point { x = 0.0, y = 0.0, };
}
```

A static function call expression has a callee path and an argument list.

An explicit generic argument list appears after the callee path and before the call argument list.

The callee path must resolve to a static callable declaration or callable value.

Static function parameters follow the [argument binding](arguments.md) rules.

Static function calls use the callable contract rules defined in [Callables](../callables.md).

A static function call with a positional argument for a non-`pos` parameter is rejected.

```bray
lookup(table)
```

Static function call argument validation, missing default handling, and parameter-context checking follow the
[argument binding](arguments.md) rules.

A synchronous static function call produces the static function's declared result.

A call to an async static function whose declared result is `T` produces an owned `Future<T>`.

A static function call can use ordinary and trusted guarantees available at that program point to satisfy preconditions.

A synchronous static function call makes the static function's `ensures(...)` guarantees available after successful
completion. An async static function establishes those conditions only after normal direct-await completion or within
the `RunResult.Completed` arm after task observation. Constructing its `Future<T>` establishes no body postcondition and
carries body effects, capabilities, execution requirements, and lifecycle behavior until execution.

A static function call participates in overload resolution when the path resolves to an overload declaration.

A static function call first selects exactly one callable through path resolution, argument mapping and type
compatibility, explicit generic substitution and static constraints, target availability, and overload resolution.
Ordinary call checking then validates ownership, borrowing, mutation authority, dependency contracts, capabilities,
effects, trusted obligations, and contract guarantees for that selected callable.

Static callee path resolution is a checking step and has no runtime evaluation order.

Static function argument expressions and omitted parameter defaults follow the same evaluation-order rules as function
calls.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Unary and binary expressions](unary-and-binary-expressions.md)
- Next: [Tuple expressions](tuple-expressions.md)

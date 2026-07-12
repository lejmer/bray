# Static function call expressions

A **static function call expression** calls a type-level function associated with a type, trait application, implementation, module, package, or other path-capable entity.

```bray
Point.origin()
Buffer.from_bytes(bytes)
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

The callee path must resolve to a static callable declaration or callable value.

Static function parameters follow the [argument binding](arguments.md) rules.

Static function calls use the callable contract rules defined in [Callables](../callables.md).

A static function call with a positional argument for a non-`pos` parameter is rejected.

```bray
lookup(table)
```

Static function call argument validation, missing default handling, and parameter-context checking follow the [argument binding](arguments.md) rules.

A static function call produces the static function’s declared result.

A static function call to an async static function produces an owned async computation.

A static function call can use ordinary and trusted facts from the fact context to satisfy preconditions.

A static function call can establish facts from the static function’s `ensures(...)` clause after successful completion.

A static function call participates in overload resolution when the path resolves to an overload declaration.

A static function call first selects exactly one callable through path resolution, argument mapping and type compatibility, explicit
generic substitution and static constraints, target availability, and overload resolution. Ordinary call checking then validates
ownership, borrowing, mutation authority, dependency contracts, capabilities, effects, trusted obligations, and contract facts for
that selected callable.

Static callee path resolution is a checking step and has no runtime evaluation order.

Static function argument expressions and omitted parameter defaults follow the same evaluation-order rules as function calls.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Unary and binary expressions](unary-and-binary-expressions.md)
- Next: [Tuple expressions](tuple-expressions.md)

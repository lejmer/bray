# Overload declarations

An overload declaration introduces an explicit overload family.

Same-name declarations do not automatically form overload sets.

Callable overload declarations group separately named callable declarations under one shared call surface.

```bray
func parse_int(pos text: string, radix: i32 = 10) -> i64
{
    ...
}

func parse_float(pos text: string) -> r64
{
    ...
}

overload parse =
{
    parse_int,
    parse_float,
}
```

Each overload arm keeps its own declaration identity and can still be referenced directly.

Calls through the overload name use overload selection.

Callable overload selection rules are defined in [Function overloading](../callables/function-overloading.md).

Implementation overload declarations group named trait implementation declarations under a shared subject and trait surface.

Implementation overload selection rules are defined in [Implementations](../types/implementations.md).

Overload declarations do not rank candidates.

If more than one arm remains applicable, the use is ambiguous and rejected.

Result type and expected type do not select overload arms.

Default arguments do not make a callable overload arm selectable.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Predicate declarations](predicate-declarations.md)
- Next: [Declaration order and checking](declaration-order-and-checking.md)

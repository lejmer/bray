# Arguments

An **argument** is an expression supplied to a callable parameter, constructor parameter, method parameter, static
function parameter, named constructor parameter, or another callable-like parameter.

```bray
left = 1
right = 2
mode = FileMode.write
buffer
```

Arguments are named by default in callable calls.

Callable parameter surfaces and `pos` parameter rules are defined in [Parameters](../callables/parameters.md).

A named argument identifies the parameter being supplied by name.

A positional argument identifies the parameter being supplied by position and is valid only for a parameter marked
`pos`.

The argument expression supplies the value or access path checked against that parameter.

```bray
add(left = 1, right = 2)
print("hello")
```

An argument supplied in named form writes the parameter name explicitly.

Named argument order does not determine parameter binding.

Positional argument order determines binding to the positional parameter prefix.

Positional arguments must appear before named arguments.

Argument order is source order for evaluation.

Named argument order does not change which parameter receives which named argument.

Duplicate arguments for the same parameter are errors.

Arguments for parameters that do not exist are errors.

A required parameter without a supplied argument and without a default is an error.

An argument expression is checked in the expected context of the corresponding parameter.

Expected parameter context can guide literal typing, variant shorthand, struct construction shorthand, box construction
shorthand, tuple element typing, array element typing, and conversion checking.

```bray
draw(Circle(center = origin, radius = 1.0))
```

Here the `pos shape` parameter type can provide the expected union type for `Circle(...)`.

Arguments can supply owned values.

```bray
consume(buffer)
```

Arguments can supply shared borrows.

```bray
read(&buffer)
```

Arguments can supply mutable borrows.

```bray
fill(&mut buffer)
```

Arguments can supply values constructed inline.

```bray
draw(Point { x = 1.0, y = 2.0 })
```

Argument expressions participate in ownership, borrowing, mutation authority, initialization, destruction, finalization,
capability checking, effect checking, and condition refinement.

A function call, method call, or static function call cannot use positional syntax to satisfy parameters that are not
marked `pos`.

Tuple expressions and array expressions remain positional structural expressions because their positions are the
structure being constructed, not callable parameter binding.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Function call expressions](function-call-expressions.md)
- Next: [Defaulted arguments](defaulted-arguments.md)

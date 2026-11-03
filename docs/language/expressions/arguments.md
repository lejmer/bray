# Arguments

Syntax follows the [argument lists grammar](../syntax-grammar.md#argument-lists).

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

A named argument identifies the parameter being supplied by name. A positional argument identifies it by position and
is valid only for a parameter marked `pos`. The argument expression supplies the value or access path checked against
that parameter.

```bray
add(left = 1, right = 2)
print("hello")
```

Named form writes the parameter name explicitly, so source order does not change which parameter receives a named
argument. Positional arguments bind in order to the positional parameter prefix and must appear before named arguments.
Both forms evaluate their argument expressions in source order.

Duplicate arguments for one parameter and arguments for nonexistent parameters are errors. A required parameter without
a supplied argument or default is also an error.

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

The supplied value or access path must satisfy the parameter's ownership, borrowing, mutation-authority, and
initialization requirements. Its destruction and finalization obligations follow the call's ownership transfers.
The call must also permit the argument's effects and capabilities and preserve any conditions it relies on, under
[expression context](expression-context.md).

A function call, method call, or static function call cannot use positional syntax to satisfy parameters that are not
marked `pos`.

Tuple expressions and array expressions remain positional structural expressions because their positions are the
structure being constructed, not callable parameter binding.

## Argument type matching

After argument mapping, an explicitly supplied argument matches its parameter type when one of these rules applies:

- The argument has the parameter's type.
- The argument has type `T` and the parameter has type `T?`. The call constructs a present nullable value.
- The argument is a numeric literal that admits the parameter's numeric type under
  [literal adaptation](literal-expressions.md#literal-adaptation).

A typed numeric variable does not match a different numeric type through widening. Argument matching does not apply
user-defined conversions or recursive `as` conversions. An explicit conversion can supply a value of the parameter type.
Foreign variadic arguments follow the separate
[C variadic promotion rules](../targets-layout-abi-and-raw-memory/callable-abi.md#variadic-callable-contracts).

An expected parameter type can guide contextual construction when that parameter is known. For an overload family,
[overload selection](../callables/function-overloading.md) uses the supplied expressions and each candidate's parameter
types. The expected result type does not select a candidate.

A type match establishes only the type part of call selection. The selected call must also satisfy ownership,
borrow and reborrow rules, dependencies, lifecycle obligations, capabilities, effects, and preconditions. A failure in
those checks rejects the selected call without selecting another overload.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Function call expressions](function-call-expressions.md)
- Next: [Defaulted arguments](defaulted-arguments.md)

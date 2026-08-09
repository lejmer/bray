# Function overloading

Function overloading is explicit.

Same-name function declarations do not automatically form an overload set.

An overload declaration introduces a shared call name over separately named callable declarations.

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

The overload name is the shared call surface.

Each overload arm keeps its own declaration name.

The separately named arm can still be referenced and called directly.

```bray
let value = parse_int(input);
```

Direct calls to an arm use that arm's ordinary callable rules, including default arguments.

Calls through the overload name use overload selection.

```bray
let value = parse(input);
```

Of the call's arguments, overload selection uses only those explicitly supplied by the caller. It also applies receiver rules,
explicit generic substitution and static constraints, and target availability where those categories are present.

Default arguments do not participate in overload selection.

A defaulted parameter does not make an overload arm selectable when that parameter is omitted.

After a single overload arm has been selected, the call is checked as a call to that selected arm.

Result type does not participate in overload selection.

Expected type does not participate in overload selection.

Overload resolution does not rank candidates.

A call through an overload name must resolve to exactly one overload arm.

If no overload arm matches, the call is rejected.

If more than one overload arm matches, the call is rejected as ambiguous.

For the overload above:

```bray
parse(input)
```

selects `parse_float`.

```bray
parse(input, radix = 10)
```

selects `parse_int`.

The direct call remains valid:

```bray
parse_int(input)
```

An overload arm matches a call only when:

- every supplied named argument exists as a parameter of the arm,
- every supplied positional argument corresponds to a `pos` parameter at the same position in the arm,
- no parameter is supplied more than once,
- every parameter of the arm is supplied explicitly by the call,
- each supplied argument expression is compatible with the corresponding parameter type,
- every explicitly supplied generic argument forms a valid substitution,
- the arm's static generic constraints are satisfied,
- the arm is available for the selected target profile.

Argument ownership availability, borrow availability, mutation authority, dependency contracts, effects, capabilities, trusted
obligations, `requires(...)` conditions, postconditions, expected result type, and result type do not make an overload arm match.

After exactly one arm is selected, ordinary call checking validates all of those requirements for the selected arm. If that call is
invalid, the call is rejected. Resolution does not fall back to another overload arm.

The receiver of a method is supplied by method-call syntax and is not a named argument.

For method overloads, receiver mode and receiver compatibility participate in overload selection.

Declarations from other modules or packages participate in overload resolution only through visible overload declarations and deterministic lookup.

Ambiguous polymorphism is rejected.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Callable types and values](callable-types-and-values.md)
- Next: [Generic functions](generic-functions.md)

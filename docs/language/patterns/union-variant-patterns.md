# Union variant patterns

A union variant pattern matches the active variant of a union value.

Qualified variant pattern:

```bray
Shape.Circle(center = c, radius = r)
```

Expected-subject variant pattern:

```bray
Circle(center = c, radius = r)
```

Explicit expected-subject shorthand:

```bray
.Circle(center = c, radius = r)
```

The unqualified expected-subject form is valid when pattern resolution finds the variant through the subject type or
another visible pattern-capable declaration.

The leading-dot form is valid when the expected subject type is a known union type and that union contains the named
variant. It is an explicit subject-member shorthand, not the required form.

A no-payload variant pattern uses no parentheses.

```bray
Shape.Empty
Empty
.Empty
```

A payload variant pattern uses parentheses and payload patterns.

```bray
Circle(center = c, radius = r)
```

Named payload patterns are matched by name.

Named payload pattern order does not matter.

A positional payload pattern can match a payload field declared with `pos`.

```bray
Result.Ok(value)
Result.Error(error)
```

Each positional payload pattern supplies the corresponding `pos` payload field by declaration order.

Positional payload patterns must appear before named payload patterns.

A positional payload pattern for a non-`pos` payload field is rejected.

A payload field cannot be matched both positionally and by name.

Duplicate payload fields are errors.

Unknown payload fields are errors.

Missing payload fields are errors unless `..` is present.

Successful matching of a variant pattern refines the subject to that active variant in the matched region.

Payload bindings become available according to the pattern operation's ownership and access mode.

## Union refinement

A successful union variant pattern refines the subject to the matched active variant.

Within the matched region:

```text
the active variant is known,
the selected payload exists,
payload fields are initialized,
payload field bindings are available according to the operation mode.
```

A no-payload variant pattern introduces no payload bindings.

A payload variant pattern introduces bindings for the selected payload fields.

Exhaustive handling of a closed union accounts for every variant.

Coverage checking for unions uses the union's closed variant set.

Control-flow merges after union variant matching require coherent type, ownership, initialization, destruction,
capability, and finalization state.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Literal patterns](literal-patterns.md)
- Next: [Product patterns](product-patterns.md)

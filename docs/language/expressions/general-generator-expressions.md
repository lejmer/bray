# General generator expressions

A **general generator expression** constructs a generator value from zero or more yielded elements.

```bray
let names =
{
    each user in users
    {
        yield user.name;
    }
};
```

A general generator expression has the form:

```bray
{
    each <pattern> in <source>
    {
        ...
    }
}
```

A brace-enclosed expression whose only top-level child is a general generator iteration expression is parsed as a
general generator expression rather than an ordinary block expression.

A general generator expression must contain exactly one top-level generator iteration expression.

A bare top-level `each` is not valid in an ordinary block expression.

The top-level generator iteration expression establishes the general generator region.

A general generator region is a multi-yield region.

`yield` inside the iteration body contributes values to the general generator region unless captured by a nested
yield-capable region.

The generated element type is determined from yielded values, expected-type guidance, conversion checking, and
constraint solving.

When an expected generator element type is available, each yielded value is checked against that type.

The source expression of the top-level generator iteration expression is evaluated once before iteration begins.

Nested control flow and nested generator iteration expressions can be used inside the top-level iteration body to
compose produced values.

```bray
let values =
{
    each source in sources
    {
        each item in source
        {
            yield item;
        }
    }
};
```

The general generator expression completes with the generated value.

The top-level generator iteration expression itself completes as `unit`.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Array generator expressions](array-generator-expressions.md)
- Next: [General generator iteration expressions](general-generator-iteration-expressions.md)

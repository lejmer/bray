# Type declarations

A **type declaration** introduces a named type.

Product types are declared with `struct`.

Union types are declared with `union`.

```bray
struct Point
{
    x: r64;
    y: r64;
}

union Shape
{
    Circle(center: Point, radius: r64);
    Rectangle(min: Point, max: Point);
    Empty;
}
```

A type declaration defines the type’s primary semantic surface.

A type declaration can contain representation members, lifecycle declarations, constructors, and other type-owned declarations according to the rules for that type category.

A type declaration can be `public` or `internal`.

`public` is the default visibility.

```bray
public struct Point
{
    x: r64;
    y: r64;
}

internal struct ParserState
{
    position: usize;
}
```

A public type is part of the public API surface of its module or package.

An internal type is available within its intended scope.

Use outside that scope requires explicit internal-use acknowledgement.

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Scalar Types](scalar-types.md)
- Next: [Generic types](generic-types.md)

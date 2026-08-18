# Visibility and paths

## Function visibility

Function declarations can be public or internal.

Public is the default.

```bray
public func exported() -> i32
{
    return 1;
}

internal func helper() -> i32
{
    return 2;
}
```

Since `public` is the default, this is equivalent to `public func`:

```bray
func exported() -> i32
{
    return 1;
}
```

Use of internal functions outside their intended scope requires explicit acknowledgement.

```bray
let x = internal some.module.helper();
```

A module can acknowledge internal use through `using internal`.

```bray
using internal some.module.helper;
```

`using internal` acknowledges specific internal modules, declarations, or declaration paths.

`using internal` applies to specific modules, declarations, or declaration paths rather than entire packages.

## Module paths and function calls

Bray uses `.` for module paths, package paths, type paths, member access, and nested access.

```bray
math.sin(x);
pkg.module.function(value = x);
pkg.module.Type;
```

The binder resolves whether the left side is a module, package, type, value, or access path.

A referenced external path must either be declared by a `using` declaration or be reachable through the current package
or module context.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Effects and capabilities](effects-and-capabilities.md)
- Next: [Local callable values](local-callable-values.md)

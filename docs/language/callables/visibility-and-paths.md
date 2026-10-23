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
using some.api;

let x = internal some.api.helper();
```

A module can acknowledge internal use through `using internal`.

```bray
using internal some.api.helper;
```

`using internal` acknowledges specific internal modules, declarations, or declaration paths.

`using internal` applies to specific modules, declarations, or declaration paths rather than entire packages.

## Module paths and function calls

Bray uses `.` for module paths, package paths, type paths, member access, and nested access.

```bray
using math;
using pkg.api;

math.sin(x);
pkg.api.function(value = x);
pkg.api.Type;
```

The binder resolves whether the left side is a module, package, type, value, or access path.

A named path into another logical module requires a matching `using` declaration, including paths within the current
package. A fully qualified path identifies its target but does not declare the dependency. Local `internal` and `trusted`
do not supply a missing dependency declaration.

References within the current logical module need no self-import. See
[Using declarations](../modules-and-packages/using-declarations.md) for path coverage and split-module scope.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Effects and capabilities](effects-and-capabilities.md)
- Next: [Local callable values](local-callable-values.md)

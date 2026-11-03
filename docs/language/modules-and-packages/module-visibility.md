# Module visibility

A module declaration can be public or internal.

`public` is the default.

```bray
public module api;
module api;

internal module details;
```

Since `public` is the default, `public module api;` and `module api;` declare the same public module visibility.

Public modules are reachable through ordinary package and module path resolution.

An internal module path is accessible within its defining package. Access from a consuming package requires explicit
internal-use acknowledgement under [visibility and reachability](../declarations/visibility-and-reachability.md).
References into another logical module still require a matching `using` declaration.

```bray
using internal details;
```

Internal module visibility applies to the module path.

Public declarations inside an internal module still require internal-use acknowledgement from consuming packages.

Split declarations of the same module must agree on module visibility.

Changing a module's visibility can be a public API change.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Split modules](split-modules.md)
- Next: [Trusted modules](trusted-modules.md)

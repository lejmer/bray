# Trusted modules

A module declaration can include `trusted`.

```bray
trusted module runtime.memory;
```

Block module declarations can also be trusted:

```bray
trusted module runtime.memory
{
    ...
}
```

The `trusted` modifier permits trusted declarations in that module.

It does not make ordinary declarations in the module trusted.

If a module has split declarations, all declarations that contribute to that module share the same trusted-module state.

Split declarations of the same module must agree on whether the module is trusted.

A trusted module declaration is rejected when the resulting logical module contains no trusted declarations.

A non-trusted module cannot contain trusted declarations.

A module declaration can combine `trusted` with module visibility.

```bray
trusted internal module runtime.memory;
```

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Module visibility](module-visibility.md)
- Next: [Module bodies](module-bodies.md)

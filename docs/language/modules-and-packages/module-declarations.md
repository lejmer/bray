# Module declarations

Every source unit must declare its source-scoped module.

The source-scoped module declaration syntax is:

```bray
module net;
```

Grammar shape:

```text
module-modifiers 'module' module-path ';'
```

where:

```text
module-modifiers = ['trusted'] [visibility]
```

The canonical modifier order is `trusted` before visibility.

The source-scoped module declaration applies to the rest of the source unit outside later block module declarations.

The source-scoped module declaration must appear before any `using`, `export`, function, type, trait, implementation, constant, predicate, lifecycle, or other semantic declaration in the source unit.

Comments and documentation comments can appear before the source-scoped module declaration.

A source unit without a source-scoped module declaration is rejected.

Module identity is explicit.

Source origins do not define module identity.

Moving or renaming a source input does not rename its module.

## Block module declarations

A block module declaration contributes declarations to a named module:

```bray
module net.tests
{
    ...
}
```

Grammar shape:

```text
module-modifiers 'module' module-path block
```

A block module declaration is a package-level module contribution.

It is not nested inside the source-scoped module.

It does not inherit the source-scoped module path.

It can name any module in the current package.

This permits test and support modules to live in the same source unit as the declarations they exercise:

```bray
module net;

func parse_packet(pos bytes: &[u8]) -> Packet
{
    ...
}

@test
module net.tests
{
    @test
    func parses_minimal_packet()
    {
        ...
    }
}
```

Module declarations cannot be nested.

```bray
module net
{
    module net.tests
    {
        ...
    }
}
```

This is rejected.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Target constraints and gates](target-constraints-and-gates.md)
- Next: [Split modules](split-modules.md)

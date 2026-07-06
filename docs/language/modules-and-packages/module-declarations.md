# Module declarations

Every source unit must declare at least one module contribution.

There are two source-unit shapes:

- one source-unit module declaration followed by unbraced module items,
- one or more braced block module declarations.

These shapes do not mix in the same source unit.

## Source-unit module declarations

A source-unit module declaration uses the semicolon form:

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

The source-unit module declaration applies to every unbraced item that follows it in the source unit.

A source-unit module declaration must appear before any `using`, `export`, function, type, trait, implementation, constant,
predicate, lifecycle, or other semantic declaration in that source-unit shape.

Comments and documentation comments can appear before the source-unit module declaration.

Block module declarations are not allowed after a source-unit module declaration.

Module identity is explicit.

Source origins do not define module identity.

Moving or renaming a source input does not rename its module.

## Block module declarations

A block module declaration uses the braced form and contributes declarations to a named module:

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

It is not nested inside a source-unit module.

It does not inherit a source-unit module path.

It can name any module in the current package.

If a source unit starts with a block module declaration, then every top-level declaration in that source unit must be a block
module declaration.

This permits test and support modules to live in the same source unit as the declarations they exercise:

```bray
module net
{
    func parse_packet(pos bytes: &[u8]) -> Packet
    {
        ...
    }
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

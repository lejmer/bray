# Directives

A **directive** is a compile-time instruction attached to a source element.

Directive syntax uses `@` followed by the directive name.

```bray
@target(std.target.atomic.u64)
module counters;

@layout(c)
struct Header
{
    ...
}

@entrypoint
func main()
{
    ...
}
```

Argumentless directives omit parentheses.

Directives with arguments use parenthesized directive arguments.

Directive arguments use constant-expression syntax.

The directive name determines:

- which source elements the directive can attach to,
- whether arguments are required,
- whether positional directive arguments are accepted,
- which named directive arguments are accepted,
- whether the directive can be repeated,
- which semantic rules the directive changes.

Language-defined directives include:

- `@target(...)`,
- `@test`,
- `@entrypoint`,
- `@link(...)`,
- `@symbol(...)`,
- `@abi(...)`,
- `@layout(...)`,
- `@copy`,
- `@tag(...)`.

Module directives are defined in [Modules and packages](../modules-and-packages.md).

Callable ABI, linking, symbols, extern-related directives, target directives, and layout directives are defined in [Targets, layout, ABI, and raw memory](../targets-layout-abi-and-raw-memory.md).

Copy directives are defined in [Copy contracts](../types/copy-contracts.md).

Repeated directives, conflicting directives, invalid directive targets, invalid directive arguments, and unsupported directive combinations are rejected.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Visibility and reachability](visibility-and-reachability.md)
- Next: [Modifiers](modifiers.md)

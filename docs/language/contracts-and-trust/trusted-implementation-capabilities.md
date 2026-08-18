# Trusted implementation capabilities

A trusted implementation capability is authority used inside a trusted declaration body.

Trusted implementation capabilities are compiler-known names in the ordinary lookup namespace. They are not values or
predicates, cannot be called, and are only valid where the grammar expects a trusted capability.

A trusted declaration that uses trusted implementation capabilities declares the exact capabilities used by its body
with `uses(...)`.

```bray
trusted func copy_bytes(
    destination: &mut ByteBuffer,
    source: &ByteBuffer,
    count: usize,
) uses(raw_memory)
{
    ...
}
```

Multiple trusted capabilities are comma-separated.

```bray
trusted func call_os_read(
    handle: OsHandle,
    buffer: &mut ByteBuffer,
) -> Result<usize, OsError>
    uses(foreign_call, unchecked_init)
{
    ...
}
```

The `uses(...)` clause is implementation-side.

It says what trusted capabilities the declaration body uses.

It does not by itself impose a trusted obligation on callers.

A trusted declaration must use exactly the trusted implementation capabilities it declares.

Declaring an unused trusted capability is rejected.

Using a trusted implementation capability that is not declared by the surrounding trusted declaration is rejected.

If a trusted declaration uses no trusted implementation capabilities, it has no `uses(...)` clause.

A non-`trusted` declaration cannot have a `uses(...)` clause with trusted capabilities.

`trusted expression` does not grant trusted implementation capabilities.

Trusted implementation capabilities cover low-level operations named by the trust rules, such as raw memory, manual
allocation, foreign calls, unchecked initialization, and other compiler-known capabilities.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Ordinary requirements and postconditions](preconditions-and-postconditions.md)
- Next: [Trusted caller obligations](trusted-caller-obligations.md)

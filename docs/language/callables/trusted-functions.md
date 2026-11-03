# Trusted functions

Trusted functions use the `trusted` modifier.

A trusted function can use trusted implementation capabilities, expose trusted caller obligations, or both.

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

`uses(...)` declares trusted implementation capabilities available only inside the trusted declaration body. A trusted
function that uses none has no `uses(...)` clause.

A trusted implementation can expose an ordinary safe API. Any trusted caller obligations must appear in the callable
contract and be satisfied by the caller, whether the function is public or internal.

Trusted implementation capabilities are defined in
[Trusted implementation capabilities](../contracts-and-trust/trusted-implementation-capabilities.md).

Trusted caller obligations are defined in
[Trusted caller obligations](../contracts-and-trust/trusted-caller-obligations.md).

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Contract clauses on functions](contract-clauses-on-functions.md)
- Next: [Async functions and computations](async-functions-and-computations.md)

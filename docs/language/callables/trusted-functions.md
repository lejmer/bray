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

`uses(...)` declares trusted implementation capabilities used by the body.

A trusted function that uses no trusted implementation capabilities has no `uses(...)` clause.

A trusted implementation can expose an ordinary safe API.

A trusted caller obligation appears in the public function contract.

Trusted implementation capabilities are available only inside the trusted declaration body.

Trusted caller obligations must be visible in the callable contract and satisfied by the caller.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Contract clauses on functions](contract-clauses-on-functions.md)
- Next: [Async functions and computations](async-functions-and-computations.md)

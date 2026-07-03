# Trusted caller obligations

A trusted caller obligation is a trusted predicate requirement in a contract clause.

```bray
requires(
    trusted core.memory.valid_read(pointer = source, count = count),
)
```

A trusted caller obligation represents a fact Bray cannot prove or check through ordinary semantics.

Trusted caller obligations must come from at least one of:

- an explicit trust boundary,
- an enclosing trusted obligation,
- a live trusted witness value,
- a trusted declaration that establishes the fact,
- a compiler-recognized trusted source.

Trusted caller obligations must not appear from nowhere.

A trusted implementation can expose an ordinary safe API when the declaration checks or establishes all necessary invariants internally.

```bray
trusted func copy_bytes_checked(
    destination: &mut ByteBuffer,
    source: &ByteBuffer,
    count: usize,
) -> Result<unit, CopyError>
    uses(raw_memory)
{
    if count > destination.length
    {
        return Result.Error(CopyError.DestinationTooSmall);
    }

    if count > source.length
    {
        return Result.Error(CopyError.SourceTooSmall);
    }

    ...
}
```

Calling that function is ordinary because the function's public contract does not expose trusted caller obligations.

A declaration exposes trusted caller obligations when its contract requires trusted facts.

```bray
trusted func copy_bytes_unchecked(
    destination: RawPointer<u8>,
    source: RawPointer<u8>,
    count: usize,
) -> unit
    requires(
        trusted core.memory.valid_write(pointer = destination, count = count),
        trusted core.memory.valid_read(pointer = source, count = count),
        trusted core.memory.non_overlapping(left = destination, left_count = count, right = source, right_count = count),
    )
    uses(raw_memory)
{
    ...
}
```

The caller must provide, preserve, or acknowledge those facts.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Trusted implementation capabilities](trusted-implementation-capabilities.md)
- Next: [Predicates and predicate expressions](predicates-and-predicate-expressions.md)

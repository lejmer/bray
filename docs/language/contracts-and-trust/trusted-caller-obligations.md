# Trusted caller obligations

A trusted caller obligation is a trusted predicate requirement in a contract clause.

```bray
using core.memory;

requires(
    trusted core.memory.valid_read(pointer = source, count = count),
)
```

A trusted caller obligation represents a condition Bray cannot prove or check through ordinary semantics.

Trusted caller obligations must come from at least one of:

- an explicit trust boundary,
- an enclosing trusted obligation,
- a live trusted witness value,
- a trusted declaration that establishes the condition,
- a compiler-recognized trusted source.

Trusted caller obligations must not appear from nowhere.

A trusted implementation can expose an ordinary safe API when the declaration checks or establishes all necessary
invariants internally.

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
        return Error(DestinationTooSmall);
    }

    if count > source.length
    {
        return Error(SourceTooSmall);
    }

    ...
}
```

Calling that function is ordinary because the function's public contract does not expose trusted caller obligations.

A declaration exposes trusted caller obligations when its contract requires trusted guarantees.

```bray
using core.memory;

trusted func copy_bytes_unchecked(
    destination: DataPointer<u8>,
    source: DataPointer<u8>,
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

The caller must provide, preserve, or acknowledge those conditions.

## Establishing trusted guarantees

An explicitly trusted postcondition on a trusted declaration can establish a trusted premise that ordinary contract
reasoning cannot derive. The declaration's author is responsible for that premise. The compiler records its trusted
origin separately from a checked deduction. A trusted declaration remains subject to ordinary type, ownership,
borrowing, initialization, lifecycle, effect and execution-property checks.

A trusted implementation capability permits its named operations. It does not by itself establish a predicate.
A trust boundary acknowledges its operand's caller obligations. It does not turn those obligations into established
postconditions or hide unresolved obligations from the enclosing declaration's callers.

A trusted premise about raw memory uses the language-defined meaning of the selected memory predicate. It cannot
replace a required ownership or initialization transition, override a conflicting checked state, create a second owner,
or end an active dependent borrow. Ordinary postconditions and `pure` or `total` promises remain checked.

A trusted declaration can expose an ordinary API when its implementation establishes the obligations internally.
Its callers do not require a trust boundary merely because its implementation uses trusted operations.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Trusted implementation capabilities](trusted-implementation-capabilities.md)
- Next: [Predicates and predicate expressions](predicates-and-predicate-expressions.md)

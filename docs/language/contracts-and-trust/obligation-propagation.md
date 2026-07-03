# Obligation propagation

Any trusted obligation used inside a callable or lifecycle declaration must be discharged before the declaration exits or appear in that declaration's public contract.

This prevents trusted obligations from being hidden by wrappers.

```bray
func bad(pos pointer: RawPointer<u8>) -> u8
{
    return trusted core.memory.read<u8>(pointer);
}
```

This is rejected because the wrapper accepts a trusted obligation inside its body but does not expose that obligation to its caller.

This is valid because the obligation is exposed:

```bray
func read_wrapper(pos pointer: RawPointer<u8>) -> u8
    requires(
        trusted core.memory.valid_read(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = pointer),
        trusted core.memory.initialized_as<u8>(pointer = pointer),
    )
{
    return trusted core.memory.read<u8>(pointer);
}
```

A wrapper is also valid when it proves or establishes the required facts before calling the trusted operation.

Trusted obligations must propagate through:

- wrappers,
- callable values,
- generic parameters,
- dynamic dispatch,
- constructors,
- destructors,
- finalizers,
- scope enter and exit declarations,
- named callable contracts,
- exports,
- re-exports,
- internal declarations.

A callable with trusted caller obligations cannot be used where an ordinary callable is expected.

```bray
let f: func(pos buffer: &Buffer, index: usize) -> u8 = get_unchecked;
```

This is rejected when `get_unchecked` has trusted caller obligations.

The callable type must preserve the trusted obligation.

A trust boundary inside a callable or lifecycle declaration does not hide the obligation from that declaration's callers.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Trust boundaries](trust-boundaries.md)
- Next: [Lifecycle and foreign boundaries](lifecycle-and-foreign-boundaries.md)

# Trusted witness values

A trusted witness value is an ordinary value whose live value contract carries one or more trusted predicate facts.

Witness values are a semantic category of values, not a separate source construct.

There is no dedicated witness expression syntax.

Witness values are introduced by ordinary declaration mechanisms:

- trusted functions,
- trusted constructors,
- trusted lifecycle declarations,
- [compiler-known declarations](../compiler-known-and-standard-library/compiler-known-declarations.md),
- [recognized standard-library declarations](../compiler-known-and-standard-library/standard-library-recognition.md) whose language-defined contracts establish trusted facts.

A declaration introduces a witness value when its successful result carries trusted facts in `ensures(...)` and those facts are tied to the returned value, its fields, its owned storage, its borrows, or its dependency contract.

```bray
struct ReadableByte
{
    pointer: RawPointer<u8>;
}

trusted func assume_readable_byte(pos pointer: RawPointer<u8>) -> ReadableByte
    requires(
        trusted core.memory.valid_read(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = pointer),
        trusted core.memory.initialized_as<u8>(pointer = pointer),
    )
    ensures(
        result.pointer == pointer,
        trusted core.memory.valid_read(pointer = result.pointer, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = result.pointer),
        trusted core.memory.initialized_as<u8>(pointer = result.pointer),
    )
{
    return ReadableByte(pointer = pointer);
}
```

The returned `ReadableByte` value is a witness value.

While the witness value is live and valid, its carried trusted facts are available in the fact context.

Those facts can satisfy matching trusted requirements.

```bray
let readable = trusted assume_readable_byte(pointer);

let byte = core.memory.read<u8>(readable.pointer);
```

The trust boundary in the first statement acknowledges the trusted requirements declared by `assume_readable_byte`.

The second statement can use the trusted facts carried by `readable`.

A trust boundary expression does not create a witness value by itself.

If a trust boundary operand returns a witness value, the lasting facts come from the returned value's contract, not from the trust boundary.

## Witness lifetime and movement

A witness value carries its trusted facts only while the value is fully initialized, live, and valid.

Moving a witness value moves the trusted facts carried by that value.

After the move, the old access path no longer carries those facts.

Returning a witness value returns its trusted facts as part of the result value's dependency contract.

Storing a witness value stores its trusted facts only when the destination can preserve every dependency required by the value.

Storing a witness value does not extend any allocation, borrow, scoped capability, resource, or lifetime that the witness depends on.

Copying a witness value is rejected unless the value's copy contract explicitly states how the copied value preserves or splits the trusted facts.

Observing or copying ordinary fields from a witness value does not copy the trusted facts carried by the witness value.

The trusted facts remain tied to the witness value and to the dependencies named by its contract.

Partially moving a witness value is rejected when the move would separate trusted facts from the value or dependency that carries them.

## Witness invalidation

The trusted facts carried by a witness value are invalidated when the witness value is moved from, consumed, destroyed, finalized, assigned `none`, replaced, or otherwise stops being live.

The trusted facts are also invalidated when a value, storage identity, allocation, borrow, scoped capability, resource, epoch, or version named by the carried facts is invalidated.

Mutating a witness value, a referenced access path, or referenced storage invalidates every carried fact whose truth can depend on the mutated state.

Destroying or finalizing an owner invalidates witness facts that depend on the owner's storage or resource state.

Leaving a `with` body invalidates witness facts that depend on the scoped capability produced by that `with` expression unless the selected `exit` contract transfers the obligation to another live value.

Cancellation, panic propagation, `return`, `yield`, `break`, `continue`, nullable propagation, result propagation, and run-result propagation preserve witness facts only along paths where the witness value and every dependency it names remain live and valid.

## Witness use in contracts

A trusted requirement can be satisfied by a live witness value when the fact carried by the witness value matches the required trusted predicate after ordinary path, field, equality, and dependency reasoning.

Witness facts participate in the same fact context as other trusted facts.

They do not bypass ownership checking, borrowing rules, mutation authority, initialization checking, destruction checking, finalization checking, capability checking, visibility, internal-access acknowledgement, or effect checking.

A callable or lifecycle declaration that receives, returns, stores, or captures a witness value must preserve the witness value's dependency contract.

A public API that exposes a witness value exposes the trusted facts and obligations carried by that value as part of its public contract.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Fact context](fact-context.md)
- Next: [Trust boundaries](trust-boundaries.md)

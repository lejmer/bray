# Contracts and Trust

**Specification:** [Contracts and trust](https://github.com/lejmer/bray/blob/develop/docs/language/contracts-and-trust.md)

## Contents

- [Contract clauses](#contract-clauses)
- [Predicates and predicate expressions](#predicates-and-predicate-expressions)
- [Static constraints](#static-constraints)
- [Contract reasoning](#contract-reasoning)
- [Trusted implementation and caller contracts](#trusted-implementation-and-caller-contracts)
- [Trust boundaries and witness values](#trust-boundaries-and-witness-values)
- [Obligation propagation](#obligation-propagation)
- [Choose the contract mechanism](#choose-the-contract-mechanism)
- [Common mistakes](#common-mistakes)

## Contract clauses

**Core model:** `requires(...)` states conditions needed before execution, `ensures(...)` states guarantees after normal completion, `with(...)` constrains generic declarations at compile time, and `uses(...)` lists the exact trusted implementation capabilities exercised by a trusted body.

The examples assume referenced support types, traits, and compiler-known declarations are in scope. Their bodies are intentionally small so the contract surface remains visible.

```bray
trusted module contract_example;

predicate range_fits(offset: usize, count: usize, length: usize) = offset + count <= length;

const func remaining(pos length: usize, offset: usize) -> usize
    requires(offset <= length)
    ensures(result + offset == length)
{
    return length - offset;
}

func reserve(pos length: usize, capacity: usize, requested: usize) -> usize
    requires(length <= capacity, requested <= capacity - length)
    ensures(result == length + requested, result <= capacity)
{
    assert(length <= capacity);
    assert(requested <= capacity - length, "reservation exceeds capacity");

    return length + requested;
}
```

Each clause accepts a comma-separated list of predicate expressions. Put every wrapped entry on its own line with a trailing comma. A short single-entry clause can remain inline.

Ordinary requirements are proved from available conditions or checked at runtime when the declaration permits it. A failed runtime requirement panics. Postconditions become available only after successful normal completion and remain available while every value, storage identity, capability, and version they mention remains valid.

`result` is the compiler-introduced postcondition binding for a declaration's normal completion value. Use it only in `ensures(...)` on a declaration that produces a value.

Contract integer arithmetic uses exact mathematical semantics. The `offset + count` relation in `range_fits` therefore cannot wrap like a machine-sized runtime operation. Generated runtime checks preserve the same meaning through checked arithmetic or an equivalent rewrite.

See [contract clauses](https://github.com/lejmer/bray/blob/develop/docs/language/contracts-and-trust/contract-clauses.md), [ordinary requirements and postconditions](https://github.com/lejmer/bray/blob/develop/docs/language/contracts-and-trust/preconditions-and-postconditions.md), and [contract arithmetic](https://github.com/lejmer/bray/blob/develop/docs/language/contracts-and-trust/contract-arithmetic.md).

## Predicates and predicate expressions

A predicate names a pure contract-level relation. An ordinary predicate has one predicate-expression body. A trusted predicate declares an opaque relation in a trusted module and ends with a semicolon.

```bray
predicate nonempty(length: usize) = length > 0;

predicate has_space(length: usize, capacity: usize) = remaining(capacity, offset = length) > 0;

predicate every_index_fits(indices: &[usize], length: usize) = all(
    {
        each index in indices
        {
            yield index < length;
        }
    }
);

predicate any_flag(flags: &[bool]) = any(flags);

trusted predicate readable_byte(pointer: RawPointer<u8>);
```

Predicate parameters have names and types without callable ownership modifiers or defaults. Predicate declarations can be module-level, type-associated, trait requirements or defaults, and trait implementation fulfillments.

Predicate expressions are pure, deterministic, total, terminating, and observational. They can use literals, parameters, constants, observable fields and structural values, exact arithmetic, Boolean operators, comparisons, conditional expressions, predicate calls, predicate-valid callable calls, and finite bounded `all(...)` or `any(...)` folds. `result` is additionally available in a postcondition predicate context.

A public function or method called from a predicate expression exposes `const` in its declaration surface. Its selected contract, arguments, requirements, body operations, and result must all be valid in predicate-expression context. Private helpers can have const eligibility inferred within one checking unit, but that inference does not become exported API.

Keep predicate bodies to one predicate expression. Stateful operations, ownership transfer, allocation, I/O, async execution, trusted capability use, and other effects belong in ordinary execution rather than contract evaluation.

See [predicate declarations](https://github.com/lejmer/bray/blob/develop/docs/language/declarations/predicate-declarations.md), [predicates and predicate expressions](https://github.com/lejmer/bray/blob/develop/docs/language/contracts-and-trust/predicates-and-predicate-expressions.md), and [predicate-expression callable calls](https://github.com/lejmer/bray/blob/develop/docs/language/contracts-and-trust/predicate-expression-callable-calls.md).

## Static constraints

A `with(...)` clause contains compile-time predicate expressions over declared generic parameters, trait applications, type-valued members, constant arguments, and other static values.

```bray
func first_byte<I>(pos iter: I) -> u8?
    with(I: Iterable, I(Iterable).Element == u8)
{
    return none;
}

struct NonemptyBuffer<T, const N: usize>
    with(T: Copyable, N > 0)
{
    values: [T; N];
}
```

The first constraint establishes the exact `Iterable` application before the second constraint refers to its selected `Element`. Type equality relates already available types. Trait satisfaction selects the implementation surface that makes a qualified type-valued member meaningful.

Static constraints are available while checking the constrained declaration's signature, contract clauses, and body. They govern compile-time validity, while runtime conditions come from ordinary value predicates and requirements.

See [static constraints](https://github.com/lejmer/bray/blob/develop/docs/language/contracts-and-trust/static-constraints.md).

## Contract reasoning

Conditions become available from requirements, guarantees, constructors, lifecycle contracts, branches, successful matches and guards, assertions, trusted witnesses, and trusted declarations. A requirement is satisfied when the matching condition is available on every control-flow path reaching the operation.

```bray
func checked_offset(pos length: usize, offset: usize) -> usize
    requires(offset <= length)
    ensures(result <= length)
{
    if offset == length
    {
        return length;
    }

    assert(offset < length, "offset must identify an element");

    return offset;
}
```

A successful `assert(condition)` or `assert(condition, message)` establishes its ordinary condition on the normal continuation. The message is evaluated only when the condition fails.

An available condition remains usable while the values and state it describes remain valid and unchanged in relevant ways. Mutation, movement, consumption, destruction, finalization, reinitialization, borrow expiry, allocation expiry, capability loss, or epoch change can invalidate it. Conditions over independent copied immutable scalars can remain available after the original storage changes.

See [contract reasoning](https://github.com/lejmer/bray/blob/develop/docs/language/contracts-and-trust/contract-reasoning.md) and [assertion expressions](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/assertion-expressions.md).

## Trusted implementation and caller contracts

Trusted implementation capabilities and trusted caller obligations are independent. A trusted declaration lists implementation authority in `uses(...)`. Trusted predicate requirements in `requires(...)` describe conditions its caller must establish, preserve, or visibly acknowledge.

```bray
struct ReadableByte
{
    pointer: RawPointer<u8>;
}

trusted func initialize_byte(pos pointer: RawPointer<u8>, pos value: u8)
    requires(
        trusted core.memory.valid_write<u8>(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = pointer),
    )
    ensures(trusted core.memory.initialized_as<u8>(pointer = pointer))
    uses(raw_memory, unchecked_init)
{
    trusted core.memory.write<u8>(pointer, value);
}

trusted func assume_readable_byte(pos pointer: RawPointer<u8>) -> ReadableByte
    requires(
        trusted readable_byte(pointer),
        trusted core.memory.valid_read<u8>(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = pointer),
        trusted core.memory.initialized_as<u8>(pointer = pointer),
    )
    ensures(
        result.pointer == pointer,
        trusted readable_byte(result.pointer),
        trusted core.memory.valid_read<u8>(pointer = result.pointer, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = result.pointer),
        trusted core.memory.initialized_as<u8>(pointer = result.pointer),
    )
{
    return ReadableByte
    {
        pointer = pointer,
    };
}

trusted func read_byte(pos readable: &ReadableByte) -> u8
    requires(
        trusted core.memory.valid_read<u8>(pointer = readable.pointer, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = readable.pointer),
        trusted core.memory.initialized_as<u8>(pointer = readable.pointer),
    )
    uses(raw_memory)
{
    return trusted core.memory.read<u8>(readable.pointer);
}
```

`initialize_byte` and `read_byte` use the exact implementation capabilities needed by their raw operations. Their trusted requirements remain caller-facing. `assume_readable_byte` uses no trusted implementation capability, but its trusted requirements and guarantees make the returned ordinary value a witness. The live witness can establish `read_byte`'s caller obligations without another boundary.

A trusted implementation can instead expose a fully ordinary API by checking or establishing every low-level invariant internally. In that case callers see ordinary requirements and errors rather than trusted obligations.

See [trusted implementation capabilities](https://github.com/lejmer/bray/blob/develop/docs/language/contracts-and-trust/trusted-implementation-capabilities.md) and [trusted caller obligations](https://github.com/lejmer/bray/blob/develop/docs/language/contracts-and-trust/trusted-caller-obligations.md).

## Trust boundaries and witness values

`trusted expression` places an explicit trust boundary around exactly one operand. It records acceptance of the trusted caller obligations required by that operand and otherwise preserves the operand's type, value category, ownership, control flow, effects, and finalization behavior.

```bray
func initialize_and_read(pos pointer: RawPointer<u8>, pos value: u8) -> u8
{
    trusted initialize_byte(pointer, value);

    let readable: ReadableByte = trusted assume_readable_byte(pointer);

    return read_byte(&readable);
}

trusted func read_pair(pos first: RawPointer<u8>, pos second: RawPointer<u8>) -> u16
    requires(
        trusted core.memory.valid_read<u8>(pointer = first, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = first),
        trusted core.memory.initialized_as<u8>(pointer = first),
        trusted core.memory.valid_read<u8>(pointer = second, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = second),
        trusted core.memory.initialized_as<u8>(pointer = second),
    )
    uses(raw_memory)
{
    return trusted
    {
        let low: u8 = core.memory.read<u8>(first);
        let high: u8 = core.memory.read<u8>(second);

        yield(high as u16) * 256 + (low as u16);
    };
}
```

The first function uses single-expression boundaries. The second uses a block operand, so both raw reads fall within one visible boundary. A trust boundary acknowledges obligations for its operand. Runtime validation remains explicit through ordinary checks or trusted API contracts, and implementation authority remains declaration-owned through `uses(...)`.

A witness value is an ordinary value whose live contract carries trusted guarantees. Its guarantees move with the value and remain available only while the value and every dependency named by those guarantees remain valid. Mutation, movement from the old path, destruction, finalization, replacement, partial separation, borrow expiry, or dependency invalidation removes affected guarantees.

The `ReadableByte` returned above carries the guarantees stated in `assume_readable_byte` and lets `read_byte` discharge matching requirements without another boundary. The boundary created the acknowledgement at the call site, while the returned value's `ensures(...)` contract carries the lasting evidence.

See [trust boundaries](https://github.com/lejmer/bray/blob/develop/docs/language/contracts-and-trust/trust-boundaries.md), [trust boundary expressions](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/trust-boundary-expressions.md), and [trusted witness values](https://github.com/lejmer/bray/blob/develop/docs/language/contracts-and-trust/trusted-witness-values.md).

## Obligation propagation

A declaration that uses a trusted obligation must discharge it internally or preserve it in its public contract. The same rule applies through wrappers, callable values, generic parameters, dynamic dispatch, constructors, lifecycle declarations, exports, and internal declarations.

```bray
callable ByteReader = trusted func(pos pointer: RawPointer<u8>) -> u8
    requires(
        trusted core.memory.valid_read<u8>(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = pointer),
        trusted core.memory.initialized_as<u8>(pointer = pointer),
    )
    uses(raw_memory);

trusted func read_wrapper(pos pointer: RawPointer<u8>) -> u8
    requires(
        trusted core.memory.valid_read<u8>(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = pointer),
        trusted core.memory.initialized_as<u8>(pointer = pointer),
    )
    uses(raw_memory)
{
    return trusted core.memory.read<u8>(pointer);
}
```

The named callable contract can hold only callable values with compatible trust, capability, and caller-obligation surfaces. `read_wrapper` exposes every trusted condition needed by the operation in its body, so callers see the obligation instead of losing it behind the wrapper.

Constructors, finalizers, destructors, `enter`, and `exit` declarations use the same `requires(...)`, `ensures(...)`, and `uses(...)` split. Foreign declarations state every pointer, initialization, ownership, aliasing, threading, synchronization, callback, resource, error, and panic condition visible to Bray callers.

See [obligation propagation](https://github.com/lejmer/bray/blob/develop/docs/language/contracts-and-trust/obligation-propagation.md) and [lifecycle and foreign boundaries](https://github.com/lejmer/bray/blob/develop/docs/language/contracts-and-trust/lifecycle-and-foreign-boundaries.md).

## Choose the contract mechanism

| Intent                                         | Mechanism                          | Decisive rule                                                                                 |
|------------------------------------------------|------------------------------------|-----------------------------------------------------------------------------------------------|
| Require a checkable value condition            | `requires(...)`                    | The caller proves it through available reasoning or the permitted runtime check.              |
| Publish a normal-completion guarantee          | `ensures(...)`                     | Use `result` for the returned value and preserve every dependency named by the guarantee.     |
| Restrict a generic declaration                 | `with(...)`                        | Establish trait applications before referring to their selected type-valued members.          |
| Name a reusable contract relation              | `predicate name(...) = expression` | Keep the body pure, deterministic, total, terminating, and observational.                     |
| Name an opaque trusted relation                | `trusted predicate name(...);`     | Declare it in a trusted module and use it as a trusted condition.                             |
| Declare trusted body authority                 | `uses(...)`                        | List exactly the trusted implementation capabilities exercised by that declaration body.      |
| Expose a compiler-unprovable caller condition  | `requires(trusted predicate(...))` | Preserve the condition through every wrapper and callable surface.                            |
| Accept trusted obligations at one use site     | `trusted expression`               | Scope the acknowledgement to the smallest operand that requires it.                           |
| Check and retain an ordinary runtime condition | `assert(condition[, message])`     | Successful completion makes the condition available while its referenced state remains valid. |
| Carry trusted guarantees with a value          | A value-returning `ensures(...)`   | Keep the witness and every transitive dependency live and valid.                              |

## Common mistakes

- Keep `uses(...)` for implementation authority and trusted `requires(...)` for caller obligations.
- Declare only capabilities the trusted body actually exercises.
- Put `result` only in a value-producing declaration's `ensures(...)` clause.
- Establish an exact trait application before using its type-valued members in `with(...)`.
- Keep predicate expressions observational and use ordinary execution for mutation, allocation, I/O, and async work.
- Preserve trusted obligations in wrappers, callable contracts, implementation surfaces, and lifecycle declarations until they are discharged.
- Use a trust boundary only around an operand with trusted caller obligations and keep its scope as small as the acknowledgement requires.
- Treat assertions as ordinary runtime checks and trusted boundaries as explicit acknowledgements of trusted obligations.
- Keep witness values and all referenced storage, borrows, capabilities, resources, epochs, and versions valid for as long as their guarantees are needed.
- Re-establish conditions after any operation that can invalidate the values or state they describe.

## Remember

Requirements belong to callers, guarantees belong to successful completion, static constraints belong to generic checking, capabilities belong to trusted implementations, and trusted obligations must remain visible until proof, a live witness, or an explicit boundary discharges them.

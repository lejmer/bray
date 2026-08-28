# Compiler-Known Declarations

**Specification:** [Compiler-known declarations and standard-library recognition](https://github.com/lejmer/bray/blob/develop/docs/language/compiler-known-and-standard-library.md)

## Contents

- [Identity and ambient availability](#identity-and-ambient-availability)
- [Compiler-provided declarations](#compiler-provided-declarations)
- [Protected representation](#protected-representation)
- [Compiler-known traits and operations](#compiler-known-traits-and-operations)
- [Recognized standard-library declarations](#recognized-standard-library-declarations)
- [Target-conditional availability](#target-conditional-availability)
- [Choose the declaration category](#choose-the-declaration-category)

## Identity and ambient availability

**Core model:** Compiler-known declarations have closed language-defined identities and contracts. They enter every module's ordinary lookup environment without an import when target-available, while recognized standard-library declarations remain ordinary visible package declarations whose stable identities let the compiler apply their specified contracts.

Compiler-known names are not imports or dependencies. They cannot be redeclared in the same declaration scope, shadowed by package declarations, re-exported, or inferred from matching source spelling. No compiler-known declaration belongs to the reserved `std` package namespace.

```bray
const ENABLED: bool = true;
const DISABLED: bool = false;
const NO_PORT: u16? = none;

func complete() -> unit
{
    return unit;
}

func transfer_pointer<T>(pos pointer: RawPointer<T>) -> RawPointer<T>
{
    return consume pointer;
}

func retain_range(pos range: Range<i32>) -> Range<i32>
{
    return range;
}

func transfer_future<T>(pos pending: Future<T>) -> Future<T>
{
    return consume pending;
}

async func finish_task<T>(pos task: Task<T>) -> RunResult<T>
{
    return await task.join();
}

func blocking_operation()
    requires(blocking_execution())
{
    perform_blocking_work();
}
```

The ambient surface includes scalar and structural type forms, `string`, integer ranges, raw pointers, result and conversion types, async and run-boundary types, execution predicates, compiler-known traits, special values, `core.memory`, and the `target` property path. Each entity still follows its owning type, expression, contract, memory, or target rules.

Use the closed [conformance catalog](https://github.com/lejmer/bray/blob/develop/docs/language/compiler-known-and-standard-library/conformance-catalog.md) when exact membership is needed.

See [compiler-known declarations](https://github.com/lejmer/bray/blob/develop/docs/language/compiler-known-and-standard-library/compiler-known-declarations.md) and the [compiler-known surface](https://github.com/lejmer/bray/blob/develop/docs/language/compiler-known-and-standard-library/compiler-known-surface.md).

## Compiler-provided declarations

A compiler-provided declaration is a compiler-known declaration whose implementation comes from the compiler rather than ordinary Bray source. Its public declaration surface remains ordinary and includes generic parameters, parameter names and modifiers, defaults, result type, contracts, capabilities, effects, const eligibility, availability, and overload participation.

A bodyless compiler-provided declaration in the specification describes the language contract rather than syntax that a package may write. User and standard-library packages cannot add, replace, overload, or implement compiler-provided declarations.

Calls use normal argument, generic, ownership, effect, trust, contract, overload, and evaluation-order rules. The compiler may lower the operation to generated instructions, a runtime call, a platform API, or another mechanism, but that choice cannot change observable Bray semantics.

The inherent `Future<T>.start()`, `Task<T>.join()`, and `Task<T>.cancel()` members and selected declarations under compiler-known core paths are examples. Source code uses their exposed contracts exactly like other visible declarations and never names their implementation bodies.

See [compiler-provided declarations](https://github.com/lejmer/bray/blob/develop/docs/language/compiler-known-and-standard-library/compiler-provided-declarations.md).

## Protected representation

Protected representation keeps compiler-owned invariants out of source-visible storage. It does not make a type unusable. When its public contract permits, source can name, move, borrow, pass, return, and store the value, and can match only through explicitly exposed patterns.

`Range<T>`, `Future<T>`, `Task<T>`, `PanicReport`, raw pointers, and selected support types use protected representation where their owning rules require it. Their public contracts supply their source operations while the compiler owns their representation invariants.

The pointer, future, and task operations in the first example use only public contracts. A source struct literal cannot construct those representations, field access cannot inspect them, and a same-shaped source type cannot substitute for them. Copying is available only when the type's owning language rule defines a copy contract.

See [protected representation](https://github.com/lejmer/bray/blob/develop/docs/language/compiler-known-and-standard-library/protected-representation.md).

## Compiler-known traits and operations

An ordinary package can implement a compiler-known trait when the trait permits it. The implementation remains an ordinary implementation declaration and participates only through normal visibility, coherence, overload-family, import, ownership, contract, and target rules.

```bray
struct Ticket
{
    id: u64;
}

impl TicketEquatable = Ticket(Equatable<Ticket>)
{
    func equals(pos other: &Ticket) -> bool
    {
        return self.id == other.id;
    }
}

func same_ticket(pos left: Ticket, pos right: Ticket) -> bool
{
    return left == right;
}
```

The `==` operator selects the exact compiler-known `Equatable<Ticket>` trait and its exact `equals` member identity. A different trait or member with the same spelling does not participate. The overloadable operator surface and its trait mappings form a closed compiler-known set.

A package dependency does not silently activate implementations of compiler-known traits. External implementations participate only when the implementation coherence rules make them available in the current domain.

See [compiler-known trait implementations](https://github.com/lejmer/bray/blob/develop/docs/language/compiler-known-and-standard-library/compiler-known-trait-implementations.md) and the [trait conversion and operator contracts](https://github.com/lejmer/bray/blob/develop/docs/language/types/traits.md).

## Recognized standard-library declarations

A recognized standard-library declaration is ordinary `std` source with a stable declaration identity known to the compiler. It remains subject to the selected toolchain package input, ordinary path resolution, `using`, visibility, overload participation, implementation coherence, and public compatibility.

```bray
module text_summary;

using std.string;

func summarize(pos value: &string) -> (usize, bool)
{
    let scalar_count: usize = std.string.scalar_count(value);
    let empty: bool = std.string.is_empty(value);

    return(scalar_count, empty);
}
```

These calls are recognized only because the visible declarations have the exact standard-library identities. A user declaration named `scalar_count`, a declaration in another package, or a different visible declaration with the same surface remains ordinary code.

Recognition can provide specified checking, lowering, optimization, const eligibility, or contract behavior without making a declaration ambient. Current recognized families include numeric conversion policy, string operations, callback state, and the standard-library memory, layout, allocation, uninitialized-storage, and anchored-borrow surfaces listed by the conformance catalog. Safe atomic wrappers, channels, operating-system threads, child processes, task combinators, synchronization owners, run checkpoints, and runtime-selection types remain ordinary standard-library declarations rather than recognized names.

Compiler lowering retains the recognized internal `std.runtime` allocation, owned-text, and character declarations by identity. These are ordinary Bray implementations, while their public operations keep the contracts of the owning visible declarations.

A bodyless standard-library declaration shown in the specification describes the required surface. It is not special declaration syntax for a standard-library package.

See [standard-library declarations](https://github.com/lejmer/bray/blob/develop/docs/language/compiler-known-and-standard-library/standard-library-declarations.md), [standard-library recognition](https://github.com/lejmer/bray/blob/develop/docs/language/compiler-known-and-standard-library/standard-library-recognition.md), and [recognized standard-library operations](https://github.com/lejmer/bray/blob/develop/docs/language/compiler-known-and-standard-library/recognized-standard-library-operations.md).

## Target-conditional availability

The compiler evaluates target availability before ordinary source checking. Compiler-known and recognized standard-library declarations that are unavailable for the selected target are absent from that product's usable surface and fail before code generation.

```bray
module target_summary
{
    const POINTER_BYTES: usize = target.pointer.BYTES;

    const NATIVE_SYSTEM: string = target.identity.SYSTEM;
}

@target(target.atomic.U64)
module target_counter
{
    const U64_ATOMICS_AVAILABLE: bool = true;
}

@target(!target.atomic.U64)
module target_counter
{
    const U64_ATOMICS_AVAILABLE: bool = false;
}
```

`target` is an ambient compiler-known path, not part of `std`. The `@target(...)` operand is an ordinary compile-time boolean expression over target properties, literals, and permitted built-in operations. It cannot call user code or inspect declarations from the source graph being selected.

A disabled module contribution is not used by that product, so it can pair a target-specific implementation with a negated fallback. Gating changes contribution selection only. It does not change module identity, declaration visibility, path resolution, trusted status, or runtime behavior.

Public signatures, contracts, layouts, constants, implementations, overloads, and availability that depend on target properties record those dependencies in compiled interface metadata.

See [target-conditional declarations](https://github.com/lejmer/bray/blob/develop/docs/language/compiler-known-and-standard-library/target-conditional-declarations.md) and [target profiles and properties](https://github.com/lejmer/bray/blob/develop/docs/language/targets-layout-abi-and-raw-memory/target-profiles-and-properties.md).

## Choose the declaration category

| Category                                | How source gains access                            | What the compiler knows                                        |
|-----------------------------------------|----------------------------------------------------|----------------------------------------------------------------|
| Compiler-known declaration              | Ambient when target-available                      | Exact language identity and contract                           |
| Compiler-provided declaration           | Ambient compiler-known surface                     | Exact contract and compiler-supplied implementation            |
| Protected-representation declaration    | Its ordinary compiler-known public surface         | Exact public contract plus compiler-private representation     |
| Compiler-known trait implementation     | Ordinary coherence and implementation rules        | Exact trait and member identities, not implementation spelling |
| Recognized standard-library declaration | Ordinary `std` package, path, and visibility rules | Exact stable declaration identity and specified behavior       |
| Ordinary source declaration             | Ordinary scope, path, import, and visibility rules | Only its checked source contract                               |

**Remember:** Compiler-known means exact language identity, not special spelling. Compiler-provided means the compiler supplies the implementation while source uses only its ordinary declaration surface. Protected representation hides invariant-bearing storage, not the public type. Recognized `std` declarations remain ordinary visible package declarations, and target availability is established before source uses them.

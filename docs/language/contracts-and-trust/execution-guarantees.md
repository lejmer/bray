# Conditional execution guarantees

`executes(pure, total)` declares execution properties as a comma-separated list of property names. The properties
are independent and can be declared separately. `when(condition) { ... }` groups guarantees under a predicate that
holds at execution entry.

```bray
func identity(pos value: bool) -> bool
    when(value)
    {
        executes(pure, total)
        ensures(result)
    }
{
    return value;
}
```

A group contains `executes(...)`, `ensures(...)`, and nested `when(...)` groups. The ordinary callable body defines
execution and produces the result.

These clauses apply to functions, methods, static methods, lambdas, constructors, lifecycle declarations, trait
requirements and defaults, callable types, and foreign declarations. Each declaration retains its ownership, result,
execution, ABI, and trust rules.

## Entry and completion

An unguarded execution property holds for every valid input satisfying the declaration's requirements. A guarded
property holds when its guard holds at execution entry. Nested guards are conjunctions at that same entry.
Every applicable group holds, independently of declaration order.

Guards follow the [predicate-expression rules](predicates-and-predicate-expressions.md). Their value inputs are the
declaration's parameters and receiver, where present. Common requirements and enclosing guards are available while
checking a group. `result` belongs to postcondition context.

A group's postconditions describe normal-exit state and the returned value. Its entry state determines which
guarantees apply throughout execution, including when the body mutates a value observed by the guard.

A caller can use a guarded guarantee when available conditions prove its entry predicate. This is a compile-time
decision. Ordinary requirement checks follow the [precondition rules](preconditions-and-postconditions.md).

## Pure execution

`pure` execution observes its permitted inputs and computes values without runtime effects. Mutation, including
mutation of internal temporary storage, allocation, deallocation, I/O, synchronization, task operations, inactive
async-frame creation, suspension, and cancellation observation make execution impure. So do observations of changing
ambient information through clocks, entropy, mutable global state, addresses, or scheduler state.

Computing with a supplied timestamp or seed can be pure. Obtaining fresh entropy or advancing mutable generator
state is impure.

Purity is determined by the selected operations and their observation dependencies. Selected construction, local
cleanup, and owned-input cleanup contribute to execution effects. Ownership transfer follows the ordinary ownership
rules. Later cleanup of a returned owner belongs to its caller.

Uniform [cleanup capacity](../async-and-concurrency/async-representation-and-storage.md#cleanup-capacity) admission and
discharge contribute their actual effects. A current-value completion proof can omit a finalizer invocation without
making the owner's construction or disposal pure. Ordinary moves transfer the existing allowance without new admission.

Forming a pointer to protected storage with `uninit_pointer` or `uninit_pointer_mut`, and deriving an anchored
borrow with `borrow_from` or `borrow_mut_from`, are pure and total on their valid input domains. The pointer and
borrow retain the source storage and capability dependencies. Exposing or comparing numerical addresses remains
an address observation.

## Total execution

`total` execution terminates normally for valid inputs satisfying the declaration's requirements, including
owned-input and local cleanup. A reachable uncaught language panic, current-run cancellation, divergence, or unresolved
cleanup exit violates this guarantee. Process termination and hardware failure are outside Bray's execution model.

Every returned result case, including `Result.Error`, counts as normal completion. Postconditions describe the
result case and state guaranteed at completion.

Termination verification follows finite control flow, supported bounded iteration, and well-founded selected call
dependencies. Each participating callable and cleanup step needs verified evidence. The compiler rejects live
recursive proof cycles, including cycles through generic witnesses and implicit cleanup. A recursive function can
promise `total` execution on a guarded base-case domain whose reachable execution has well-founded dependencies.

Open generic dependencies remain attached to the proof and are checked with the actual implementation witnesses.
Certifying a new `total` body requires resolved, well-founded dependencies for its selected dynamic targets.
Callable values can carry already-verified guarantees for ordinary indirect calls.

## Relationship to `const`

[`const`](../callables/const-functions.md) makes a callable eligible for constant evaluation. Its body, operations,
arguments, and result must satisfy the rules of the constant or predicate context in which it is used. Public use
in those contexts requires an exposed `const` declaration.

`pure` describes the effects of ordinary execution on the promised input domain. That execution can observe
permitted runtime inputs and transfer owners under the ordinary ownership rules. A guarded `pure` promise can
cover one input domain while the same body performs effects for other inputs. Termination is the separate
`total` property.

Checked `const` eligibility establishes the corresponding `pure` and `total` properties on its valid input domain.

Note: `executes(pure)` is not conditional `const`. Even `executes(pure, total)` leaves constant-evaluation and
predicate-call eligibility subject to their own rules.

## Checking and interfaces

The compiler checks the ordinary body under common requirements and each declared guard domain. Reachable operations,
branches, matches, calls, returns, and cleanup must satisfy every applicable promise. Acceptance requires sufficient
evidence for each guarantee, including the contracts of selected operations and implicit lifecycle steps.

All Bray source bodies undergo this verification, including `trusted` bodies. A trusted foreign declaration can
assert guarantees across the foreign boundary. Those assertions carry the declaration's trust obligations.

Compiled interfaces retain entry guards, guarded postconditions, execution properties, proof provenance, and proof
dependencies. Separately compiled providers supply validated compatible evidence. Foreign assertions remain
distinguished from checked Bray proofs and preserve `extern`, module trust, ABI, `foreign_call`, and caller-obligation
contracts. Compiler-known declarations must also satisfy their closed role contracts.

Callable conversions may forget guarantees. Every guarantee in the target contract must follow from the source
contract. Trait implementations establish each required guarantee on its required input domain and may provide
additional guarantees. Conformance uses bounded implication, with acceptance requiring a proven implication.
Generic substitution and dynamic dispatch preserve these requirements. Concrete type-wide lifecycle selection
follows the [lifecycle selection rules](../lifecycle/lifecycle-selection.md).

## Async execution and finalization

An async call creates an inactive owned `Future<T>`. Its execution guarantees apply when its body begins execution,
and its postconditions become available after normal completion. Explicit `await` requires an async context.
Execution-lane requirements, task-start boundaries, cancellation checkpoints, and ownership obligations follow the
ordinary async rules.

The [finalization rules](../lifecycle/finalization.md) use verified `pure` and `total` execution with a known `unit`
or `Ok(unit)` outcome to discharge an already-completed whole-value graceful step before optimization.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- [Contract clauses](contract-clauses.md)
- [Finalization](../lifecycle/finalization.md)

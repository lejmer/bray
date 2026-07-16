# Effects and capabilities

Function signatures include effects and capability contracts.

A function can use only the capabilities available through its parameters, local bindings, including pattern-introduced bindings,
generic constraints, execution mode, lifecycle state, trusted declarations, and surrounding context.

An effect is a semantic property of evaluating, driving, or resolving a callable that matters to call checking, context validity,
generic satisfaction, dynamic dispatch, or public API compatibility.

The caller-visible effect surface is:

- `const`,
- `async`,
- receiver and parameter modes,
- `requires(...)`,
- `ensures(...)`,
- `with(...)`,
- trusted `uses(...)`,
- lifecycle contracts,
- task and async-computation contracts,
- parameter and result type contracts.

Effects and capability contracts are part of:

- function signatures,
- function types,
- call checking,
- behavioral contract satisfaction,
- generic constraints,
- dynamic dispatch,
- public API compatibility.

The compiler computes a body effect summary for each callable body.

The body effect summary is derived from:

- selected callable contracts of calls and method calls,
- selected lifecycle declaration contracts,
- construction, destruction, finalization, and resource-scope behavior,
- assignments and mutation,
- allocation and deallocation,
- I/O,
- async computation creation, `await`, task start, task joins, and task cancellation,
- panic-catching boundaries,
- trusted capability use.

The computed body effect summary must be valid for the callable's declaration surface and surrounding context.

Constant-evaluation eligibility is declared with the `const` function modifier.

`requires(...)` declares caller obligations and preconditions. For an async callable, requirements about supplied values and
invocation state are checked while constructing the frame, while requirements about the execution context are carried by the
resulting computation until execution.

`ensures(...)` declares established facts after normal completion.

`uses(...)` declares trusted implementation capabilities used by a trusted callable body.

A trusted callable's `uses(...)` clause must exactly describe the trusted implementation capabilities used by its body.

Trusted implementation capabilities cover low-level operations named by the trust rules.

Ordinary safe allocation, ordinary I/O, and ordinary mutation are checked as body effects and as caller-visible effects when they
cross the callable boundary or are constrained by the surrounding context.

### Mutation effects

Mutation authority is represented by receiver and parameter modes.

Caller-reachable mutation occurs when a callable can mutate storage reachable by the caller before or after the call.

Caller-reachable mutation must be visible through one of:

- a mutable receiver mode,
- a mutable borrow parameter,
- an owned parameter consumed by the callable,
- a global or module storage contract,
- a trusted raw-memory contract,
- a type, lifecycle, or trait contract that exposes mutation authority.

Internal mutation occurs when a callable mutates storage that is created inside the callable or owned exclusively by the callable
and is not reachable by the caller except through the callable's returned value.

Internal mutation is still a runtime effect.

Constant-evaluation context, predicate-expression context, static constraint context, and other effect-free contexts require
callables without internal mutation.

Internal mutation is represented in callable types only when it creates a caller-visible requirement through the ordinary callable
surface.

### Allocation and I/O effects

Allocation creates runtime storage or asks a storage policy to create runtime storage.

Deallocation releases runtime storage.

Safe allocation and safe deallocation are ordinary runtime effects.

Low-level allocation, raw allocation facts, and allocator manipulation require trusted capabilities such as `manual_alloc` or
`raw_memory` when the selected operation's contract names those capabilities.

Constant-evaluation context, predicate-expression context, static constraint context, and other allocation-free contexts require
callables without allocation or deallocation.

Allocation and deallocation become caller-visible when the callable's signature, type contracts, lifecycle contracts, or result
obligations require the caller to preserve, destroy, finalize, join, cancel, or otherwise resolve storage produced by the call.

I/O is any interaction with external state outside the Bray abstract machine, including files, terminals, network connections,
devices, clocks, environment state, randomness, and foreign callbacks with externally visible behavior.

I/O enters Bray through compiler-known, standard-library, foreign, or user declarations whose contracts describe the resource,
capability, ownership, borrowing, finalization, and panic behavior involved.

Constant-evaluation context, predicate-expression context, static constraint context, and other effect-free contexts require
callables without I/O.

I/O becomes caller-visible when the callable's signature or contracts require an I/O resource, return an I/O resource, mutate an
I/O resource, transfer an I/O obligation, or state facts about external behavior.

### Cancellation effects

Cancellation is an async and run-boundary effect.

An `async` callable type carries suspendable execution and cancellation participation.

Calling an async function creates an async computation whose cancellation behavior is governed by [Async and concurrency](../async-and-concurrency.md).
Body effects, body capabilities, execution-context requirements, and normal-completion facts belong to the computation's execution
contract; they are not effects or facts of inactive-frame construction.

Awaiting an async computation, starting it as a task, joining a task, cancelling a task, and observing a run boundary must satisfy
the async computation's ownership, borrowing, capability, effect, finalization, and cancellation obligations.

A synchronous callable can request task cancellation only by constructing or transferring an async cancellation computation; it
cannot drive that computation or end an unresolved task obligation without an async execution context.

That authority is represented by the parameter, receiver, or field type that carries the task obligation.

Task cancellation is represented through `async`, task-handle ownership, and the contracts of values that carry cancellation
authority.

`blocking_execution()`, `compute_execution()`, and `main_thread_execution()` are compiler-provided context predicates used in
`requires(...)`. For synchronous calls they are immediate preconditions. Async invocation defers them into `Future<T>` because
invocation does not execute the body; direct await validates them against the current lane and task start selects a satisfying
lane. The same phase distinction applies to body effects and capabilities: direct await requires them from the current execution
context, while task start proves that the selected lane and every dependency transferred into it satisfy them.

### Effects in callable types

A callable type includes every caller-visible contract clause needed to call a value of that type.

Two callable declarations with the same parameter and result shape but incompatible caller-visible contracts have different
callable types.

Callable types represent caller-visible effects through the ordinary callable type surface:

- `const` for constant-evaluation eligibility,
- `async` for suspendable execution and cancellation participation,
- `@abi(...)` for explicit callable ABI contracts,
- receiver and parameter modes for ownership, borrowing, movement, and mutation requirements,
- trusted `uses(...)` for trusted implementation capability envelopes that must be preserved,
- contract clauses for preconditions, postconditions, static constraints, trusted caller obligations, facts, and resource obligations,
- parameter and result types for task handles, async computations, storage obligations, lifecycle obligations, and resource
  ownership.

Callable type assignment, trait implementation checking, dynamic dispatch, and public API compatibility preserve caller-visible
effects.

An ordinary runtime callable that allocates internally, mutates internal temporary storage, or performs internal safe bookkeeping can
match an ordinary runtime callable type when those effects remain internal and impose no caller-visible obligations.

A callable can match a callable type or context when its body effect summary is valid for every effect requirement of that type or
context.

After overload resolution selects exactly one overload arm by explicit argument mapping and type compatibility, receiver rules,
explicit generic substitution and static constraints, and target availability, contract checking verifies that the caller's context
satisfies the selected arm's caller-visible obligations.

Overload resolution does not rank or distinguish overloads by effect or capability contracts.

If multiple overloads remain applicable after argument matching, the call is ambiguous.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Async functions and computations](async-functions-and-computations.md)
- Next: [Visibility and paths](visibility-and-paths.md)

# Lifecycle and foreign boundaries

Trust rules apply to lifecycle declarations.

Constructors, finalizers, destructors, `enter`, and `exit` declarations use the same contract split as callables:

- `uses(...)` declares trusted implementation capabilities,
- `requires(...)` declares caller or lifecycle preconditions,
- `ensures(...)` declares established facts,
- trusted requirements must be propagated, discharged, established, or acknowledged.

Lifecycle declaration syntax and ordering are defined by the lifecycle rules.

Foreign boundaries are trust boundaries when Bray code interacts with code, storage, callbacks, or runtime behavior outside Bray's ordinary semantics.

Foreign boundaries include:

- calls from Bray into an extern callable,
- calls from foreign code into an exported ABI callable,
- foreign callbacks invoked through ABI-qualified callable values,
- foreign APIs that read, write, initialize, retain, release, or alias Bray-accessible storage,
- foreign APIs that create, consume, or transfer resource handles,
- foreign runtime behavior that can reenter Bray.

Callable ABI, `extern`, `@abi(...)`, `@link(...)`, and `@symbol(...)` rules are defined in [Callable ABI and FFI](../callables/callable-abi-and-ffi.md).

Raw pointer facts, raw memory predicates, allocation facts, and ABI layout helpers are defined by the raw memory rules.

An extern callable with a foreign ABI is a trusted declaration.

The extern declaration is the complete Bray-visible contract for the foreign callable.

The compiler checks calls to an extern callable against that declaration exactly like calls to ordinary Bray callables.

The foreign implementation body is not Bray source and is not type checked as Bray source.

A foreign callable that requires trusted facts must state them as trusted requirements.

Declarations for foreign APIs must describe every caller-visible foreign obligation, including pointer validity, alignment, initialization state, byte count, element count, lifetime behavior, ownership transfer, aliasing permissions, thread-affinity requirements, synchronization requirements, callback reentrancy behavior, resource acquisition and release obligations, ordinary error return conventions, and panic boundary behavior.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Obligation propagation](obligation-propagation.md)
- Next: [Summary](summary.md)

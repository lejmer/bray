# Lifecycle and foreign boundaries

Trust rules apply to lifecycle declarations.

Constructors, finalizers, destructors, `enter`, and `exit` declarations use the same contract split as callables:

- `uses(...)` declares trusted implementation capabilities,
- `requires(...)` declares caller or lifecycle preconditions,
- `ensures(...)` declares established conditions,
- trusted requirements must be propagated, discharged, established, or acknowledged.

Lifecycle declaration syntax and ordering are defined in [Lifecycle](../lifecycle.md).

Foreign boundaries are trust boundaries when Bray code interacts with code, storage, callbacks, or runtime behavior
outside Bray's ordinary semantics.

Foreign boundaries include:

- calls from Bray into an extern callable,
- calls from foreign code into an exported ABI callable,
- foreign callbacks invoked through ABI-qualified callable values,
- foreign APIs that read, write, initialize, retain, release, or alias Bray-accessible storage,
- foreign APIs that create, consume, or transfer resource handles,
- foreign runtime behavior that can reenter Bray.

Callable ABI, `extern`, `@abi(...)`, `@link(...)`, and `@symbol(...)` rules are defined in
[Extern declarations and FFI](../targets-layout-abi-and-raw-memory/extern-declarations-and-ffi.md).

Raw pointer conditions, raw memory predicates, allocation conditions, and ABI layout helpers are defined in
[Targets, layout, ABI, and raw memory](../targets-layout-abi-and-raw-memory.md).

An extern callable with a foreign ABI is a trusted declaration.

The extern declaration is the complete Bray-visible contract for the foreign callable.

The compiler checks calls to an extern callable against that declaration exactly like calls to ordinary Bray callables.

The foreign implementation body is not Bray source and is not type checked as Bray source.

A foreign callable that requires trusted guarantees must state them as trusted requirements.

An extern callable resolved to a separately compiled Bray artifact is not a foreign boundary merely because it is
extern. Its private ABI and linkage contract still require validation, but `foreign_call` applies only when execution
crosses a foreign ABI.

Declarations for foreign APIs must describe every caller-visible foreign obligation, including pointer validity,
alignment, initialization state, byte count, element count, lifetime behavior, ownership transfer, aliasing permissions,
thread-affinity requirements, synchronization requirements, callback reentrancy behavior, resource acquisition and
release obligations, ordinary error return conventions, and panic boundary behavior.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Obligation propagation](obligation-propagation.md)
- Next: [Summary](summary.md)

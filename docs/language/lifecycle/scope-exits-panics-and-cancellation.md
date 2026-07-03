# Scope exits, panics, and cancellation

Scope exit resolves local ownership and lifecycle state for the scope being left.

Initialized local owned values whose ownership remains in the scope are resolved when the scope exits.

Values moved out of the scope are not resolved as local owned values.

Partially initialized local values resolve only initialized represented parts.

Reachable exits from a region must merge to coherent ownership, borrowing, initialization, destruction, finalization, capability, effect, task-obligation, and fact-context state.

Panic propagation and cancellation use the same lifecycle ordering as ordinary scope exit.

When a panic exits a region, local lifecycle obligations are resolved according to the same ordering as ordinary non-panic exits unless a more specific async or run-boundary rule applies.

When cancellation exits an async computation or run boundary, owned captured state is resolved according to async cancellation, ownership, destruction, finalization, and capability rules.

If an async block owns live task obligations and a panic reaches the block boundary, the async block cancels those tasks before the panic continues according to [Async block expressions](../async-and-concurrency/async-block-expressions.md).

Fallible finalization that returns `Result.Error` leaves the finalization obligation unresolved.

A value with an unresolved finalization obligation cannot be destroyed.

If a scope cannot resolve a lifecycle obligation, transfer it to a valid owner, or convert it into an explicit fallback ownership form, the program is rejected.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Partial values and replacement](partial-values-and-replacement.md)
- Next: [Lifecycle requirements in traits](lifecycle-requirements-in-traits.md)

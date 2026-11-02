# Lifecycle requirements in traits

Traits can require `finalize`, `destruct`, `enter`, or `exit` behavior.
[Lifecycle requirements in traits](../types/traits.md#lifecycle-requirements-in-traits) defines the requirement syntax,
signatures, contracts, and fulfillment rules.

[Lifecycle selection](lifecycle-selection.md) defines selection of concrete finalizers and destructors and of exact
trait implementation `enter` and `exit` fulfillments. The trait rules preserve the distinction between type-wide
behavior and implementation-specific scoped behavior.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Scope exits, panics, and cancellation](scope-exits-panics-and-cancellation.md)
- Next: [API compatibility](api-compatibility.md)

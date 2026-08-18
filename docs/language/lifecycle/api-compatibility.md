# API compatibility

Changing lifecycle declarations can be a public API change when ownership, destruction, finalization, construction, or
scoped-use behavior changes.

Adding a lifecycle declaration can change:

- when callers must finalize a value,
- when callers can destroy a value,
- whether a value can be copied,
- whether a value can be used in a `with` expression,
- which effects or trusted obligations are required,
- which failures or async behavior callers must handle.

Removing a lifecycle declaration can remove behavior that callers, traits, or implementations depend on.

Changing a lifecycle signature, execution mode, result type, contract clause, trusted capability use, or visible
obligation changes the lifecycle declaration surface.

Changing represented-part destruction order or whole-value lifecycle ordering is a public semantic change when it can
affect observable behavior.

Trait lifecycle requirements are part of the trait contract.

Changing a trait lifecycle requirement can change which implementations satisfy the trait.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Lifecycle requirements in traits](lifecycle-requirements-in-traits.md)
- Next: [Summary](summary.md)

# Summary

Constructors create fully initialized values.

Finalizers complete fallible or asynchronous lifecycle obligations before ownership ends.

Destructors perform synchronous infallible cleanup when ownership ends.

`enter` and `exit` define scoped capability behavior for `with` expressions.

Lifecycle ordering is `construct`, ordinary use, scoped enter and exit, finalization, destruction, and represented-part
destruction.

Partial values do not run whole-value lifecycle behavior.

Whole-value lifecycle behavior requires full initialization at every point where that behavior can run.

Panic propagation and cancellation resolve lifecycle obligations according to the same ordering as ordinary scope exit,
with [async and run-boundary rules](../async-and-concurrency.md) where applicable.

Materialized product and thread-local statics are resolved exactly once by their product host or exact native-thread
attachment in deterministic dependency order before the infrastructure required by cleanup shuts down.

Trait lifecycle requirements constrain implementing subjects.

Lifecycle declarations are part of public API compatibility when they affect construction, finalization, destruction,
scoped use, effects, obligations, or ownership behavior.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [API compatibility](api-compatibility.md)

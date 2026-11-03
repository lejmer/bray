# Summary

This summary is informative. The detailed rules include:

- [lifecycle selection](lifecycle-selection.md).
- [lifecycle ordering](lifecycle-ordering.md).
- [lifecycle requirements in traits](lifecycle-requirements-in-traits.md).

Construction establishes a fully initialized value. During ordinary use, `enter` and `exit` provide scoped capability
behavior for `with` expressions. Before ownership ends, finalization completes fallible or asynchronous obligations.
Finalization is followed by synchronous infallible destruction, then represented-part destruction.

Partial values do not run whole-value lifecycle behavior, which requires full initialization wherever it can run.
Panic propagation and cancellation resolve obligations in the same order as ordinary scope exit, with the
[async and run-boundary rules](../async-and-concurrency.md) where applicable.

Materialized product and thread-local statics are resolved exactly once by their product host or exact native-thread
attachment. Resolution follows deterministic dependency order before the infrastructure required by that cleanup shuts
down.

Trait lifecycle requirements constrain implementing subjects. Lifecycle declarations affect public API compatibility
when they govern construction, finalization, destruction, scoped use, effects, obligations, or ownership behavior.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [API compatibility](api-compatibility.md)

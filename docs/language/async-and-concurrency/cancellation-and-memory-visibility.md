# Cancellation and memory visibility

Cancellation requests are observed by tasks and threads through cancellation points and operation contracts.

A cancellation request does not grant direct access to the cancelled run's captured storage.

Cancellation does not interrupt an atomic operation at a partial state.

Cancellation does not interrupt a non-cancellable operation at an arbitrary source point.

When cancellation completes, all destruction, finalization, capability release, and synchronization behavior required by the cancelled state has completed or has been transferred according to the cancelled run's contract.

After cancellation completes, the cancelling run observes the completion edge produced by cancellation.

If cancellation runs cleanup while a scoped synchronization capability is held, cleanup must release that capability according to the same `exit`, destruction, and finalization rules that apply to ordinary scope exit.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Atomic operation contracts](atomic-operation-contracts.md)
- Next: [Capability transfer across run boundaries](capability-transfer-across-run-boundaries.md)

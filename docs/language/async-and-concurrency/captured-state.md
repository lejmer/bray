# Captured state

Suspension captures every live value, borrow, capability, effect, fact dependency, and finalization obligation needed to resume the async computation.

A value moved into an async computation is owned by that computation until the value is returned, moved elsewhere, destroyed, or transferred into a spawned task.

A borrow captured by an async computation remains active for the computation's lifetime.

A mutable borrow captured by an async computation remains exclusive for the computation's lifetime.

A scoped capability captured by an async computation remains held for the computation's lifetime.

An async computation cannot outlive a borrowed value or scoped capability it captures.

The compiler tracks captured state across:

- suspension,
- movement,
- await,
- spawn,
- cancellation,
- destruction,
- task transfer.

Captured state contributes to the async computation's dependency contract.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Async functions and computations](async-functions-and-computations.md)
- Next: [Await expressions](await-expressions.md)

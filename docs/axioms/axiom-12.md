## Axiom 12: Asynchronous execution is structured and first-class

Asynchronous execution is part of the language's semantic model.

A synchronous computation completes at the point where it is evaluated.

An asynchronous computation is an owned value representing suspended or suspendable execution. It contains the state required to complete according to its type, ownership, lifetime,
capability, and effect contract.

Calling an asynchronous function creates an asynchronous computation. Awaiting an asynchronous computation drives it to completion and produces its declared result.

Suspension captures the values, borrows, capabilities, and effects that remain live across the suspension point. Captured state becomes part of the asynchronous computation's ownership
and lifetime contract.

A value moved into an asynchronous computation is owned by that computation until it is returned, moved elsewhere, or destroyed. A value borrowed by an asynchronous computation remains
borrowed for the lifetime declared by that computation.

Spawned asynchronous work is structured by default. A spawned computation belongs to a task scope that owns its lifetime, completion, cancellation, and destruction behavior.

Detached asynchronous work uses an explicit ownership-extending task handle. The handle represents responsibility for joining, cancelling, or completing the detached computation
according to its contract.

Destroying an incomplete asynchronous computation cancels it. Cancellation destroys owned captured state and releases held capabilities according to ordinary destruction rules.

Ordinary destruction is synchronous.

A value may carry an asynchronous finalization obligation as part of its type contract. An asynchronous finalization obligation must be completed, transferred, or converted into an
explicit fallback ownership form before the owning scope exits.

A value with an asynchronous finalization obligation is tracked by the compiler like ownership, initialization, movement, and destruction state. Every exit path from the owning scope must
account for the obligation.

Asynchronous behavior is part of behavioral contracts. A contract method is synchronous or asynchronous as part of the contract it declares.

Low-level async runtime machinery is part of the trusted substrate. Executors, reactors, wakers, completion queues, foreign async callbacks, device async integration, and custom scheduling
primitives are implemented through trusted capabilities and exposed through safe async contracts.

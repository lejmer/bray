# Callable-body block expressions

A **callable-body block expression** is the block expression used as the body of a callable program element.
Functions, lambdas, and async functions have callable-body block expressions.

[Callable execution scopes](../callables/return-and-execution-scopes.md#callable-execution-scopes) define the scope of
these bodies. [Callable results and bodies](../callables/callable-results-and-bodies.md) define omitted result types,
explicit result syntax, and normal completion. The
[nested return and yield example](../callables/return-and-execution-scopes.md#callable-result-exits) illustrates their
different targets.

A callable-body block expression introduces a scope for local bindings, ownership tracking, borrow tracking,
destruction, finalization tracking, capability checking, effect checking, and condition refinement.

Local bindings introduced inside the callable-body block expression are visible according to ordinary block-expression
scope rules.

Local owned values whose ownership remains in the callable-body block expression are resolved when the callable-body
block expression exits. In async execution, this includes structured task and asynchronous-finalization obligations.

Callable-body exits include:

- normal completion,
- `return`,
- uncaught panic propagation,
- nullable propagation whose target boundary is the callable execution scope,
- result propagation whose target boundary is the callable execution scope,
- run-result propagation whose target boundary is the callable execution scope,
- cancellation of the current async computation,
- any expression path with type `never` that prevents the callable body from continuing.

`yield`, `break`, and `continue` are not callable-body exits merely because they occur inside a callable body.

They leave the callable-body block expression only when the region they target or the enclosing expression path that
contains them also leaves the callable body.

Return operand evaluation and result transfer follow
[Return expressions](return-expressions.md#evaluation-and-ownership).

Every returned value meets [return type acceptance](return-expressions.md). Each reachable exit resolves the obligations
for that exit under [scope exits and ownership boundaries](../ownership-and-borrowing/scope-exits-and-ownership-boundaries.md).
Paths that reach a shared continuation follow [flow-state merging](expression-results.md#flow-state-merge).
A callable must establish its declared completion guarantees on every exit to which those guarantees apply.

For async callables, the callable-body block expression is checked in an active async execution context and is the root
lexical task scope for that invocation. Every nested ordinary block is also a structured task ownership boundary.

[Async computations](../async-and-concurrency/async-functions-and-computations.md) define invocation and inactive
frames. [Async representation and storage](../async-and-concurrency/async-representation-and-storage.md) defines the
state retained across suspension. [Structured task scope exit](../async-and-concurrency/structured-task-scope-exit.md)
defines cancellation broadcast and lifecycle resolution for async exits.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Block expressions](block-expressions.md)
- Next: [Local binding declarations inside block expressions](local-binding-declarations-inside-block-expressions.md)

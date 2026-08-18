# Expression destruction and finalization behavior

Expressions can trigger destruction and finalization obligations through scope exit, assignment, movement, consumption,
cancellation, and resource-scope behavior.

Assignment ends the previous destination value according to destruction and lifecycle rules.

Leaving a block expression destroys local owned values whose ownership remains in the block expression scope.

A value with a finalization obligation must be finalized, transferred to another owner that assumes the obligation, or
converted into an explicit fallback ownership form before the owning scope exits.

Async finalization is completed through asynchronous execution.

Ordinary destruction remains synchronous.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Expression initialization behavior](expression-initialization-behavior.md)
- Next: [Flow-sensitive contract reasoning](flow-sensitive-contract-reasoning.md)

# Thread entry state

The thread entry callable selected by `spawn thread` is not a bound-method value and does not carry lambda capture state.

Thread entry state is supplied only by the receiver and arguments of the `spawn thread` entry application.

For a function or static function entry, the entry state is the bound parameter set.

For a callable-value entry, the callable value is evaluated as the callee and the entry state is the callable value plus the bound parameter set.

For a method entry, the entry state is the evaluated receiver plus the bound parameter set.

A receiver or argument that is passed by ownership becomes owned by the thread entry state unless the value is copied by its copy contract.

A receiver or argument that is passed by shared borrow keeps the reached storage shared-borrowed for the lifetime represented by the thread handle.

A receiver or argument that is passed by mutable borrow keeps the reached storage mutably borrowed for the lifetime represented by the thread handle.

While the thread handle is live, the creating run cannot perform operations that conflict with borrow, ownership, capability, finalization, or fact dependencies carried by the thread entry state.

Joining the thread resolves the thread obligation and releases or returns entry-state dependencies according to the selected callable's contract and the join result.

Cancelling the thread resolves the thread obligation by cancelling the run, destroying initialized owned entry state, releasing entry capabilities, and preserving any obligations that the cancellation contract transfers to the cancellation result or caller.

Moving a thread handle transfers the thread obligation and every dependency carried by the thread entry state.

If the destination cannot preserve those dependencies, the move is rejected.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Thread spawn expressions](thread-spawn-expressions.md)
- Next: [Thread handles and obligations](thread-handles-and-obligations.md)

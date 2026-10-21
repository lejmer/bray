# Const functions

A function can be declared with the `const` modifier:

```bray
const func max(pos a: i32, pos b: i32) -> i32
{
    if a >= b
    {
        return a;
    }

    return b;
}
```

`const func` means the callable body is valid in constant-evaluation context.

A const function is valid in constant expressions, predicate expressions, contract expressions, and static constraint
contexts when its arguments are valid in that context and its ordinary callable contract is satisfied.

A const function must be pure, deterministic, total for valid inputs, terminating, observational, effect-free, and
allocation-free.

A const function can compute with its supplied values and mutate storage confined to its own computation, subject to
the [purity rules](../contracts-and-trust/execution-guarantees.md#pure-execution). A local reference does not establish
that its referent is local. The proof includes selected calls, cleanup and every completion path.

A const function cannot mutate storage observable outside the computation, read changing ambient state, perform I/O,
start tasks, await, suspend, catch or raise panics as runtime behavior, use effectful runtime dynamic dispatch, or depend
on runtime identity.

The body of a const function is checked in constant-evaluation context.

Every reachable result-producing path must use only expressions, operations, and calls valid in constant-evaluation
context.

Callable declarations and lambda expressions invoked in constant expressions, predicate expressions, contract
expressions, or static constraints must expose an explicit `const` contract. This applies to direct calls and calls
through callable values, aliases, selected implementations, and compiled interfaces.

Ordinary runtime functions can call const functions with runtime arguments without being declared `const`.
A callable's body availability, visibility, or `pure` and `total` guarantees do not supply a const contract.

When a declaration is exported or otherwise used through compiled interface metadata, const-context callers can depend
on that declaration only when its interface exposes `const`.

[Compiler-known declarations](../compiler-known-and-standard-library/compiler-known-declarations.md) and
[recognized standard-library declarations](../compiler-known-and-standard-library/standard-library-recognition.md) can
be `const` when their language-defined declaration contract marks them as `const`.

`const` composes with ordinary function modifiers only where the combined contract is valid.

`trusted const func` is valid only when the trusted capability use is itself permitted by constant-evaluation and trust
rules.

`async const func` is rejected.

Async evaluation implies suspension or runtime execution machinery, which is not valid in constant-evaluation context.

`const` is part of a callable's caller-visible contract.

Changing whether a public function exposes `const` is a public API change.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Callable ABI and FFI](callable-abi-and-ffi.md)
- Next: [Callable results and bodies](callable-results-and-bodies.md)

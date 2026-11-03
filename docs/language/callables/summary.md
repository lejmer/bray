# Summary

This summary is informative. The detailed rules include:

- [callable results and bodies](callable-results-and-bodies.md).
- [return and execution scopes](return-and-execution-scopes.md).
- [parameters](parameters.md).

Functions use `func`, with a result type introduced by `->` or an omitted result type of `unit`. Their bodies are block
expressions in callable-body context. `return` supplies a result and exits the nearest callable execution scope, while
`yield` supplies a value to a yield-capable region.

Parameters are bindings declared by signatures. Owned parameters are immutable by default, and arguments are named by
default. `pos` permits positional arguments and must form an initial run in the parameter list. `mut` before a parameter
name applies to the owned binding, while `mut` after `&` applies to storage reached through that borrow layer.

An ellipsis after fixed parameters declares a target-supported variadic foreign callable contract. Trailing arguments
are positional and use the selected ABI's exact default promotions.

Callable types use `func(...) -> ...`, and callable values carry their full contracts, including ABI and caller
obligations. Higher-order functions preserve those obligations. Async functions declared to return `T` produce owned,
inactive `Future<T>` values when called.

Trusted implementation power and trusted caller obligations are distinct. Function contracts use the ordinary contract
and trust rules.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Methods](methods.md)

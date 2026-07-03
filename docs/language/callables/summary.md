# Summary

Functions use `func`.

Callable result types use `->`.

An omitted callable result type means `unit`.

Function bodies are block expressions in callable-body context.

Callable result values are supplied explicitly through `return`.

`return` exits the nearest callable execution scope.

`yield` supplies values to yield-capable regions.

Parameters are bindings declared by function signatures.

Owned parameters are immutable by default.

Parameters are named by default.

The `pos` parameter modifier permits positional arguments for that parameter.

`pos` parameters form an initial run in the parameter list.

`mut` before a parameter name applies to an owned local parameter binding.

`mut` after `&` applies to the storage reached by that borrow layer.

Function types use `func(...) -> ...`.

Function values carry their full callable contract.

Callable ABI is part of a callable's contract.

Higher-order functions preserve caller obligations.

Async functions produce owned async computations.

Trusted implementation power and trusted caller obligations are distinct.

Function contracts integrate with the contract and trust rules.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Methods](methods.md)

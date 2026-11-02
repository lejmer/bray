# Lambda expressions and anonymous callable expressions

Syntax follows the [lambda expressions grammar](../syntax-grammar.md#lambda-expressions).

An **anonymous callable expression** creates a callable value with `lambda`.

```bray
let increment = lambda (pos value: i32) -> i32
{
    return value + 1;
};
```

[Lambda callable rules](../callables/lambda-expressions-and-anonymous-callables.md) define parameters, omitted result
types, modifiers, ABI qualification, receiver restrictions, and the prohibition on capturing enclosing local bindings.
[Callable execution scopes](../callables/return-and-execution-scopes.md#callable-execution-scopes) define lambda scope.

Evaluating a lambda expression creates the callable value without copying, moving, or borrowing enclosing local state.

The lambda body is not evaluated when the lambda expression is evaluated.

The lambda body is evaluated when the callable value is called.

The callable value produced by a lambda has the callable type described by its parameters, result type, execution mode,
contract clauses, and trusted obligations.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [With expressions](with-expressions.md)
- Next: [Expression effects and capabilities](expression-effects-and-capabilities.md)

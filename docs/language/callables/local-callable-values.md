# Local callable values

Function declarations are declaration forms, not block-expression items.

They do not appear inside callable-body block expressions or ordinary block expressions.

Named reusable behavior belongs at module scope, type scope, implementation scope, or trait scope.

Local callable behavior is expressed with a lambda value bound to a local binding.

```bray
func outer() -> i32
{
    let inner = lambda () -> i32
    {
        return 1;
    };

    return inner();
}
```

A lambda introduces its own callable execution scope.

A `return` inside the lambda exits the lambda.

Block expressions support local binding declarations.

They do not support nested named function declarations, nested type declarations, nested trait declarations, nested implementation
declarations, nested module declarations, or nested package declarations.

Lambdas are capture-free.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Visibility and paths](visibility-and-paths.md)
- Next: [Lambda expressions and anonymous callables](lambda-expressions-and-anonymous-callables.md)

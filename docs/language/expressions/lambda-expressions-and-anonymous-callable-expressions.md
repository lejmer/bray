# Lambda expressions and anonymous callable expressions

An **anonymous callable expression** creates a callable value.

Anonymous callable expressions use `lambda`.

```bray
let increment = lambda (pos value: i32) -> i32
{
    return value + 1;
};
```

A lambda has the form:

```bray
lambda (parameters) -> Result
{
    ...
}
```

The parameter list uses the same parameter grammar as function declarations.

The result type is optional. An omitted result type means `unit`.

The lambda body is a callable-body block expression.

The lambda body creates its own callable execution scope.

`return` exits the lambda, not the enclosing callable execution scope.

```bray
func outer() -> i32
{
    let f = lambda () -> i32
    {
        return 1;
    };

    f();

    return 2;
}
```

The first `return` exits the lambda.

The second `return` exits `outer`.

A lambda expression can include callable modifiers before `lambda`.

```bray
async lambda () -> Response
{
    return await read_response();
}

trusted lambda (pos bytes: &mut [u8])
    uses(raw_memory)
{
    ...
}
```

An ABI-qualified lambda expression writes the ABI directive before `lambda`.

```bray
let callback: @abi(c) func(pos value: i32) -> i32 =
    @abi(c) lambda (pos value: i32) -> i32
    {
        return value;
    };
```

Receiver-mode modifiers such as `mut` and `consume` do not apply to `lambda`.

Lambdas have no receiver.

Inside a method body, `self` from the enclosing method is not available inside a lambda body.

Lambdas do not capture enclosing local bindings.

A lambda body can reference:

- lambda parameters,
- local bindings introduced inside the lambda body,
- declarations visible from the declaration context.

A lambda body cannot reference:

- local bindings from enclosing block or callable scopes,
- `self` from an enclosing method or lifecycle body,
- scoped capabilities available only through an enclosing local binding.

State used by a lambda must be passed explicitly through parameters or represented by a named type with methods.

```bray
let clear_buffer = lambda (pos target: &mut Buffer)
{
    target.clear();
};
```

Evaluating a lambda expression creates the callable value without copying, moving, or borrowing enclosing local state.

The lambda body is not evaluated when the lambda expression is evaluated.

The lambda body is evaluated when the callable value is called.

The callable value produced by a lambda has the callable type described by its parameters, result type, execution mode, contract
clauses, and trusted obligations.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [With expressions](with-expressions.md)
- Next: [Expression effects and capabilities](expression-effects-and-capabilities.md)

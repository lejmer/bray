# Lambda expressions and anonymous callables

An anonymous callable expression uses `lambda`.

```bray
let increment = lambda (pos value: i32) -> i32
{
    return value + 1;
};
```

A lambda has the same callable shape as a function declaration, but has no binding name.

```bray
lambda (parameters) -> Result
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

Parameters use the same parameter grammar as functions.

The result type is optional. An omitted result type means `unit`.

A lambda body is a callable-body block and creates its own callable execution scope.

`return` exits the lambda's callable execution scope.

Lambdas have no receiver.

Inside a method body, `self` from the enclosing method is not available inside a lambda body.

Receiver-mode modifiers such as `mut` and `consume` do not apply to `lambda`.

Trusted, asynchronous, and contract clauses compose with lambda syntax according to their ordinary callable rules.

```bray
async lambda (pos request: Request) -> Response
{
    return await handle(request);
}

trusted lambda (pos bytes: &mut [u8])
    uses(raw_memory)
{
    ...
}
```

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

Method paths do not implicitly produce bound-method values.

```bray
let f = buffer.clear; // invalid
```

Use a lambda with an explicit receiver parameter instead.

```bray
let clear_buffer = lambda (pos target: &mut Buffer)
{
    target.clear();
};
```

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Local callable values](local-callable-values.md)
- Next: [Methods](methods.md)

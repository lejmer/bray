# Function declarations

A function is declared with `func`.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

The function name follows `func`.

Parameters are declared inside parentheses.

An ellipsis after the fixed parameters declares a variadic foreign callable contract. Variadic syntax is restricted to
extern trusted functions and ABI-qualified callable types whose selected target ABI supports it.

The result type follows `->`.

The function body is a block expression in callable-body context.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Overview](overview.md)
- Next: [Callable ABI and FFI](callable-abi-and-ffi.md)

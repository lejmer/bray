# Methods

A method is a callable associated with a type or behavioral contract.

Method calls use `.`:

```bray
value.method(parameter = argument);
```

A method has a function-like callable contract.

The receiver is supplied by method-call syntax.

The receiver is not written as an ordinary parameter.

Inside a method body, `self` is the compiler-introduced receiver binding.

`self` cannot be declared as an ordinary parameter, local binding, or pattern binding.

The type name `Self` can still be used in ordinary parameter, result, local binding, and field types wherever `Self` is
in scope.

```bray
func distance_to(pos other: &Self) -> r64
{
    ...
}
```

Method declaration syntax is determined by receiver mode.

```bray
func length() -> usize;

mut func clear();

consume func into_bytes() -> Bytes;

consume mut func normalize() -> Self;
```

`func` declares a method with a shared receiver.

Inside a shared receiver method, `self` is an observable receiver access path.

`mut func` declares a method with a mutable receiver.

Inside a mutable receiver method, `self` is an exclusive mutable receiver access path.

`consume func` declares a method that consumes the receiver.

Inside a consuming receiver method, `self` is an owned receiver value.

`consume mut func` declares a method that consumes the receiver and gives the method body mutable local authority over
`self`.

Receiver mode is part of the method's callable contract.

Receiver mode participates in method call checking and method overload selection.

Const, trusted, asynchronous, generic, and contract clauses compose with receiver-mode syntax according to their
ordinary declaration rules.

```bray
trusted mut func reserve(pos count: usize)
    uses(manual_alloc);
```

A static function has no receiver.

`self` is unavailable inside a static function body.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Lambda expressions and anonymous callables](lambda-expressions-and-anonymous-callables.md)
- Next: [Summary](summary.md)

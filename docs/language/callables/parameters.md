# Parameters

A parameter is a binding declared by a function signature.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

Here `left` and `right` are parameters.

A parameter has:

1. a name,
2. a type,
3. an ownership or borrowing mode,
4. a mutation capability,
5. a call-position permission,
6. a lifetime and capability contract.

The parameter binding exists inside the function body.

## Positional parameters

Parameters are named by default.

A parameter can use the `pos` modifier to permit positional arguments at call sites.

```bray
func print(pos message: string)
{
    ...
}

func fit(pos data: DataFrame, max_iterations: i32)
{
    ...
}
```

The `pos` modifier changes the call surface only.

The parameter still has a name, and the function body uses that name.

A `pos` parameter can be supplied positionally or by name.

```bray
print("hello");
fit(data_frame, max_iterations = 10);
fit(data = data_frame, max_iterations = 10);
```

A parameter without `pos` must be supplied by name unless it is omitted through a default.

```bray
fit(data_frame, 10); // invalid
```

A `pos` parameter may only appear before any non-`pos` parameter in the same parameter list.

```bray
func valid(pos first: A, pos second: B, third: C);

func invalid(first: A, pos second: B); // invalid
```

Parameter modifiers apply independently to the parameter.

## Owned parameters

A parameter of plain type receives an owned value.

```bray
func consume(pos buffer: Buffer)
{
    ...
}
```

The argument is moved into the function unless the argument type is copyable or another explicit passing rule applies.

Inside the function, the parameter binding owns the value.

## Immutable owned parameters

Owned parameters are immutable by default.

```bray
func inspect(pos buffer: Buffer)
{
    ...
}
```

The function owns `buffer`, and the local binding has immutable access authority.

Ownership and mutation authority are separate capabilities.

## Mutable owned parameters

A mutable owned parameter uses `mut` before the parameter name.

```bray
func normalize(pos mut buffer: Buffer) -> Buffer
{
    ...
    return buffer;
}
```

This means the function receives ownership of the argument and the local parameter binding has mutation authority over that owned
value, subject to the type's field and representation rules.

The `mut` before the parameter name applies to the local owned binding.

The caller's binding keeps its own declared capability rules.

The grammar accepts parameter modifiers before the parameter name.

## Shared borrow parameters

A shared borrow parameter uses `&T`.

```bray
func read(pos buffer: &Buffer)
{
    ...
}
```

The borrow type forms section of the Types chapter defines shared-borrow observation, aliasing, lifetime, and capability rules.

## Mutable borrow parameters

A mutable borrow parameter uses `&mut T`.

```bray
func fill(pos buffer: &mut Buffer)
{
    ...
}
```

The borrow type forms section of the Types chapter defines mutable-borrow exclusivity, lifetime, and capability rules.

## `mut` placement in parameters

`mut` before a parameter name marks the local owned parameter binding as mutable.

```bray
func normalize(pos mut buffer: Buffer) -> Buffer
{
    ...
}
```

`mut` after `&` marks mutation authority over the storage reached by that borrow layer.

```bray
func fill(pos buffer: &mut Buffer)
{
    ...
}
```

These are different syntactic positions with different meanings.

The grammar accepts parameter modifiers before the parameter name.

The grammar represents borrowed parameter capability through the borrow type itself:

```bray
func read(pos buffer: &Buffer)
{
    ...
}

func fill(pos buffer: &mut Buffer)
{
    ...
}
```

## Nested borrow parameter types

Nested borrow type forms can appear in parameter types.

```bray
func f(x: &T)
{
}

func f(x: &mut T)
{
}

func f(x: &&T)
{
}

func f(x: &&mut T)
{
}

func f(x: &mut &T)
{
}

func f(x: &mut &mut T)
{
}
```

The borrow type forms section of the Types chapter defines nested borrow capability and reachable-operation rules.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Callable results and bodies](callable-results-and-bodies.md)
- Next: [Function calls](function-calls.md)

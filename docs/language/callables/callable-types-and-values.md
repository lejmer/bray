# Callable types and values

## Function types

Callable types use `func(...) -> ...`.

```bray
let op: func(left: i32, right: i32) -> i32 = add;
```

Tuple types use tuple syntax:

```bray
(T1, T2)
```

Function types use callable syntax:

```bray
func(left: T1, right: T2) -> R
```

Example:

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}

let op: func(left: i32, right: i32) -> i32 = add;
```

## Function values

A function can be used as a value when its callable contract matches the expected function type.

```bray
let op: func(left: i32, right: i32) -> i32 = add;
```

A function value carries its full callable contract.

The callable contract includes:

1. parameter names,
2. parameter call-position permissions,
3. parameter types,
4. result type,
5. execution mode,
6. callable ABI,
7. ownership behavior,
8. borrowing behavior,
9. mutation requirements,
10. lifetime requirements,
11. capability requirements,
12. caller-visible effects,
13. trusted caller obligations,
14. finalization behavior.

A function assignment succeeds when the target callable type preserves the callable contract required by the function
value.

## Higher-order functions

A function can accept another function as a parameter.

```bray
func apply(pos value: i32, op: func(pos value: i32) -> i32) -> i32
{
    return op(value);
}
```

The function parameter type describes the callable contract required by `apply`.

The body of `apply` can use the capabilities guaranteed by that callable type.

A callable with trusted caller obligations, async execution mode, mutating requirements, or additional caller-visible
effects can be passed when the parameter type includes those obligations.

## Callable contract preservation

Callable contracts are preserved through assignment, named callable contracts, wrappers, generic parameters, dynamic
dispatch, exports, and re-exports.

A callable assignment succeeds when the target callable type preserves every caller-visible obligation of the source
callable.

Body trust and `uses(...)` describe how the implementation is checked. Callable compatibility compares caller
requirements and execution guarantees independently of those implementation acknowledgments. Trusted predicate
requirements and execution-lane requirements remain part of the caller contract.

Example ordinary callable type:

```bray
let f: func(buffer: &Buffer, index: usize) -> u8 = get_checked;
```

Callable types write parameter names.

Callable types that permit positional arguments use the same `pos` parameter modifier as callable declarations.

```bray
let op: func(pos value: i32) -> i32 = double;
```

A callable with trusted caller obligations requires a callable type that carries those obligations.

A callable type uses the callable signature grammar without a body.

```bray
func(left: i32, right: i32) -> i32
func(pos value: i32) -> i32
async func(pos request: Request) -> Response
trusted func(pos bytes: &mut [u8])
    uses(raw_memory)
@abi(c) func(pos context: RawPointer<u8>, pos value: i32) -> i32
```

The parameter list uses the same parameter grammar as callable declarations.

An ellipsis after one or more fixed parameters is part of an ABI-qualified foreign callable contract. Its trailing
arguments are positional and follow the selected ABI's variadic promotion rules.

The result type can be omitted when the result is `unit`.

Callable modifiers that are visible in a callable contract are written before `func`.

Callable ABI directives that are visible in a callable contract are written immediately before `func`.

Contract clauses attach after the callable signature.

```bray
func(pos value: i32) -> i32
    requires(value >= 0)
```

Contract clauses on callable types use the same predicate-expression syntax as contract clauses on callable
declarations.

The callable type must preserve every caller-visible obligation of the callable value assigned to it.

### Named callable contracts

A named callable contract declaration gives a reusable name to a callable type form.

```bray
callable Transform =
    func(pos value: i32) -> i32;
```

A named callable contract declaration has no body and creates no callable value.

The declared name can be used in type positions that expect a callable type.

```bray
func double(pos value: i32) -> i32
{
    return value + value;
}

let transform: Transform = double;
```

A named callable contract can be generic.

```bray
callable Mapper<T, U> =
    func(pos value: T) -> U;

callable ArrayConsumer<T, const N: usize> =
    func(pos items: &[T; N]) -> unit;
```

A generic callable contract can have `with(...)` constraints.

```bray
callable OrderedPredicate<T>
    with(T: Comparable<T>) =
    func(pos left: &T, pos right: &T) -> bool;
```

The `with(...)` clause uses static predicate expressions.

The callable type on the right-hand side can reference the callable declaration's generic parameters and constraints.

A named callable contract is not a general type alias.

It can name only callable type forms.

It does not rename a type, function, method, lambda, implementation, module, or package.

It does not create a wrapper value or adapter.

A callable value satisfies a named callable contract when its visible callable contract satisfies the named contract.

Named callable contracts describe only the visible callable contract.

A named callable contract does not create wrapper state, bind a receiver, or attach hidden environment state to the
callable value that satisfies it.

Named callable contracts cannot be overloaded.

There can be at most one visible callable contract declaration for a given name in a name/coherence domain.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Return and execution scopes](return-and-execution-scopes.md)
- Next: [Function overloading](function-overloading.md)

# Generic functions

Generic functions are parameterized by type parameters, const parameters, or both.

The generic parameter list is written after the function name and before the function parameter list.

```bray
func identity<T>(pos value: T) -> T
{
    return value;
}

func element_count<T, const N: usize>(pos items: &[T; N]) -> usize
{
    return N;
}
```

Bare generic parameter names declare type parameters.

Const parameters are declared with `const NAME: Type`.

Generic function parameter lists contain type parameters and const parameters.

Capability requirements and caller-visible effects are represented by the ordinary callable surface and contract
clauses.

Generic function calls supply generic arguments explicitly.

```bray
let value = identity<i32>(10);
let count = element_count<u8, 4>(&bytes);
```

Generic arguments are not inferred from ordinary arguments, expected result type, assignment target type, return type,
or constraints.

A generic function body is checked against its declared constraints.

Generic function constraints are written with `with(...)` clauses.

`with(...)` clauses contain static predicate expressions.

Generic code can use the operations, ownership behavior, effects, capabilities, and contracts guaranteed by its
constraints.

Generic instantiation must satisfy the generic function's full callable contract.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Function overloading](function-overloading.md)
- Next: [Contract clauses on functions](contract-clauses-on-functions.md)

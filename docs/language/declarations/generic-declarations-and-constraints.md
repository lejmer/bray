# Generic declarations and constraints

A generic declaration introduces generic parameters.

Generic parameter lists are written after the declaration name.

```bray
func identity<T>(pos value: T) -> T
{
    return value;
}

struct ArrayBuffer<T, const N: usize>
{
    items: [T; N];
}

static EMPTY_BUFFER<T, const N: usize>: ArrayBuffer<T, N>
    with(T: Copyable) = ArrayBuffer<T, N>.empty();
```

Generic parameter lists can contain:

- type parameters,
- const parameters.

Bare generic parameter names declare type parameters.

Const parameters use `const NAME: Type`.

Declaration generics do not include capability parameters. Capability requirements and caller-visible effects belong
to the ordinary declaration contract and its clauses.

Generic constraints use `with(...)` clauses containing static predicate expressions. They form part of the
declaration's semantic contract, so changing them changes which uses and implementations are accepted.

Generic arguments for callable calls are explicit. They are not inferred from ordinary arguments, result type,
assignment target type, return type, or constraints.

Implementation declarations instead infer generic parameters from otherwise unresolved generic names in the implementing
subject and, for a trait implementation, the trait application. They have no explicit generic parameter list. Their
`with(...)` clauses constrain those inferred parameters.

Generic type rules are defined in [Generic types](../types/generic-types.md).

Generic function rules are defined in [Generic functions](../callables/generic-functions.md).

Generic static declarations define open instance templates. Each demanded closed substitution selects one
address-bearing instance according to [Static storage declarations](static-storage-declarations.md).

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Modifiers](modifiers.md)
- Next: [Declaration-owned expressions](declaration-owned-expressions.md)

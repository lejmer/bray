# Static constraints

A `with(...)` clause is a comma-separated list of static predicate expressions.

```bray
func first_token<I>(pos iter: I) -> Token?
    with(
        I: Iterable,
        I(Iterable).Element == Token,
    )
{
    ...
}
```

Static predicate expressions are checked in static constraint context.

They are compile-time conditions.

They do not read runtime storage, execute runtime code, allocate, mutate, move, borrow, perform I/O, dispatch
dynamically, or establish trusted runtime conditions.

Allowed in static predicate expressions:

- references to generic parameters,
- type expressions,
- trait applications,
- type-valued member references,
- compile-time constants and constant generic values,
- boolean operators,
- equality and comparison operators whose operands are valid in static constraint context,
- calls to predicates, functions, and methods that are valid in static constraint context.

A predicate, function, or method is valid in static constraint context only when its parameters, body, selected callable
contract, and result are valid in static constraint context.

Static constraint conditions become available while checking the constrained declaration body, its signature, and its
contract clauses.

They do not become runtime conditions unless a separate value predicate or contract clause establishes a runtime
condition.

A type-valued member reference in static constraint context is valid only when the exact implementation subject and
exact trait application are established by the same constraint set or by an enclosing constraint context.

This is rejected because the selected `Iterable` application is not established:

```bray
func first_token<I>(pos iter: I) -> Token?
    with(
        I(Iterable).Element == Token,
    )
{
    ...
}
```

Type equality does not select an implementation, create a trait satisfaction condition, or choose an arm from an
implementation overload family.

Result type and expected type do not infer missing static constraints.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Predicates and predicate expressions](predicates-and-predicate-expressions.md)
- Next: [Predicate-expression callable calls](predicate-expression-callable-calls.md)

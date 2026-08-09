# Generic types

A type declaration can be generic.

```bray
struct Pair<TLeft, TRight>
{
    left: TLeft;
    right: TRight;
}
```

A generic type declaration is parameterized by declared generic parameters.

The generic parameter list is written after the type name.

Bray has two generic parameter kinds:

- **type parameters:** declared by a bare generic parameter name,
- **const parameters:** declared with `const NAME: Type`.

All explicit generic parameter lists use this parameter-kind set.

```bray
struct Pair<TLeft, TRight>
{
    left: TLeft;
    right: TRight;
}

struct FixedBuffer<T, const N: usize>
{
    items: [T; N];
}
```

A type parameter denotes a type.

A const parameter denotes a compile-time value of its declared type.

Const parameters have no runtime storage, cannot be borrowed, cannot be moved, and cannot be assigned.

Const parameters can be used in type-form arguments and static predicate expressions where their declared type is valid.

Capabilities, effects, lifetimes, and storage policies are not generic parameter kinds.

A storage policy is supplied as an ordinary type parameter when a declaration needs to abstract over storage.

The body of a generic type declaration is checked against its declared parameters and constraints.

A concrete instantiation supplies arguments for the generic parameters.

```bray
Pair<i32, r64>
FixedBuffer<u8, 32>
```

Generic arguments are supplied explicitly and in parameter order.

A type parameter argument must be a type expression.

A const parameter argument must be a compile-time constant expression compatible with the const parameter's declared type.

Generic arguments are not inferred from expected type, field initializers, result type, constraints, or surrounding expression
context.

A `with(...)` clause constrains declared generic parameters.

A `with(...)` clause does not introduce generic parameters.

A generic declaration body is checked once against the operations, type relationships, const conditions, ownership behavior,
capabilities, effects, and contracts established by its declared parameters and `with(...)` constraints.

A concrete instantiation is valid only when every supplied generic argument has the required kind and every `with(...)`
constraint is satisfied.

## Generic argument matching and variance

Bray does not have source-level variance annotations.

Generic type parameters are invariant.

Generic const parameters match by exact compile-time value and declared const parameter type.

Closed const arguments use canonical typed value identity. Integer arguments compare by their exact value in the declared type.
Real and complex arguments compare by their exact selected runtime-format bits. Strings and aggregate arguments compare by exact
typed content recursively.

An open generic context can use const parameters and checked constant expressions whose concrete values depend on a later
substitution. Open const arguments retain their checked operation structure after closed subexpressions are evaluated. The language
does not assume arbitrary algebraic rewrites when deciding whether two open arguments are the same.

A static constraint can prove two otherwise distinct open const arguments equal for an operation in that constraint context. Such a
proof does not globally identify the two open type expressions outside the context. After concrete substitution, exact closed values
determine type identity.

Two concrete generic instantiations are the same type only when they use the same generic declaration and the same ordered generic
arguments. Two open instantiations have the same global canonical identity when their ordered open argument terms are canonical
matches. A contextual proof of equality does not change that global identity.

```bray
Pair<i32, string>
Pair<i32, string> // same type
Pair<string, i32> // different type
```

The compiler does not coerce one generic instantiation to another by covariance, contravariance, or structural similarity.

Callable parameter and result types do not use variance to select assignment or overload compatibility.

Callable assignment is checked by callable-contract preservation rules.

Borrow lifetimes and dependency contracts can be shortened when the compiler proves the shorter requirement is valid.

This shortening is dependency-contract checking, not generic variance.

Compiler-known type forms can define their own narrow weakening rules when required by their semantic contract.

Those weakening rules do not create general variance for user-declared generic types.

An unused generic parameter remains part of the declared type identity.

An unused type parameter does not create storage, ownership, destruction, finalization, copy, lifetime, or capability obligations by itself.

An unused const parameter does not create runtime storage by itself.

Unused generic parameters can still appear in static constraints, associated behavior, implementation selection, and public API identity.

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Type declarations](type-declarations.md)
- Next: [Copy contracts](copy-contracts.md)

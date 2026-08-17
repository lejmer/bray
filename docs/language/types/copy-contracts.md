# Copy contracts

A **copy contract** is a type contract that permits copying a value of that type.

Copying creates a separate value with its own ownership story.

Copying preserves the copied value's abstract value.

Copying does not move from the source access path.

After a successful copy, the source access path remains initialized and usable according to its previous ownership and capability
state.

A copy operation cannot fail.

A copy operation cannot call user-defined code, perform I/O, allocate source-visible resources, access raw memory, or require trusted
caller obligations.

Any operation that can fail, allocate source-visible resources, perform domain-specific duplication, preserve identity, duplicate
external resources, or run user-defined behavior is an explicit named operation such as `clone`, `duplicate`, or a domain-specific
function.

It is not copy behavior.

Copyability participates in static constraints through the compiler-known contract `Copyable`.

`T: Copyable` is true exactly when `T` has a copy contract in the current static context.

Source code does not manually implement `Copyable`.

A user-declared product or union type opts into compiler-derived copy behavior with the `@copy` directive:

```bray
@copy
struct Point
{
    x: r64;
    y: r64;
}
```

`@copy` attaches to primary representation declarations.

The user-declared primary representation declarations that accept `@copy` are `struct` and `union`.

`@copy` does not attach to implementation blocks, trait declarations, callable declarations, modules, aliases, or arbitrary type
expressions.

Only one `@copy` directive can apply to a primary representation declaration.

For a non-generic product or union, `@copy` is accepted only when every represented field or payload field has a copy contract and
the type has no non-copyable lifecycle or ownership obligations.

For a generic product or union, `@copy` declares a conditional derived copy contract.

A concrete instantiation of that generic type is copyable only when every represented field or payload field in that concrete
instantiation has a copy contract and the concrete instantiation has no non-copyable lifecycle or ownership obligations.

The generic type can still be instantiated with non-copyable arguments unless its own `with(...)` constraints forbid those
arguments.

That concrete instantiation is simply not copyable.

The compiler derives the copy operation by recursively copying represented fields, active payload fields, tuple elements, array
elements, nullable contained values, callable values, and dependency contracts according to each part's copy contract.

The compiler does not synthesize or call a user-defined copy body.

The following type categories have language-defined copy contracts:

- scalar types with values,
- `string`,
- `Range<T>`,
- `RawPointer<T>`,
- shared borrow values,
- nullable values whose contained type is copyable,
- tuples whose element types are all copyable,
- fixed-size arrays whose element type is copyable,
- callable values,
- product and union values whose concrete type has an accepted `@copy` contract.

The following type categories are not copyable by default:

- mutable borrow values,
- `Future<T>`,
- `Task<T>`,
- `box[S] T`,
- `box[S] view TraitApplication`,
- `box[S] [T]`,
- owned values with unresolved finalization obligations,
- values with user-declared destructors,
- values with user-declared finalizers,
- values with scope enter or scope exit lifecycle declarations,
- values that own or carry non-copyable resource obligations,
- trait-view storage or access forms whose outer type form is not copyable.

A compiler-known [protected-representation](../compiler-known-and-standard-library/protected-representation.md) type can have a language-defined copy contract only when the owning language rule defines that contract.

Its implementation representation is not source-observable.

The observable copy contract must still preserve Bray ownership, borrowing, aliasing, mutation, destruction, finalization, and
capability rules.

Copying a shared borrow copies the borrow value and preserves the same reached storage, lifetime, and capability requirements.

Copying a raw pointer copies the pointer value and does not copy, borrow, move, initialize, destroy, or otherwise affect the reached
storage.

Copying a callable value copies the callable identity and dependency contract.

Callable values do not contain hidden captured state.

Copying a nullable value in absent state copies the absent state.

Copying a nullable value in present state copies the contained value according to the contained type's copy contract.

Copying a product copies each field according to field declaration order unless the product rules define a narrower
ordering requirement.

Copying a union copies the active tag and the active payload according to the active payload's copy contract.

Inactive payloads have no initialized values to copy.

Declaring `@copy` on a type with any user-declared `destruct`, `finalize`, `enter`, or `exit` lifecycle declaration is rejected.

Declaring a `destruct`, `finalize`, `enter`, or `exit` lifecycle declaration for a type that has `@copy` is rejected.

If a type has `@copy`, adding a represented field or payload field that is not copyable for a concrete instantiation makes that
concrete instantiation not copyable.

For non-generic `@copy` types, such a field or payload makes the declaration invalid.

`@copy` does not imply stable layout, plain storage, raw-memory copyability, bitwise relocation, thread sharing, or FFI safety.

Those are separate contracts.

This is valid:

```bray
@copy
struct Pair
{
    left: i32;
    right: i32;
}
```

This is valid, but `Cell<T>` is copyable only for concrete `T` values that are copyable:

```bray
@copy
struct Cell<T>
{
    value: T;
}
```

This is rejected because the type declares a destructor:

```bray
@copy
struct FileHandle
{
    raw: RawPointer<u8>;
}

impl FileHandle
{
    destruct ()
    {
        ...
    }
}
```

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Generic types](generic-types.md)
- Next: [Lifecycle declarations](lifecycle-declarations.md)

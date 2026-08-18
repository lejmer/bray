# Scalar Types

A **scalar type** is a compiler-known named value type whose values are treated as atomic by the type system.

Scalar types are:

- fixed-width signed integer types,
- fixed-width unsigned integer types,
- machine-sized integer types,
- real floating-point types,
- complex floating-point types,
- `bool`,
- `char`,
- `unit`,
- `never`.

Scalar type names are
[compiler-known declarations](../compiler-known-and-standard-library/compiler-known-declarations.md).

User code cannot redeclare a scalar type name, add representation members to a scalar type, add variants to a scalar
type, or attach lifecycle declarations to a scalar type.

Scalar types have no user-visible fields, payload fields, tuple elements, array elements, or structural representation
members.

Scalar values have no partial-move state.

A scalar access path is initialized exactly when it contains a scalar value.

The `never` type has no values and no initialized storage state.

Every scalar type with values satisfies the copy contract.

Copying a scalar value copies the scalar value and creates no alias to mutable storage.

Moving a scalar value transfers the scalar value and creates no finalization or destruction obligation beyond ending the
old value's storage state according to ordinary ownership rules.

Destroying a scalar value ends the scalar value's storage state and runs no user code.

Scalar values can be borrowed according to ordinary borrow rules.

A mutable borrow of scalar storage grants mutation authority over the scalar value stored at that access path.

Scalar operations use the exact operand types after literal adaptation and explicit conversions have been applied.

Non-literal scalar values do not implicitly convert, promote, widen, narrow, or change numeric domain for assignment,
calls, operators, overload selection, construction, or pattern matching.

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Type categories](type-categories.md)
- Next: [Type declarations](type-declarations.md)

# Expression initialization behavior

Expressions that create a complete value must initialize every required part under
[ownership states](../ownership-and-borrowing/ownership-states.md). Operations on partial values follow
[partial moves](../ownership-and-borrowing/partial-moves.md) and
[partial values and replacement](../lifecycle/partial-values-and-replacement.md).

Struct construction initializes required fields and omitted defaulted fields.

Union variant construction initializes the active tag and active payload.

Tuple construction initializes every tuple element.

Array construction initializes every array element.

Box construction initializes indirect storage with the contained value.

Assignment initializes or re-initializes the destination.

Partial moves change the source value’s initialization state.

Reinitialization is allowed when the storage and type contract permit it.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Expression ownership](expression-ownership.md)
- Next: [Expression destruction and finalization behavior](expression-destruction-and-finalization-behavior.md)

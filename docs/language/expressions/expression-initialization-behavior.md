# Expression initialization behavior

Expressions that create values must establish valid initialization state.

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

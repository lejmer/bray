# Lifecycle declarations

Lifecycle declarations attach construction, finalization, destruction, or scoped-use behavior to a type.

The lifecycle declaration kinds are:

- `construct`,
- `finalize`,
- `destruct`,
- `enter`,
- `exit`.

Lifecycle declaration syntax, selection, ordering, construction, finalization, destruction, scoped use, trait requirements, and API compatibility are defined in [Lifecycle](../lifecycle.md).

Product lifecycle declarations are whole-product lifecycle declarations.

Union lifecycle declarations are whole-union lifecycle declarations.

Lifecycle declarations can be declared in a type body or in an inherent implementation for that type.

Trait lifecycle requirements and trait implementation lifecycle fulfillments are defined by [Lifecycle requirements in traits](../lifecycle/lifecycle-requirements-in-traits.md).

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Copy contracts](copy-contracts.md)
- Next: [Product Types](product-types.md)

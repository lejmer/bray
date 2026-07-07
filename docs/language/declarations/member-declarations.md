# Member declarations

A **member declaration** appears inside a type body, trait body, or implementation body.

Member declarations are scoped to the declaring type, trait, or implementation relationship.

Type bodies can contain:

- field declarations for product types,
- variant declarations for union types,
- callable member declarations,
- constructor declarations,
- lifecycle declarations,
- type-associated constant declarations,
- type-associated predicate declarations,
- callable overload declarations.

Product and union declaration rules are defined in [Product Types](../types/product-types.md) and [Union Types](../types/union-types.md).

Trait bodies can contain:

- callable member declarations,
- constant-valued member declarations,
- type-valued member declarations,
- predicate member declarations,
- lifecycle requirement declarations.

Trait member rules are defined in [Traits](../types/traits.md).

Implementation bodies can contain implementation members.

In an inherent implementation, members introduce declarations associated with the implementation subject.

In a trait implementation, members fulfill the implemented trait application.

Implementation member rules are defined in [Implementations](../types/implementations.md).

Trait members inherit the visibility of the trait.

Individual trait members and trait implementation fulfillments do not accept `public` or `internal` modifiers.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Module body declarations](module-body-declarations.md)
- Next: [Block-level declarations](block-level-declarations.md)

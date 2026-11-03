# Summary

This summary is informative. The detailed rules include:

- [type identity](type-identity.md).
- [type forms](type-forms.md).
- [traits](traits.md).
- [implementations](implementations.md).

Types define values and permitted operations, together with their ownership and lifecycle obligations. Every value has
a type, and every access path has a type and capability state. Types govern initialization, movement, copying,
borrowing, mutation authority, destruction, finalization, contracts, and layout.

Named types have identity, while structural forms produce types under their type-form rules. Product types define
named-field structure. Bodyless structs are structurally opaque and are incomplete unless an explicit size and
alignment provide complete opaque storage. A final `[T; ..]` field gives a `@layout(c)` product flexible trailing storage.

Union types define closed alternatives with one semantic active variant. `@layout(c, tag = none)` provides overlapping
C storage without a represented tag while retaining that semantic active variant.

Traits define explicit behavioral contracts, fulfilled through explicit implementations. A named type's direct members
and owner-provided inherent members form one type-associated surface. Trait fulfillments remain separate. Type forms
exist only where compiler-known semantics cannot be expressed by ordinary named types.

Default layout is compiler-defined. Stable and C-compatible layout require explicit contracts. Public type contracts
are part of the public API, and internal type surfaces require explicit acknowledgement outside their intended scope.
Type behavior and polymorphism are explicit, and matching structure alone does not confer behavior.

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Implementations](implementations.md)

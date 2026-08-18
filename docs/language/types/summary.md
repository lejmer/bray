# Summary

Types are semantic contracts.

Every value has a type.

Every access path has a type and a capability state.

Types govern initialization, ownership, borrowing, mutation authority, movement, copying, destruction, finalization,
layout, contracts, and valid operations.

Named types have identity.

Structural type forms produce types according to their type-form rules.

Product types define named-field structure.

A bodyless struct is structurally opaque. Without an explicit size and alignment it is incomplete. With both it provides
complete opaque storage.

A final `[T; ..]` field gives a `@layout(c)` product flexible trailing storage.

Union types define closed alternatives with one semantic active variant.

`@layout(c, tag = none)` gives a union overlapping C storage without a represented tag while retaining its semantic
active variant.

Traits define explicit behavioral contracts.

An implementing subject satisfies a trait through an explicit implementation.

A named type's direct members and owner-provided inherent implementation members form one type-associated surface.

Trait implementation fulfillments remain separate from that inherent surface.

Type forms exist only when the language needs compiler-known semantics that ordinary named types cannot express.

Default layout is compiler-defined.

Stable layout and C-compatible layout require explicit layout contracts.

Public type surfaces are API surfaces.

Internal type surfaces require explicit acknowledgement outside their intended scope.

Type behavior is explicit.

Polymorphism is explicit.

No type gains behavior by accidental structural matching.

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Implementations](implementations.md)

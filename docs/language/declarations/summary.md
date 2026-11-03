# Summary

This summary is informative. The detailed rules include:

- [declaration contexts](declaration-contexts.md).
- [declaration names, lookup, and identity](declaration-names-and-identity.md).
- [declaration order and checking](declaration-order-and-checking.md).

Declarations introduce program entities, members, module contributions, compile-time relations, and semantic
relationships. Their declaration context determines which forms are accepted. Module bodies accept module-level,
using, and export declarations. Types, traits, and implementations have their own member forms, while block expressions
accept only local bindings and constant declarations.

Declaration identity is separate from source unit origin. All source names use one ordinary namespace in each lookup
scope. Visibility is public by default where supported, and internal use requires explicit acknowledgement. Directives
attach compile-time instructions to source elements. Modifiers alter declaration contracts and checking contexts.

Generic declarations can introduce type parameters, const parameters, or both. Declaration-owned defaults, constants,
predicates, constraints, and contracts are checked with those declarations, separately from executable callable and
lifecycle bodies.

Constants name compile-time values, predicates name contract-level relations, and overload declarations make overload
families explicit. Static declarations instead introduce address-bearing product storage, with `@thread_local`
selecting exact-thread storage. Both storage forms have constant materialization, deterministic closed instance
identity, inferred dependency roots, and exactly-once cleanup ownership.

`extern static` binds provider-owned native storage and produces a provider-rooted raw pointer. A native-symbol
`static mut` exposes externally mutable storage without granting ambient Bray mutation authority.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Declaration order and checking](declaration-order-and-checking.md)

# Summary

Declarations introduce program entities, members, module contributions, compile-time relations, and semantic
relationships.

Every declaration is checked in a declaration context.

Module bodies contain module-level declarations, using declarations, and export declarations.

Type bodies, trait bodies, and implementation bodies have their own member declaration forms.

Block expressions accept only local binding declarations and constant declarations.

Declaration identity is separate from source unit origin.

All source names occupy one ordinary lookup namespace within their respective lookup scopes.

Visibility is public by default where visibility is supported.

Internal use requires explicit acknowledgement.

Directives attach compile-time instructions to source elements.

Modifiers alter declaration surfaces and checking contexts.

Generic declaration surfaces contain type parameters and const parameters.

Declaration-owned defaults, constants, predicates, constraints, and contracts are checked with their declaration
surfaces.

Executable callable and lifecycle bodies remain separate from declaration-owned expression checking.

Constant declarations introduce named compile-time values.

Static declarations introduce address-bearing product storage. The `@thread_local` directive selects exact-thread
storage. Both forms have constant materialization, deterministic closed instance identity, inferred dependency roots,
and exactly-once cleanup ownership.

`extern static` composes the static surface with provider-owned native storage and produces a provider-rooted raw
pointer. A native-symbol `static mut` exposes externally mutable storage without granting ambient Bray mutation
authority.

Predicate declarations introduce contract-level relations.

Overload declarations make overload families explicit.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Declaration order and checking](declaration-order-and-checking.md)

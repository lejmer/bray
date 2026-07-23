# Visibility and reachability

Declarations are public by default when their declaration context supports visibility.

The visibility modifiers are:

- `public`,
- `internal`.

`public` is optional because it is the default.

An internal declaration is available within its intended scope.

Use outside that scope requires explicit internal-use acknowledgement.

```bray
using internal parser.impl;

let state = internal parser.impl.create_state();
```

Internal-use acknowledgement is lexical.

It applies to the acknowledged path and does not silently grant access to unrelated internal declarations.

Acknowledgement does not propagate through re-exports.

Module visibility rules are defined in [Module visibility](../modules-and-packages/module-visibility.md).

Function visibility rules are defined in [Visibility and paths](../callables/visibility-and-paths.md).

Type visibility rules are defined in [Type declarations](../types/type-declarations.md).

Trait visibility rules are defined in [Traits](../types/traits.md#trait-visibility).

Declarations in an internal module require internal-use acknowledgement to reach from outside the module's intended scope even when the declaration itself is public.

The effective reachability of a type-associated member is capped by the owning type, the member's declaring module, and the
member's own visibility. An inherent implementation does not create another visibility or activation boundary.

A public API exposes internal declarations only through an explicit public wrapper whose public signature does not require internal access.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Declaration names, lookup, and identity](declaration-names-and-identity.md)
- Next: [Directives](directives.md)

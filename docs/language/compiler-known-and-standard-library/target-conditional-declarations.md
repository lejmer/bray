# Target-conditional declarations

A **target-conditional declaration** is a compiler-known or recognized standard-library declaration whose availability depends on target facts.

The owning language rule defines each target-conditional declaration's availability rule as a compile-time boolean expression over target facts.

Before normal source checking, the compiler evaluates availability rules for the selected target profile and forms the available compiler-known and recognized standard-library surface for that product.

Using a target-unavailable declaration is a compile-time error.

A target-unavailable declaration inside a target-disabled module contribution is not used by that product.

Availability is checked during:

- name resolution,
- type checking,
- trait and implementation checking,
- contract checking,
- layout checking,
- ABI checking,
- overload resolution,
- conversion selection,
- operator selection,
- const evaluation,
- generic instantiation.

A generic declaration that uses a target-conditional declaration must be valid for the selected target profile wherever the generic body is checked or instantiated.

A target-gated module contribution can prove target availability for declarations inside that contribution.

If a public declaration's signature, contract, layout, ABI, constant value, implementation participation, overload participation, or availability depends on target facts, compiled interface metadata records the relevant target fact dependencies.

Compiled interface metadata for a target-dependent public surface is valid only for target profiles whose recorded target facts match for the purposes of that public surface.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Target profiles and target facts](target-profiles-and-target-facts.md)
- Next: [Compiler-known surface](compiler-known-surface.md)

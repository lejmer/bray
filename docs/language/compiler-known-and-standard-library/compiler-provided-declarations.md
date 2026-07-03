# Compiler-provided declarations

A **compiler-provided declaration** is a compiler-known declaration whose implementation is supplied by the compiler instead of by ordinary Bray source.

Compiler-provided declarations have ordinary declaration surfaces.

Their declaration surface includes:

- namespace,
- name,
- generic parameters,
- parameter names,
- parameter modifiers,
- default values,
- result type,
- contracts,
- trusted obligations,
- trusted capabilities,
- effects,
- const eligibility,
- availability,
- overload-family membership.

When this specification writes a bodyless compiler-provided declaration, the bodyless form describes the declaration surface and contract.

It is not source syntax that packages can write.

There is no source-level `intrinsic` declaration modifier.

The set of compiler-provided declarations is closed by this specification.

A compiler must not add, remove, rename, overload, shadow, replace, or change a compiler-provided declaration except where an owning language rule defines an explicit target-conditional declaration.

User packages cannot declare compiler-provided declarations.

Standard-library packages cannot declare compiler-provided declarations.

Standard-library packages cannot replace compiler-provided declarations.

Compiler-provided declarations exist in the compiler-known environment before source packages, imports, module declarations, overload declarations, and implementation declarations are checked.

Their fully qualified names and declaration identities are reserved.

If source declares the same fully qualified name in the same namespace, the source declaration is rejected.

If source declares a name that is textually similar but has a different namespace or declaration identity, it is an ordinary source declaration and does not gain compiler-provided behavior.

The owning language rules define the observable semantics of each compiler-provided declaration.

Those observable semantics include normal results, panic behavior, trusted fact production, trusted fact invalidation, ownership effects, borrowing effects, initialization effects, finalization effects, destruction effects, allocation effects, aliasing effects, memory effects, and target-specific constraints.

A conforming compiler must implement the specified observable semantics for every supported target where the declaration is available.

Changing those observable semantics is a compiler bug, not an implementation choice.

A behavior is not implementation-defined merely because the declaration is compiler-provided.

Compiler-provided behavior is portable unless the owning language rule explicitly marks a specific part of that behavior as target-defined or implementation-defined and defines the allowed range.

A compiler can use target intrinsics, runtime calls, inline code generation, platform APIs, allocator hooks, metadata, static analysis, or another lowering strategy when the specified observable Bray semantics are preserved.

Lowering strategy is not observable Bray semantics.

Implementation-specific lowering must not introduce extra user-visible preconditions, postconditions, panics, trusted facts, invalidations, overloads, conversions, imports, visibility changes, or evaluation-order changes.

Implementation-specific lowering must not remove any specified precondition, postcondition, panic, trusted fact, invalidation, ownership effect, borrowing effect, initialization effect, finalization effect, destruction effect, allocation effect, aliasing effect, memory effect, or evaluation-order rule.

If a declaration's behavior depends on target properties, the owning language rules state the abstract rule and the target profile provides the concrete target facts needed by that rule.

Examples of target facts include pointer width, pointer alignment, scalar layout, address-space rules, allocation alignment support, atomic operation support, and platform ABI constraints.

An always-available compiler-provided declaration must be implemented on every target that the compiler claims to support.

A target-conditional compiler-provided declaration is part of the language surface only for targets whose target facts satisfy the declaration's availability rule.

Using a target-unavailable compiler-provided declaration is a compile-time error before code generation.

If the compiler cannot implement a required always-available compiler-provided declaration for the selected target, the compiler must reject the target as unsupported before checking user source that depends on that target.

Calls to compiler-provided declarations are checked like normal calls against the declaration surface.

Contracts, trusted obligations, trusted capabilities, effects, named and positional parameter rules, generic argument rules, overload selection rules, evaluation-order rules, panic-catching rules, result propagation rules, and ownership rules apply normally.

The compiler-provided implementation body is not Bray source.

It is not type checked as a Bray body.

It cannot be referenced, imported, reflected over, overloaded, partially applied, replaced, wrapped by name resolution, or selected by source-level implementation rules except through the declaration surface exposed by the language rules.

Compiler-provided declarations can be used by the standard library like ordinary visible compiler-known declarations.

Standard-library wrappers over compiler-provided declarations are ordinary standard-library declarations unless an owning language rule explicitly makes the wrapper compiler-known.

Compiler-specific extensions must not appear in compiler-known namespaces.

Compiler-specific extensions must not change name resolution, overload resolution, type checking, ownership checking, contract checking, trusted fact checking, or code generation for conforming Bray source.

Compiler-specific extensions are reached through ordinary package or target configuration mechanisms outside the compiler-known declaration set.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Compiler-known declarations](compiler-known-declarations.md)
- Next: [Protected representation](protected-representation.md)

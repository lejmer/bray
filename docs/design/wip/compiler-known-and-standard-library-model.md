# Compiler-known and standard library model

## Overview

Bray distinguishes compiler-known declarations from standard-library declarations.

The compiler can reason about both categories, but they enter name resolution differently.

Compiler-known declarations are available without import, subject to any target availability rule defined by the owning language
model.

Standard-library declarations are ordinary declarations supplied by standard-library packages and must be visible through normal
import and path rules before source code can use them.

This split keeps the language core small while still allowing the compiler to understand selected library contracts precisely.

---

## Compiler-known declarations

A **compiler-known declaration** is a language-defined declaration whose identity and contract are known to the compiler.

Compiler-known declarations are available in every module without an import, subject to any target availability rule defined by the
owning language model.

They participate in ordinary type checking, path resolution, overload selection, implementation coherence, ownership checking, borrowing, effect checking, contract checking, and code generation according to their language-defined contracts.

Compiler-known names occupy their normal namespace before source declarations are checked.

User code cannot declare another entity with the same name in the same namespace as a compiler-known declaration.

Compiler-known declarations are not dependencies.

They are not imported, re-exported, versioned, or shadowed by package declarations.

When a compiler-known declaration has a semantic declaration in the design documents, that declaration describes its language contract.

It does not imply that user code can replace, redeclare, or emulate that declaration by spelling the same name.

Some compiler-known declarations have protected representation.

Protected representation means user code can use the declaration according to its public contract, but cannot construct, inspect, or mutate representation details that the compiler reserves for language invariants.

---

## Compiler-provided declarations

A **compiler-provided declaration** is a compiler-known declaration whose implementation is supplied by the compiler instead of by
ordinary Bray source.

Compiler-provided declarations have ordinary declaration surfaces.

Their declaration surface includes their namespace, name, generic parameters, parameter names, parameter modifiers, default values,
result type, contracts, trusted obligations, trusted capabilities, effects, const eligibility, availability, and overload-family
membership.

A compiler-provided declaration can be written in a design document as a bodyless declaration.

A bodyless compiler-provided declaration in a design document is specification notation.

It is not source syntax that packages can write.

There is no source-level `intrinsic` declaration modifier.

The set of compiler-provided declarations is closed by the language specification.

A compiler must not add, remove, rename, overload, shadow, replace, or change a compiler-provided declaration except where the owning
language model defines an explicit target-conditional declaration.

User packages cannot declare compiler-provided declarations.

Standard-library packages cannot declare compiler-provided declarations.

Standard-library packages cannot replace compiler-provided declarations.

Compiler-provided declarations exist in the compiler-known environment before source packages, imports, module declarations, overload
declarations, and implementation declarations are checked.

Their fully qualified names and declaration identities are reserved.

If source declares the same fully qualified name in the same namespace, the source declaration is rejected.

If source declares a name that is textually similar but has a different namespace or declaration identity, it is an ordinary source
declaration and does not gain compiler-provided behavior.

The owning language model defines the observable semantics of each compiler-provided declaration.

Those observable semantics include normal results, panic behavior, trusted fact production, trusted fact invalidation, ownership
effects, borrowing effects, initialization effects, finalization effects, destruction effects, allocation effects, aliasing effects,
memory effects, and any target-specific constraints named by the owning model.

A conforming compiler must implement the specified observable semantics for every supported target where the declaration is available.

Changing those observable semantics is a compiler bug, not an implementation choice.

A behavior is not implementation-defined merely because the declaration is compiler-provided.

Compiler-provided behavior is portable unless the owning language model explicitly marks a specific part of that behavior as
target-defined or implementation-defined and defines the allowed range.

A compiler can use target intrinsics, runtime calls, inline code generation, platform APIs, allocator hooks, metadata, static
analysis, or any other lowering strategy when the specified observable Bray semantics are preserved.

Lowering strategy is not observable Bray semantics.

Implementation-specific lowering must not introduce extra user-visible preconditions, postconditions, panics, trusted facts,
invalidations, overloads, conversions, imports, visibility changes, or evaluation-order changes.

Implementation-specific lowering must not remove any specified precondition, postcondition, panic, trusted fact, invalidation,
ownership effect, borrowing effect, initialization effect, finalization effect, destruction effect, allocation effect, aliasing
effect, memory effect, or evaluation-order rule.

If a declaration's behavior depends on target properties, the owning language model must state the abstract rule and the target
configuration must provide the concrete target facts needed by that rule.

Examples of target facts include pointer width, pointer alignment, scalar layout, address-space rules, allocation alignment support,
atomic operation support, and platform ABI constraints.

A target fact used by a compile-time constant makes that constant target-dependent.

Target-dependent constants are evaluated for the selected target profile, and compiled interface metadata records the target facts
that affect the value.

A compiler must not reuse a target-dependent constant value across target profiles unless the recorded target facts are identical
for the purposes of that constant.

A compiler must not expose target-specific behavior through a compiler-provided declaration unless the owning language model defines
that behavior or target fact as part of the declaration's contract.

Compiler-specific extensions must not appear in compiler-known namespaces.

Compiler-specific extensions must not change name resolution, overload resolution, type checking, ownership checking, contract
checking, trusted fact checking, or code generation for conforming Bray source.

If a compiler exposes extensions, they must be reached through ordinary package or target configuration mechanisms that are outside
the compiler-known declaration set.

An always-available compiler-provided declaration must be implemented on every target that the compiler claims to support.

A target-conditional compiler-provided declaration is part of the language surface only for targets whose target facts satisfy the
declaration's availability rule.

Using a target-unavailable compiler-provided declaration is a compile-time error before code generation.

If the compiler cannot implement a required always-available compiler-provided declaration for the selected target, the compiler
must reject the target as unsupported before checking user source that depends on that target.

Calls to compiler-provided declarations are checked like normal calls against the declaration surface.

Contracts, trusted obligations, trusted capabilities, effects, named and positional parameter rules, generic argument rules, overload
selection rules, evaluation-order rules, panic-catching rules, result propagation rules, and ownership rules apply normally.

The compiler-provided implementation body is not Bray source.

It is not type checked as a Bray body.

It cannot be referenced, imported, reflected over, overloaded, partially applied, replaced, wrapped by name resolution, or selected by
source-level implementation rules except through the declaration surface exposed by the language model.

Compiler-provided declarations can be used by the standard library like ordinary visible compiler-known declarations.

Standard-library wrappers over compiler-provided declarations are ordinary standard-library declarations unless the owning language
model explicitly makes the wrapper compiler-known.

---

## Target profiles

A **target profile** is the language-level description of one selected compilation target for one package product.

The package/build layer supplies the selected target profile before source graph contributions are checked.

The compiler validates the target profile before checking target-gated module contributions, module bodies, declarations,
contracts, layouts, ABI surfaces, raw-memory operations, async runtime contracts, atomic operations, and constant expressions.

A target profile contains:

- target identity facts,
- pointer facts,
- scalar availability and scalar layout facts,
- endian facts,
- alignment facts,
- callable ABI facts,
- data layout ABI facts,
- atomic capability facts,
- address-space facts,
- allocation facts,
- symbol and linkage facts required by selected ABI modes.

A target profile is immutable for one product compilation.

All target facts used by a product are facts of that one selected target profile.

If a target profile is missing a required fact, contains contradictory facts, or states a fact outside the language-defined range
for that fact, the target profile is invalid and the product is rejected before source checking.

---

## Target facts

A **target fact** is a compiler-known compile-time constant fact exposed by the selected target profile.

Target facts are available under the reserved compiler-known path prefix `std.target`.

`std.target` uses ordinary path syntax, but its declarations are compiler-known target facts, not ordinary standard-library
declarations.

Target fact declarations are available without `using`.

Source packages cannot declare, import, re-export, overload, shadow, replace, or implement declarations under `std.target`.

Target facts can have scalar, string, boolean, or compiler-known target-fact enum types.

Target facts are valid in constant expressions, predicate expressions, contract expressions, static constraints, target-selection
expressions, layout checking, ABI checking, and compiler-known availability rules.

Target fact values are deterministic for the selected target profile.

The language-defined target fact groups are:

- `std.target.identity`: stable identity facts such as target name, architecture, vendor, operating system, environment, and ABI
  family,
- `std.target.pointer`: pointer size, pointer alignment, address-sized integer facts, and pointer representation facts,
- `std.target.scalar`: scalar availability and scalar layout facts for built-in scalar types,
- `std.target.endian`: the selected target's byte order,
- `std.target.alignment`: supported alignment ranges for storage, allocation, and ABI surfaces,
- `std.target.abi`: callable ABI and data layout ABI availability facts,
- `std.target.atomic`: atomic storage and atomic operation capability facts,
- `std.target.address_space`: address-space availability and pointer behavior facts,
- `std.target.allocation`: allocation size and alignment support facts,
- `std.target.linkage`: symbol encoding, linkage kind, and external artifact facts exposed by the target profile.

The target fact namespace includes at least these boolean and scalar facts:

```bray
std.target.identity.name
std.target.identity.arch
std.target.identity.vendor
std.target.identity.system
std.target.identity.environment
std.target.identity.abi
std.target.pointer.bits
std.target.pointer.bytes
std.target.endian.little
std.target.endian.big
std.target.scalar.bool
std.target.scalar.char
std.target.scalar.i8
std.target.scalar.i16
std.target.scalar.i32
std.target.scalar.i64
std.target.scalar.i128
std.target.scalar.u8
std.target.scalar.u16
std.target.scalar.u32
std.target.scalar.u64
std.target.scalar.u128
std.target.scalar.usize
std.target.scalar.isize
std.target.scalar.r16
std.target.scalar.r32
std.target.scalar.r64
std.target.scalar.r128
std.target.scalar.c32
std.target.scalar.c64
std.target.scalar.c128
std.target.scalar.c256
std.target.atomic.u8
std.target.atomic.u16
std.target.atomic.u32
std.target.atomic.u64
std.target.atomic.u128
std.target.atomic.pointer
std.target.abi.c
std.target.abi.system
std.target.address_space.host
std.target.address_space.device
std.target.alignment.max_storage
std.target.alignment.max_allocation
```

`std.target.identity.name`, `std.target.identity.arch`, `std.target.identity.vendor`, `std.target.identity.system`,
`std.target.identity.environment`, and `std.target.identity.abi` have type `string`.

`std.target.pointer.bits`, `std.target.pointer.bytes`, `std.target.alignment.max_storage`, and
`std.target.alignment.max_allocation` have type `usize`.

The other facts listed above have type `bool`.

Exactly one of `std.target.endian.little` and `std.target.endian.big` is true.

Scalar availability facts for required scalar types must be true for every conforming target profile.

Scalar availability facts for target-conditional scalar types are true only when the selected target profile supports the required
representation and operations for that scalar type.

Additional target fact declarations can be defined only by the language specification.

Compiler-specific target facts cannot appear under `std.target`.

Compiler-specific target information belongs to package/build metadata, diagnostics, or compiler-specific tooling outside the
compiler-known declaration surface.

---

## Target-conditional declarations

A **target-conditional declaration** is a compiler-known or recognized standard-library declaration whose availability depends on
target facts.

The owning language model defines each target-conditional declaration's availability rule as a compile-time boolean expression over
target facts.

Before normal source checking, the compiler evaluates availability rules for the selected target profile and forms the available
compiler-known and recognized standard-library surface for that product.

Using a target-unavailable declaration is a compile-time error.

A target-unavailable declaration inside a target-disabled module contribution is not used by that product.

Availability is checked during name resolution, type checking, trait and implementation checking, contract checking, layout
checking, ABI checking, overload resolution, conversion selection, operator selection, const evaluation, and generic instantiation.

A generic declaration that uses a target-conditional declaration must be valid for the selected target profile wherever the generic
body is checked or instantiated.

A target-gated module contribution can prove target availability for declarations inside that contribution.

If a public declaration's signature, contract, layout, ABI, constant value, implementation participation, overload participation, or
availability depends on target facts, compiled interface metadata records the relevant target fact dependencies.

Compiled interface metadata for a target-dependent public surface is valid only for target profiles whose recorded target facts
match for the purposes of that public surface.

---

## Compiler-known surface

The compiler-known surface includes:

- built-in scalar type names such as `bool`, `char`, `unit`, `never`, integer types, real types, complex types, and machine-sized integer types,
- the compiler-known text type `string`,
- the compiler-known raw pointer type `RawPointer<T>`,
- structural type forms such as tuple types, fixed-size array types, slice types, nullable types, borrow types, trait-view types, owned-indirection types, and callable types,
- compiler-known result and run-boundary types such as `Result<T, E>`, `RunResult<T>`, `PanicReport`, `ConversionError`,
  `Task<T>`, and `Thread<T>`,
- compiler-known raw memory declarations and trusted predicates under `core.memory`,
- compiler-known target facts under `std.target`,
- compiler-known type-form support traits such as `Storage<T>`,
- compiler-known iteration traits such as `Iterable` and `Iterator`,
- compiler-known conversion traits such as `ConvertTo<Target>` and `CheckedConvertTo<Target>`,
- the compiler-known `Copyable` contract used by static constraints,
- compiler-known operator traits such as `Add<Rhs>`, `Equatable<Rhs>`, `Comparable<Rhs>`, and the other operator traits defined by the Type Model,
- compiler-known literals and special values such as `true`, `false`, `unit`, and `none`.

This list is a catalog of the current design surface.

The detailed rules for each entity remain in the model that owns that feature.

Some compiler-known entities have target availability rules.

When a compiler-known entity is target-unavailable, source that uses it is rejected before code generation.

---

## Compiler-known trait implementations

Compiler-known traits can be implemented by ordinary packages when the trait contract permits it.

An implementation of a compiler-known trait is still an ordinary implementation declaration.

It participates in a coherence domain only when it is declared in that domain or explicitly imported into it according to implementation coherence rules.

A package dependency does not silently activate dependency implementations for compiler-known traits.

The compiler recognizes the trait identity and member contracts.

It does not infer implementations from names, structure, or similar-looking members.

---

## Standard-library declarations

A **standard-library declaration** is an ordinary declaration supplied by a standard-library package.

The standard-library root package is `std`.

Standard-library declarations are not automatically visible.

Source code can use a standard-library declaration only when the declaration is reachable through the `std` package root and
visible through normal import or path rules.

When a design document shows a standard-library declaration without a body, that bodyless form is specification notation for the
declaration surface and contract.

It is not source syntax that standard-library packages can write.

A conforming standard-library package must provide the declaration through ordinary Bray source, trusted Bray source, or another
declared dependency mechanism allowed by the dependency and trust models.

If a declaration has no ordinary standard-library implementation because the compiler provides it, it is a compiler-provided
compiler-known declaration, not a standard-library declaration.

The compiler may recognize selected standard-library declarations by stable declaration identity.

Recognized standard-library declarations can have compiler-defined checking, lowering, optimization, diagnostics, const eligibility,
or contract behavior.

Recognition is based on the declaration's identity, not on accidental spelling.

A user-defined declaration named like a standard-library declaration is just a user-defined declaration.

It does not gain standard-library recognition.

A standard-library declaration that wraps a compiler-provided declaration remains an ordinary standard-library declaration unless the
owning language model explicitly makes it compiler-known.

Compiler recognition of a standard-library declaration must preserve that declaration's specified contract and observable semantics.

Recognition does not let different compilers define different standard-library behavior.

Standard-library declarations do not create ambient behavior.

Importing a standard-library declaration follows the same rules as importing any other declaration.

It does not execute code, initialize modules, or silently extend overload sets, implementation overload families, operators, conversions, or behavioral contracts.

A `using std.some.path` declaration makes that qualified path available according to normal path rules.

It does not make `some`, `path`, or the final declaration name available as an unqualified name.

---

## Standard-library operations known to the compiler

The standard library can provide operations whose contracts are known to the compiler.

Current known standard-library operations include:

- `std.convert<Target>(source)`, the fallible conversion operation backed by `CheckedConvertTo<Target>`,
- numeric policy operations such as `std.round_to<Target>(source, rule = ...)`, `std.truncate_to<Target>(source)`, `std.saturate_to<Target>(source)`, and `std.wrap_to<Target>(source)`,
- string operations under the `std.string` module, including scalar-value count, emptiness, equality, scalar indexing, scalar slicing,
  UTF-8 views, and UTF-8 construction,
- raw memory helpers under `std.memory`, including raw pointer helpers, allocation owners, raw buffers, device memory helpers, and
  ABI/layout helpers, when imported,
- standard storage policy types and helpers used with compiler-known type forms, when imported.

These operations are not syntax.

They are ordinary declarations with ordinary name resolution.

If the relevant standard-library declaration is not visible, the call is rejected by ordinary name resolution.

If a visible declaration is not the recognized standard-library declaration, it is checked as an ordinary call to that declaration.

---

## Availability summary

| Entity kind                                   | Compiler can reason about it | Available without import                   | Uses ordinary import rules        |
|-----------------------------------------------|------------------------------|--------------------------------------------|-----------------------------------|
| Compiler-known scalar types                   | yes                          | yes, when target-available                 | no                                |
| `string`                                      | yes                          | yes                                        | no                                |
| `RawPointer<T>`                               | yes                          | yes                                        | no                                |
| Compiler-known type forms                     | yes                          | yes, when target-available                 | no                                |
| `Result<T, E>`, `RunResult<T>`                | yes                          | yes                                        | no                                |
| `Task<T>`, `Thread<T>`                        | yes                          | yes                                        | no                                |
| `PanicReport`, `ConversionError`              | yes                          | yes                                        | no                                |
| `core.memory` raw memory declarations         | yes                          | yes, when target-available                 | no                                |
| `std.target` target facts                     | yes                          | yes                                        | no                                |
| Compiler-known traits                         | yes                          | yes, when target-available                 | no                                |
| User implementations of compiler-known traits | yes                          | only when declared in the coherence domain | yes, for external implementations |
| Recognized standard-library functions         | yes                          | no                                         | yes                               |
| Recognized standard-library types             | yes                          | no                                         | yes                               |
| Ordinary user declarations                    | according to their contracts | only in their declaration scope            | yes                               |

---

## Finalization TODOs

- TODO: Define the complete v1 compiler-known and recognized standard-library conformance catalog, including declaration identities,
  required contracts, target availability, compiler recognition rules, and required standard-library package contents.

---

## Design principles

Compiler-known does not mean magical runtime behavior.

Compiler-known means the compiler has a language-defined contract for the declaration.

Standard-library recognition does not make a declaration ambient.

Standard-library declarations must still be made visible by source structure.

The compiler recognizes identity, not spelling.

The same name in another package does not imply the same contract.

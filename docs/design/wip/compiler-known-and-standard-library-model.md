# Compiler-known and standard library model

## Overview

Bray distinguishes compiler-known declarations from standard-library declarations.

The compiler can reason about both categories, but they enter name resolution differently.

Compiler-known declarations are always available.

Standard-library declarations are ordinary declarations supplied by standard-library packages and must be visible through normal
import and path rules before source code can use them.

This split keeps the language core small while still allowing the compiler to understand selected library contracts precisely.

---

## Compiler-known declarations

A **compiler-known declaration** is a language-defined declaration whose identity and contract are known to the compiler.

Compiler-known declarations are available in every module without an import.

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

## Always-available compiler-known entities

The always-available compiler-known surface includes:

- built-in scalar type names such as `bool`, `char`, `unit`, `never`, integer types, real types, complex types, and machine-sized integer types,
- the compiler-known text type `string`,
- the compiler-known raw pointer type `RawPointer<T>`,
- structural type forms such as tuple types, fixed-size array types, slice types, nullable types, borrow types, trait-view types, owned-indirection types, and callable types,
- compiler-known result and run-boundary types such as `Result<T, E>`, `RunResult<T>`, `PanicReport`, `ConversionError`, and `Task<T>`,
- compiler-known raw memory declarations and trusted predicates under `core.memory`,
- compiler-known type-form support traits such as `Storage<T>`,
- compiler-known conversion traits such as `ConvertTo<Target>` and `CheckedConvertTo<Target>`,
- compiler-known operator traits such as `Add<Rhs>`, `Equatable<Rhs>`, `Comparable<Rhs>`, and the other operator traits defined by the Type Model,
- compiler-known literals and special values such as `true`, `false`, `unit`, and `none`.

This list is a catalog of the current design surface.

The detailed rules for each entity remain in the model that owns that feature.

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

The compiler may recognize selected standard-library declarations by stable declaration identity.

Recognized standard-library declarations can have compiler-defined checking, lowering, optimization, diagnostics, or contract behavior.

Recognition is based on the declaration's identity, not on accidental spelling.

A user-defined declaration named like a standard-library declaration is just a user-defined declaration.

It does not gain standard-library recognition.

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
- raw pointer convenience helpers under `std.memory`, when imported,
- standard storage policy types and helpers used with compiler-known type forms, when imported.

These operations are not syntax.

They are ordinary declarations with ordinary name resolution.

If the relevant standard-library declaration is not visible, the call is rejected by ordinary name resolution.

If a visible declaration is not the recognized standard-library declaration, it is checked as an ordinary call to that declaration.

---

## Availability summary

| Entity kind | Compiler can reason about it | Available without import | Uses ordinary import rules |
|-------------|------------------------------|--------------------------|----------------------------|
| Compiler-known scalar types | yes | yes | no |
| `string` | yes | yes | no |
| `RawPointer<T>` | yes | yes | no |
| Compiler-known type forms | yes | yes | no |
| `Result<T, E>`, `RunResult<T>`, `Task<T>` | yes | yes | no |
| `PanicReport`, `ConversionError` | yes | yes | no |
| `core.memory` raw memory declarations | yes | yes | no |
| Compiler-known traits | yes | yes | no |
| User implementations of compiler-known traits | yes | only when declared in the coherence domain | yes, for external implementations |
| Recognized standard-library functions | yes | no | yes |
| Recognized standard-library types | yes | no | yes |
| Ordinary user declarations | according to their contracts | only in their declaration scope | yes |

---

## Design principles

Compiler-known does not mean magical runtime behavior.

Compiler-known means the compiler has a language-defined contract for the declaration.

Standard-library recognition does not make a declaration ambient.

Standard-library declarations must still be made visible by source structure.

The compiler recognizes identity, not spelling.

The same name in another package does not imply the same contract.

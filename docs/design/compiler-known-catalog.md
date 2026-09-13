# Compiler-known catalog

The catalog supplies compiler-known declarations, special values, compiler-provided behavior identities, and recognized
standard-library identities. It is checked compiler input, separate from user packages. Language meaning remains in the
[compiler-known specification](../language/compiler-known-and-standard-library.md).

## Ownership and representation

`bray-compiler-known` owns the private catalog parser, descriptors, stable keys, representation roles, implementation
hooks, availability rules, and generation. It reuses source, syntax, and parser facilities without depending on symbols
or later semantic phases. Those phases interpret typed metadata through their own behavior implementations.

Catalog source is a small declarative wrapper around Bray declaration fragments. The ordinary parser owns the embedded
grammar, including a narrow mode for compiler-provided bodyless declarations. Catalog metadata never becomes public
`bray-syntax` nodes or executable checking and lowering scripts.

Explicit stable keys establish identity independently of spelling, file placement, or source order. Declaration owners
are resolved across the complete source inventory. Special values keep their own descriptor family, and structural type
constructors remain semantic operations rather than fabricated named declarations.

Representation roles describe protected semantic identity, while target layout remains a compilation query.
Implementation hooks name behavior without selecting a permanent lowering strategy. Closed typed registries preserve
exhaustive handling in each consuming phase. The catalog contains no callbacks or consumer-owned executable logic.

## Generation and publication

An authoritative source manifest lists `.braydef` inputs by semantic domain. Generation validates their structure and
embedded fragments, then emits deterministic checked-in Rust tables and pre-parsed syntax data. Stable key order assigns
descriptor IDs. No partially validated catalog is published.

The production compiler shares one immutable target-independent catalog directly from those tables. It does not parse
catalog text at startup or require the source files beside the binary. Generated Rust avoids a second runtime decoder
for trusted compiler data. Typed descriptor APIs keep the generation representation out of semantic consumers.

Generated fragment reconstruction uses a separate source-identity domain. Catalog provenance supports developer
inspection and invariant reports, not user-source diagnostics. Invalid checked-in catalog data is a compiler defect.
Structural checks belong to the catalog crate, and full semantic validation uses later phase APIs during compiler
validation without reversing dependency direction.

## Symbols and target views

A dedicated provider maps descriptors to ordinary kind-specific symbols with explicit origin. It creates the
compiler-known root and typed identity skeleton without source declaration discovery IDs. Stable catalog keys remain
origin identities, while ordinary semantic APIs use compilation-local typed symbol IDs.

Forward and reverse role indexes connect selected symbols to their meaning. Multi-declaration operation contracts group
the exact trait, member, and result identities needed by semantic operations. Consumers do not build parallel string
registries or classify declarations by spelling.

Target availability is a compilation-owned immutable view over the shared catalog. The same decisions govern ordinary
lookup and role queries, including complete operation groups. Target filtering cannot mutate global descriptors or
publish only part of a required contract.

## Recognized standard-library declarations

Recognition descriptors remain distinct from ambient compiler-known descriptors. They match an ordinary declaration only
after its package and complete external identity have been validated. Explicit owner-relative identities combine with
the selected package, module, owner chain, and exact declaration category.

A recognized declaration retains ordinary visibility and member relationships. Recognition adds the language-defined
behavior associated with its identity, without creating an ambient symbol or accepting a similar signature as proof.

## Related documents

- [Symbols](symbols.md)
- [Compiled package interfaces](compiled-package-interfaces.md)
- [Catalog contributor guidance](../contributing/compiler-known-catalog.md)

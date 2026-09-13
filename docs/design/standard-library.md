# Standard library

The standard library combines ordinary Bray source with a small set of validated compiler-recognized identities and
private native services. Its APIs make ownership, allocation, execution requirements, and policy visible to callers. The
compiler consumes the library as an explicit immutable input.

## Public API design

APIs begin with caller intent. Primary constructors express ordinary construction, with named defaults for ordinary
policy. `from_*` names a source representation or ownership-changing conversion. `parse` means interpreting and
validating text.

Overloads group the same operation when explicit argument arity or types select an arm. Receiver-capability variants,
synchronous and asynchronous operations, and distinct policies retain separate names. Ownership context and expected
result type do not choose an overload for the caller.

Narrow shared contracts let other libraries supply their own allocators, containers, formatting, and I/O. Convenience
APIs build on those contracts rather than introducing parallel implementations or making entire modules ambient.

## Package identity and authority

One toolchain supplies the public `std` package and its stable `library` product. Public modules live inside that
package. Private support packages may occupy reserved `std.*` identities where a separate compilation or native boundary
is needed. They remain explicit dependencies and do not become directly visible to every user package.

Standard-library source uses the ordinary project and compiler pipeline. A dedicated host authority permits reserved
package identities. That authority does not grant source-level trust, discharge obligations, or bypass checking.
Recognition requires validated package and declaration identity, including source authority for source-built inputs.
Spelling alone cannot impersonate a recognized operation.

Compiler-known declarations remain outside `std`. Private runtime roles also have their own identities and ABI
ownership. Ordinary trusted declarations in `std.runtime` can provide internal allocation and text support retained by
lowering, while target-selected `std.platform` declarations supply the native mechanisms.

## Immutable bundles

A bundle manifest is a closed inventory of the selected interface, implementation, and target-native artifacts. It
records independent compatibility dimensions: package and product identity, interface format and semantics, target,
runtime and platform ABIs, recognized declarations, and content digests. A release label cannot substitute for any of
these checks.

`bray-standard-library` owns the manifest model, codec, content identity, and structural validation shared by producers
and consumers. Portable paths remain within the configured root. Unlisted files are not candidates, and incompatible
inputs do not trigger a search for a nearby replacement.

Bundle identity hashes the normalized semantic inventory, including artifact identities. Unicode and other generated
data record their pinned inputs and generator provenance. Host data updates cannot silently alter an existing bundle. A
producer validates and publishes a new immutable root as a unit. Compilation never repairs one in place.

## Selection and dependency direction

The host passes either no standard-library selection or one configured root. `bray-project` represents a selected
`std:library` as an explicit dependency of user products. Private support packages stay behind that dependency, and
ordinary visibility governs access to declarations.

`bray-compilation` demands the manifest, interface, and implementation or native artifacts as the requested work needs
them. Reading a declaration should not load every target archive. Each demanded artifact is validated and shared within
the immutable compilation snapshot.

Package-interface encoding belongs to `bray-package-interface`. Target properties belong to `bray-target`, runtime
semantics and ABI identities to the runtime model and ABI crates, and artifact compatibility to
`bray-runtime-interface`. The emitter publishes products. Toolchain assembly orchestrates these owners through `xtask`
without defining another manifest model or compiler pipeline.

Independent target builds can run in parallel with exact source, feature, catalog, ABI, and output selections. Stable
identity controls their merge. Installation and dependency acquisition remain host-tool responsibilities outside
compiler semantics.

## Related documents

- [Core data library](core-data-standard-library.md)
- [I/O and platform services](io-and-platform-services.md)
- [Foreign interoperability](foreign-and-platform-interoperability.md)
- [Testing](testing.md)
- [Project graphs](project-manifests.md)

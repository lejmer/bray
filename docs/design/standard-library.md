# Standard Library Packages And Toolchain Artifacts

This document defines how ordinary Bray standard-library source becomes an exact compiler input. It covers package identity,
artifact compatibility, immutable layout, and discovery without prescribing an installer, archive format, operating-system package,
or code-generation backend.

## Principles

The standard library follows the same language and package rules as other Bray libraries except where the language reserves its
identity and grants narrowly defined trusted implementation privileges.

- The public standard-library package identity is exactly `std`.
- `std` declarations remain ordinary source or imported declarations.
- Selecting the `std` package does not make its declarations ambient.
- The compiler recognizes specified declarations by validated package and declaration identity, never by spelling alone.
- A standard-library artifact is an explicit immutable compilation input.
- Tooling never downloads, updates, searches for, or version-solves a standard library.
- Installation and release assembly remain outside compiler semantics.

Compiler-known declarations are not part of the `std` package. Private runtime ABI roles are also not source declarations in
`std`. A standard-library implementation may call compiler-provided declarations or bind private runtime roles through their
owning contracts, but those mechanisms do not change package identity.

## Package Set

One selected toolchain supplies one public package with identity `std`. Public standard-library modules such as `std.io` and
`std.memory` are modules inside that package rather than independently resolved packages.

The package layer reserves `std` and the `std.*` package-identity prefix for toolchain-owned standard-library inputs. User and
vendored package manifests cannot claim either identity. The exact `std` identity is the deliberate exception to the ordinary
two-or-more-segment package-identity rule.

Private support packages may use reserved `std.*` identities when an implementation needs a separate compilation or native
boundary. They are explicit dependencies of the public `std` product, are never automatically visible to user source, and cannot
export a public identity that impersonates the `std` package.

The source and trust rules for public and private packages are defined separately so that distribution layout does not become an
authority mechanism.

## Compatibility Dimensions

There is no single standard-library version number that participates in dependency solving. Compatibility is the conjunction of
independent typed contracts:

- package identity is exactly `std`,
- the `.brayi` artifact satisfies its own format and semantic compatibility rules,
- the selected target profile satisfies the interface's target requirements,
- target-native artifacts name the exact target identity they were produced for,
- runtime-dependent artifacts require a compatible `RuntimeAbiVersion`,
- recognized declarations match the compiler-known recognition catalog by stable semantic identity,
- and every artifact byte sequence matches its declared digest.

A toolchain release may have a human-facing release version, but that value is not a semantic package version and cannot relax any
compatibility check. The standard-library bundle identity is derived from its canonical manifest and artifact digests. Equal bundle
identities therefore mean equal selected content, not merely equal labels.

No compatibility fallback selects a nearby target, older runtime ABI, differently named package, or alternate artifact after an
exact selection fails. Missing or incompatible inputs produce structured diagnostics.

## Bundle Manifest

The standard-library root contains one strict UTF-8 JSON manifest. The semantic model is Bray-owned and independent of JSON. The
manifest records:

- its serialization format revision,
- the `std` package identity,
- the package-interface artifact path, kind, byte length, and digest,
- every target artifact set in canonical target-identity order,
- each target set's exact target identity and runtime ABI requirement,
- every native or dependency artifact's kind, relative path, byte length, and digest,
- and the digest-derived identity of the complete bundle.

Unknown fields, duplicate identities, non-canonical ordering, unsafe relative paths, unrecognized artifact kinds, and digest or
length mismatches are rejected. Paths use the portable path rules from the project-manifest contract and cannot escape the
standard-library root.

The manifest is a closed inventory. An artifact present on disk but absent from the manifest is not a candidate. A manifest entry
whose file is absent is an error. Directory enumeration order never affects selection.

## Immutable Layout

The logical layout beneath a configured standard-library root is:

```text
standard-library/
|-- manifest.json
|-- interfaces/
|   `-- std.brayi
`-- targets/
    `-- <target-identity>/
        `-- <runtime-abi>/
            |-- std.brayd
            `-- <target-native artifacts>
```

The manifest is authoritative, so these names organize a bundle rather than acting as a search convention. Target-native file
names and suffixes follow `bray-target` policy. A target set may reference the same content-addressed artifact as another set, but
the manifest still records each exact compatibility selection.

Published roots are immutable. Building or updating a toolchain creates and validates a new root, then publishes that root as one
unit. Compilation never edits, repairs, or fills a root in place.

## Discovery And Selection

Compiler entry points receive either:

- no standard-library selection, for products that deliberately do not use `std`, or
- one explicit configured standard-library root.

There is no current-directory search, parent-directory search, environment fallback, registry lookup, network access, or host-wide
installation scan inside compilation. User-facing tooling may derive a root from its explicit toolchain configuration, but it must
pass the resulting selection into the compiler as typed data.

Discovery proceeds as demand-driven facts:

1. A request for a `std` declaration demands the configured bundle manifest.
2. Interface use demands and validates `std.brayi`.
3. Lowering, code generation, or linking demands the exact target artifact set when the selected declaration needs it.
4. Each demanded artifact is read and digest-validated once per immutable compilation state.

Diagnostics collection may demand these facts when a missing or incompatible standard library affects source checking. Merely
creating a compilation does not eagerly read every target artifact.

The selected interface enters the ordinary dependency symbol graph. Normal visibility and path lookup decide whether source can
name a declaration. Recognition adds language-defined behavior only after exact imported identity has been validated.

## Build Contract

Standard-library sources compile through the ordinary parser, declaration discovery, symbol construction, binder, checker,
lowering, code-generation, emission, and package-interface paths. A privileged build mode may supply reserved package identities
and trusted-package authority, but it does not bypass semantic checking or artifact validation.

One deterministic build request fixes:

- the complete source and dependency graph,
- package and product identities,
- enabled features,
- target profiles,
- runtime ABI contracts,
- compiler-known catalog input,
- and requested artifact kinds.

Independent target builds may run in parallel. Their outputs are merged only by canonical target and artifact identity. Repeating a
build with equal inputs produces byte-identical package interfaces, native artifacts, and bundle manifests.

## Ownership

`bray-project` owns portable configured roots and deterministic standard-library source/build graph selection.

`bray-package-interface` owns `.brayi` encoding, validation, compatibility, and imported semantic access.

`bray-target` and `bray-runtime-interface` own target and runtime ABI identities and compatibility facts.

`bray-compilation` owns lazy selection and validation of the exact interface and target artifacts required by a compilation.

`bray-emitter` owns artifact publication. Repository automation may assemble synthetic or release roots through `xtask`, but it
does not redefine the manifest or compatibility contracts.

## Explicit Non-Goals

This contract does not define:

- an installer or uninstaller,
- release archives,
- operating-system packages,
- system-wide search paths,
- automatic dependency acquisition,
- a package registry,
- semantic-version solving,
- standard-library updates,
- or backend-specific artifact semantics.

Those concerns may consume the immutable bundle contract later. They cannot become hidden compiler inputs.

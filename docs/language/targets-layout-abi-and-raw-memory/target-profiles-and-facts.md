# Target profiles and facts

A **target profile** is the language-level description of one selected compilation target for one package product.

The package and build layer supplies the selected target profile before source graph contributions are checked.

The compiler validates the target profile before checking:

- target-gated module contributions,
- module bodies,
- declarations,
- contracts,
- layouts,
- ABI surfaces,
- raw-memory operations,
- async runtime contracts,
- atomic operations,
- constant expressions.

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
- platform-service availability facts,
- symbol and linkage facts required by selected ABI modes.

A target profile is immutable for one product compilation.

All target facts used by a product are facts of that one selected target profile.

If a target profile is missing a required fact, contains contradictory facts, or states a fact outside the language-defined range
for that fact, the target profile is invalid and the product is rejected before source checking.

A **target fact** is a compiler-known compile-time constant fact exposed by the selected target profile.

Target facts are available under the reserved compiler-known path prefix `target`.

`target` uses ordinary path syntax, but its declarations are compiler-known target facts, not ordinary standard-library
declarations.

Target fact declarations are available without `using`.

Source packages cannot declare, import, re-export, overload, shadow, replace, or implement declarations under `target`.

Target facts can have scalar, string, boolean, or compiler-known target-fact enum types.

Target facts are valid in constant expressions, predicate expressions, contract expressions, static constraints, target-selection
expressions, layout checking, ABI checking, and compiler-known availability rules.

Target fact values are deterministic for the selected target profile.

The language-defined target fact groups are:

- `target.identity`: stable identity facts such as target name, architecture, vendor, operating system, environment, and ABI family,
- `target.pointer`: pointer size, pointer alignment, address-sized integer facts, and pointer representation facts,
- `target.scalar`: scalar availability and scalar layout facts for built-in scalar types,
- `target.endian`: the selected target's byte order,
- `target.alignment`: supported alignment ranges for storage, allocation, and ABI surfaces,
- `target.abi`: callable ABI and data layout ABI availability facts,
- `target.atomic`: atomic storage and atomic operation capability facts,
- `target.address_space`: address-space availability and pointer behavior facts,
- `target.allocation`: allocation size and alignment support facts,
- `target.platform`: process context, standard stream, filesystem, child-process, clock, and entropy availability facts,
- `target.linkage`: symbol encoding, linkage kind, and external artifact facts exposed by the target profile.

The target fact surface includes these language-defined facts:

```bray
target.identity.NAME
target.identity.ARCH
target.identity.VENDOR
target.identity.SYSTEM
target.identity.ENVIRONMENT
target.identity.ABI
target.pointer.BITS
target.pointer.BYTES
target.endian.LITTLE
target.endian.BIG
target.scalar.BOOL
target.scalar.CHAR
target.scalar.I8
target.scalar.I16
target.scalar.I32
target.scalar.I64
target.scalar.I128
target.scalar.U8
target.scalar.U16
target.scalar.U32
target.scalar.U64
target.scalar.U128
target.scalar.USIZE
target.scalar.ISIZE
target.scalar.R16
target.scalar.R32
target.scalar.R64
target.scalar.R128
target.scalar.C32
target.scalar.C64
target.scalar.C128
target.scalar.C256
target.atomic.U8
target.atomic.U16
target.atomic.U32
target.atomic.U64
target.atomic.U128
target.atomic.POINTER
target.abi.C
target.abi.SYSTEM
target.address_space.HOST
target.address_space.DEVICE
target.alignment.MAX_STORAGE
target.alignment.MAX_ALLOCATION
target.platform.process_context
target.platform.standard_streams
target.platform.filesystem
target.platform.child_processes
target.platform.monotonic_clock
target.platform.wall_clock
target.platform.entropy
```

`target.identity.NAME`, `target.identity.ARCH`, `target.identity.VENDOR`, `target.identity.SYSTEM`,
`target.identity.ENVIRONMENT`, and `target.identity.ABI` have type `string`.

`target.pointer.BITS`, `target.pointer.BYTES`, `target.alignment.MAX_STORAGE`, and `target.alignment.MAX_ALLOCATION` have type
`usize`.

The other facts listed above have type `bool`.

Exactly one of `target.endian.LITTLE` and `target.endian.BIG` is true.

Scalar availability facts for required scalar types must be true for every conforming target profile.

Scalar availability facts for target-conditional scalar types are true only when the selected target profile supports the required
representation and operations for that scalar type.

Additional target fact declarations can be defined only by this specification.

Compiler-specific target facts cannot appear under `target`.

Compiler-specific target information belongs to package and build metadata or compiler-specific tooling outside the compiler-known
declaration surface.

A target fact used by a compile-time constant makes that constant target-dependent.

Target-dependent constants are evaluated for the selected target profile, and compiled interface metadata records the target facts
that affect the value.

A compiler must not reuse a target-dependent constant value across target profiles unless the recorded target facts are identical
for the purposes of that constant.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Overview](overview.md)
- Next: [Target constraints and gates](target-constraints-and-gates.md)

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
- `target.c`: exact mappings from target C scalar types to Bray scalar representations,
- `target.atomic`: atomic storage and atomic operation capability facts,
- `target.address_space`: address-space availability and pointer behavior facts,
- `target.allocation`: allocation size and alignment support facts,
- `target.platform`: process context, standard stream, filesystem, child-process, clock, entropy, and dynamic-loader availability
  facts,
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
target.c.CHAR
target.c.SIGNED_CHAR
target.c.UNSIGNED_CHAR
target.c.SHORT
target.c.UNSIGNED_SHORT
target.c.INT
target.c.UNSIGNED_INT
target.c.LONG
target.c.UNSIGNED_LONG
target.c.LONG_LONG
target.c.UNSIGNED_LONG_LONG
target.c.SIZE
target.c.PTRDIFF
target.c.WCHAR
target.c.BOOL
target.c.FLOAT
target.c.DOUBLE
target.c.LONG_DOUBLE
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
target.platform.dynamic_loading
```

`target.identity.NAME`, `target.identity.ARCH`, `target.identity.VENDOR`, `target.identity.SYSTEM`,
`target.identity.ENVIRONMENT`, `target.identity.ABI`, and every `target.c` fact have type `string`.

`target.pointer.BITS`, `target.pointer.BYTES`, `target.alignment.MAX_STORAGE`, and `target.alignment.MAX_ALLOCATION` have type
`usize`.

The other facts listed above have type `bool`.

Each `target.c` fact is either the canonical spelling of one available Bray scalar type or `"unavailable"`. A scalar mapping
promises equal C value representation, size, alignment, and by-value classification under `target.abi.C`. Equal width alone is
insufficient. The standard library selects transparent C wrappers with exact comparisons against these facts. A public wrapper,
constant, layout, or callable signature selected that way records the consulted `target.c` fact in its compiled interface.

The native target profiles use this closed initial mapping:

| Targets | `CHAR` | `LONG` / `UNSIGNED_LONG` | `WCHAR` | `LONG_DOUBLE` |
| --- | --- | --- | --- | --- |
| `x86_64-unknown-linux-gnu` | `i8` | `i64` / `u64` | `i32` | `unavailable` |
| `aarch64-unknown-linux-gnu` | `u8` | `i64` / `u64` | `i32` | `unavailable` |
| `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc` | `i8` | `i32` / `u32` | `u16` | `r64` |
| `x86_64-apple-darwin` | `i8` | `i64` / `u64` | `i32` | `unavailable` |
| `aarch64-apple-darwin` | `i8` | `i64` / `u64` | `i32` | `r64` |

For every row, `SIGNED_CHAR = i8`, `UNSIGNED_CHAR = u8`, `SHORT = i16`, `UNSIGNED_SHORT = u16`, `INT = i32`,
`UNSIGNED_INT = u32`, `LONG_LONG = i64`, `UNSIGNED_LONG_LONG = u64`, `SIZE = usize`, `PTRDIFF = isize`, `BOOL = bool`,
`FLOAT = r32`, and `DOUBLE = r64`. A target profile cannot claim `target.abi.C` unless every non-`unavailable` mapping is
available and satisfies the equality contract above. A C type whose representation has no exact Bray scalar remains unavailable.
the compiler and standard library do not approximate it with an equal-size byte product.

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

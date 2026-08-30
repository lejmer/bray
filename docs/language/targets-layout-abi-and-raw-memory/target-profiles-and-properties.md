# Target profiles and properties

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

- target identity properties,
- pointer properties,
- scalar availability and scalar layout properties,
- byte order,
- alignment properties,
- callable ABI properties,
- data layout ABI properties,
- atomic capabilities,
- address-space properties,
- allocation properties,
- platform-service availability,
- symbol and linkage properties required by selected ABI modes.

A target profile is immutable for one product compilation.

All target properties used by a product come from that one selected target profile.

The complete target-profile identity participates in every realized static instance identity. A compiler cannot reuse
one static storage instance across distinct target profiles, even when the currently observed size and initializer bytes
happen to match.

Every conforming target provides product-static storage. `@thread_local static` is available exactly when
`target.platform.native_threads` is true. Such a profile provides target thread-local address identity, per-attachment
materialization, and exact-thread cleanup support sufficient to satisfy the language lifecycle contract.

If a target profile is missing a required property, contains contradictory property values, or states a value outside
the language-defined range for that property, the target profile is invalid and the product is rejected before source
checking.

A **target property** is a compiler-known compile-time constant exposed by the selected target profile.

Target properties are available under the reserved compiler-known path prefix `target`.

`target` uses ordinary path syntax, but its declarations are compiler-known target properties, not ordinary
standard-library declarations.

Target property declarations are available without `using`.

Source packages cannot declare, import, re-export, overload, shadow, replace, or implement declarations under `target`.

Target properties can have scalar, string, boolean, or language-defined target enum types.

Target properties are valid in constant expressions, predicate expressions, contract expressions, static constraints,
target-selection expressions, layout checking, ABI checking, and compiler-known availability rules.

Target property values are deterministic for the selected target profile.

The language-defined target property groups are:

- `target.identity`: stable identity properties such as target name, architecture, vendor, operating system,
  environment, and ABI family,
- `target.pointer`: pointer size, pointer alignment, address-sized integer properties, and pointer representation
  properties,
- `target.scalar`: scalar availability and scalar layout properties for built-in scalar types,
- `target.endian`: the selected target's byte order,
- `target.alignment`: supported alignment ranges for storage, allocation, and ABI surfaces,
- `target.abi`: callable ABI and data layout ABI availability,
- `target.c`: exact mappings from target C scalar types to Bray scalar representations,
- `target.atomic`: atomic storage and atomic operation capabilities,
- `target.address_space`: address-space availability and pointer behavior properties,
- `target.allocation`: allocation size and alignment support,
- `target.platform`: process context, standard stream, filesystem, native-thread, child-process, clock, entropy, and
  dynamic-loader availability,
- `target.linkage`: symbol encoding, linkage kind, and external artifact properties exposed by the target profile.

The target property surface consists of these language-defined constants:

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
target.atomic.U8_ALIGNMENT
target.atomic.U8_ALWAYS_LOCK_FREE
target.atomic.U8_WAIT_NOTIFY
target.atomic.U8_CROSS_PROCESS
target.atomic.U16_ALIGNMENT
target.atomic.U16_ALWAYS_LOCK_FREE
target.atomic.U16_WAIT_NOTIFY
target.atomic.U16_CROSS_PROCESS
target.atomic.U32_ALIGNMENT
target.atomic.U32_ALWAYS_LOCK_FREE
target.atomic.U32_WAIT_NOTIFY
target.atomic.U32_CROSS_PROCESS
target.atomic.U64_ALIGNMENT
target.atomic.U64_ALWAYS_LOCK_FREE
target.atomic.U64_WAIT_NOTIFY
target.atomic.U64_CROSS_PROCESS
target.atomic.U128_ALIGNMENT
target.atomic.U128_ALWAYS_LOCK_FREE
target.atomic.U128_WAIT_NOTIFY
target.atomic.U128_CROSS_PROCESS
target.atomic.POINTER_ALIGNMENT
target.atomic.POINTER_ALWAYS_LOCK_FREE
target.atomic.POINTER_WAIT_NOTIFY
target.atomic.POINTER_CROSS_PROCESS
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
target.platform.native_threads
target.platform.child_processes
target.platform.monotonic_clock
target.platform.wall_clock
target.platform.entropy
target.platform.dynamic_loading
```

Each `target.atomic.<REPRESENTATION>` Boolean reports whether any atomic operation exists for that representation. Its
`_ALIGNMENT` companion is the required protected-storage alignment in bytes. `_ALWAYS_LOCK_FREE` is true only when every
exposed operation for that representation is implemented without a lock. `_WAIT_NOTIFY` reports wait and notification
availability. `_CROSS_PROCESS` reports whether the same operations may address storage shared between processes. These
properties are independent, so availability never implies native lock freedom or wait support.

`target.identity.NAME`, `target.identity.ARCH`, `target.identity.VENDOR`, `target.identity.SYSTEM`,
`target.identity.ENVIRONMENT`, `target.identity.ABI`, and every `target.c` property have type `string`.

`target.pointer.BITS`, `target.pointer.BYTES`, `target.alignment.MAX_STORAGE`, and `target.alignment.MAX_ALLOCATION`
have type `usize`.

The other properties listed above have type `bool`.

Each `target.c` property is either the defined spelling of one available Bray scalar type or `"unavailable"`. A scalar
mapping promises equal C value representation, size, alignment, and by-value classification under `target.abi.C`. Equal
width alone is insufficient. The standard library selects transparent C wrappers with exact comparisons against these
property values. Each property controls its corresponding wrapper independently: an unavailable mapping omits only
that wrapper and cannot suppress or imply a neighboring C wrapper. A public wrapper, constant, layout, or callable
signature selected that way records the consulted `target.c` property in its compiled interface.

The C scalar mappings for the native target profiles are:

| Targets                                             | `CHAR` | `LONG` / `UNSIGNED_LONG` | `WCHAR` | `LONG_DOUBLE` |
|-----------------------------------------------------|--------|--------------------------|---------|---------------|
| `x86_64-unknown-linux-gnu`                          | `i8`   | `i64` / `u64`            | `i32`   | `unavailable` |
| `aarch64-unknown-linux-gnu`                         | `u8`   | `i64` / `u64`            | `i32`   | `unavailable` |
| `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc` | `i8`   | `i32` / `u32`            | `u16`   | `r64`         |
| `x86_64-apple-darwin`                               | `i8`   | `i64` / `u64`            | `i32`   | `unavailable` |
| `aarch64-apple-darwin`                              | `i8`   | `i64` / `u64`            | `i32`   | `r64`         |

For every row, `SIGNED_CHAR = i8`, `UNSIGNED_CHAR = u8`, `SHORT = i16`, `UNSIGNED_SHORT = u16`, `INT = i32`,
`UNSIGNED_INT = u32`, `LONG_LONG = i64`, `UNSIGNED_LONG_LONG = u64`, `SIZE = usize`, `PTRDIFF = isize`, `BOOL = bool`,
`FLOAT = r32`, and `DOUBLE = r64`. A target profile cannot claim `target.abi.C` unless every non-`unavailable` mapping
is available and satisfies the equality contract above. A C type whose representation has no exact Bray scalar remains
unavailable. The compiler and standard library do not approximate it with an equal-size byte product.

The varying table columns and the fixed mappings above assign every `target.c` property. A native profile has no
additional implicit C scalar mapping, and a conforming compiler cannot substitute a host-derived mapping for the
selected target profile.

Every native target profile listed above has `target.platform.native_threads = true`.

The `x86_64-apple-darwin` and `aarch64-apple-darwin` native profiles require macOS 14.4 or later. The target contract
records that deployment floor in linked products.

Exactly one of `target.endian.LITTLE` and `target.endian.BIG` is true.

Scalar availability properties for required scalar types must be true for every conforming target profile.

Scalar availability properties for target-conditional scalar types are true only when the selected target profile
supports the required representation and operations for that scalar type.

Additional target property declarations can be defined only by this specification.

Compiler-specific target properties cannot appear under `target`.

Compiler-specific target information belongs to package and build metadata or compiler-specific tooling outside the
compiler-known declaration surface.

A target property used by a compile-time constant makes that constant target-dependent.

Target-dependent constants are evaluated for the selected target profile, and compiled interface metadata records the
target properties that affect the value.

A compiler must not reuse a target-dependent constant value across target profiles unless the recorded target properties
are identical for the purposes of that constant.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Overview](overview.md)
- Next: [Target constraints and gates](target-constraints-and-gates.md)

# Core Data Standard Library

This document defines the package structure and contracts for Bray's ordinary core data library. It covers text, bytes,
collections, iteration utilities, formatting, hashing, ordering, and numeric utilities without turning those library APIs into
language primitives.

## Principles

The core data library follows these rules:

- Public APIs are ordinary declarations in the `std` package.
- Compiler-known types and traits are reused rather than shadowed by library equivalents.
- Compiler recognition is limited to the closed identities named by the language specification.
- Owning values make allocation and resource ownership explicit.
- Borrowed views preserve their source dependency and cannot outlive it.
- Operations do not allocate, copy, consume, or reorder values unless their contracts say so.
- Target-independent APIs have target-independent observable behavior.
- Formatting and iteration stream values instead of requiring intermediate collections or strings.
- Errors are typed values. Absence and failure are not represented by sentinels.

The public surface must remain usable by packages that provide their own allocators, containers, formatters, or I/O layers. Core
data abstractions therefore depend on language contracts and narrow standard-library interfaces rather than on a particular host
runtime or operating system.

## Module Layout

The public modules are:

| Module | Responsibility |
|---|---|
| `std.string` | UTF-8 validation, scalar and byte views, text search, comparison, and conversion |
| `std.character` | Unicode scalar conversion, classification, and encoding utilities |
| `std.bytes` | Borrowed byte views, owned byte buffers, copying, comparison, and encoding support |
| `std.iteration` | Iterator adapters and algorithms over the compiler-known iteration traits |
| `std.collection` | General-purpose sequences, maps, sets, queues, and their views |
| `std.format` | Typed formatting, format arguments, formatters, and text or byte sinks |
| `std.hash` | Hashing contracts, hash state, and standard hash implementations |
| `std.order` | Ordering helpers and algorithms over compiler-known comparison contracts |
| `std.numeric` | Numeric limits, checked arithmetic helpers, parsing, and explicit numeric policies |
| `std.memory` | The separately specified low-level memory and allocation surface |

Submodules may group focused families without changing these ownership boundaries. A module must not re-export another module's
complete surface merely to shorten paths. Cross-module convenience functions belong with the abstraction whose contract they
implement.

The `std` package root may expose a deliberately small set of universal operations such as recognized conversion functions. It
must not make the complete core data surface ambient or duplicate every declaration from the modules above.

## Language-Owned Identities

The core data library consumes the following compiler-known declarations directly:

- `string` and `char`,
- scalar numeric types,
- fixed arrays, slices, tuples, borrows, and nullable forms,
- `Result<T, E>` and `Ordering`,
- `Iterable` and `Iterator`,
- `Equatable<Rhs>` and `Comparable<Rhs>`,
- numeric operator traits,
- `Copyable`,
- and the raw-memory declarations required by `std.memory`.

These declarations retain their compiler-known identities and language-defined contracts. The standard library does not declare
replacement `String`, `Character`, `Result`, `Ordering`, `Iterable`, or operator-trait types.

The recognized `std.string` and `std.memory` declarations are exactly those in the language conformance catalog. Their stable
package and declaration identities are part of compiler recognition. Other core data declarations are ordinary declarations even
when the compiler optimizes their bodies.

Collection types, formatting traits, hashing traits, parsing errors, and iterator adapters are not compiler-known merely because
they are widely used. Promoting any such declaration to a recognized identity requires an owning language-specification change and
an update to the conformance catalog.

## Text

`string` remains the immutable compiler-known UTF-8 text value. Its representation is protected and its language-defined copy
contract preserves its abstract sequence of Unicode scalar values.

`std.string` provides:

- scalar count, emptiness, equality, scalar indexing, and scalar slicing through the recognized operations,
- exact UTF-8 byte observation,
- validated construction from UTF-8 bytes,
- scalar and byte iteration,
- searching, prefix, suffix, splitting, trimming, and comparison utilities,
- and explicit conversions between text, characters, bytes, and parsed values.

Text APIs distinguish byte offsets, Unicode scalar indexes, and collection positions with typed values or unambiguous parameter
contracts. A byte offset is never silently interpreted as a scalar index. Operations that can encounter malformed external bytes
return a typed error. Operations over an existing `string` may rely on its valid UTF-8 invariant.

Borrowed text views carry a dependency on their source text or source byte storage. An API returning such a view must express that
dependency in its callable contract. An owning text transformation returns a new `string` and may allocate. An observing operation
accepts a shared borrow unless ownership transfer is intrinsic to the operation.

Text comparison is defined over Unicode scalar values unless an API explicitly states bytewise behavior. Locale-sensitive
collation, normalization, grapheme segmentation, and case conversion are separate policy-bearing facilities and must not be hidden
inside basic equality, ordering, indexing, or slicing.

### Initial Text Surface

The initial text surface uses `&string` as its borrowed text view and `&[u8]` as its borrowed UTF-8 byte view. It does not introduce
`StringView`, `TextView`, `ByteView`, or index-wrapper types that add no invariant beyond those structural forms. Scalar positions
and UTF-8 byte offsets use `usize` and remain distinguished by the operation that accepts them.

The recognized declarations have these exact signatures:

```bray
module std.string;

union Utf8Error
{
    InvalidEncoding;
}

func scalar_count(pos value: &string) -> usize;

func is_empty(pos value: &string) -> bool;

func equals(pos left: &string, pos right: &string) -> bool;

func scalar_at(pos value: &string, index: usize) -> char?;

func scalar_slice(pos value: &string, start: usize, end: usize) -> string;

func utf8(pos value: &string) -> &[u8];

func from_utf8(pos bytes: &[u8]) -> Result<string, Utf8Error>;
```

`scalar_at` returns `none` when `index` is outside the scalar sequence. `scalar_slice` uses the half-open scalar range
`[start, end)`, allocates an owning result, and panics when `start > end` or either bound exceeds `scalar_count(value)`. `utf8`
returns a view dependent on `value`. `from_utf8` returns `Utf8Error.InvalidEncoding` for malformed UTF-8 and otherwise constructs an
owning `string` whose scalar sequence is the decoded input.

Scalar iteration uses one public cursor identity and an ordinary named implementation:

```bray
module std.string;

struct ScalarCursor;

func scalars(pos value: &string) -> ScalarCursor;

impl ScalarCursorIterator = ScalarCursor(Iterator)
{
    type Element = char;

    mut func next() -> char?;
}
```

The cursor representation is private. The value returned by `scalars` carries the dependency of `value`, advances in Unicode
scalar order, and remains exhausted after returning `none`. Byte iteration uses the slice returned by `utf8` and the ordinary
slice iteration contract rather than a second string-specific byte cursor.

The initial `std.character` surface is:

```bray
module std.character;

func scalar_value(value: char) -> u32;

func from_scalar_value(value: u32) -> char?;

func utf8_length(value: char) -> usize;

func is_alphabetic(value: char) -> bool;

func is_numeric(value: char) -> bool;

func is_whitespace(value: char) -> bool;
```

`from_scalar_value` returns `none` for values that are not Unicode scalar values. Classification follows the Unicode data version
selected by the standard-library artifact and is independent of the host locale. Case conversion and normalization remain
separate policy-bearing additions because one input scalar can produce multiple output scalars.

## Bytes And Buffers

Borrowed byte data uses slice and borrow forms over `u8`. `std.bytes` may provide named views when they add a real contract, but a
named wrapper must not exist solely to rename `&[u8]`.

An owned byte buffer:

- owns its allocation and initialized bytes,
- is movable but not copyable,
- exposes shared and mutable borrowed slices without transferring allocation ownership,
- tracks length separately from capacity,
- preserves initialized-prefix and bounds invariants,
- destroys its initialized values before releasing storage,
- and uses explicit reserve, resize, truncate, append, and extraction operations.

Growing a buffer may replace its allocation. Existing views prevent growth or mutation whenever ordinary borrowing rules make the
operation incompatible. Capacity is not part of byte-sequence equality or ordering.

Safe byte operations are implemented over the `std.memory` contracts. Trusted code may bridge between raw allocation facts and the
safe buffer invariant, but callers of the safe surface do not inherit raw-memory obligations.

Encoding and decoding APIs name their encoding and failure policy. UTF-8 conversion uses the recognized `std.string` operations.
No byte API silently assumes host endianness, native integer width, or null termination.

### Initial Buffer Surface

The initial owning byte-buffer identity is `std.bytes.Buffer`. Its representation is private and contains one
`std.memory.RawBuffer<u8>` whose initialized prefix is the buffer's byte sequence. The type is movable and not copyable.

The public surface is:

```bray
module std.bytes;

struct Buffer;

func create(capacity: usize = 0) -> Result<Buffer, std.memory.MemoryLayoutError>;

func from_slice(pos bytes: &[u8]) -> Result<Buffer, std.memory.MemoryLayoutError>;

func length(pos buffer: &Buffer) -> usize;

func capacity(pos buffer: &Buffer) -> usize;

func as_slice(pos buffer: &Buffer) -> &[u8];

func as_slice_mut(pos buffer: &mut Buffer) -> &mut [u8];

func reserve(
    pos buffer: &mut Buffer,
    additional: usize,
) -> Result<unit, std.memory.MemoryLayoutError>;

func resize(
    pos buffer: &mut Buffer,
    new_length: usize,
    fill: u8 = 0,
) -> Result<unit, std.memory.MemoryLayoutError>;

func truncate(pos buffer: &mut Buffer, new_length: usize) -> unit;

func clear(pos buffer: &mut Buffer) -> unit;

func push(
    pos buffer: &mut Buffer,
    value: u8,
) -> Result<unit, std.memory.MemoryLayoutError>;

func append(
    pos buffer: &mut Buffer,
    bytes: &[u8],
) -> Result<unit, std.memory.MemoryLayoutError>;

func pop(pos buffer: &mut Buffer) -> u8?;
```

`create` and `from_slice` return `MemoryLayoutError` when the requested capacity cannot be represented. Allocation failure follows
the language allocation panic contract. `reserve` guarantees capacity for `length(buffer) + additional` without changing the byte
sequence. `resize` preserves the existing prefix, truncates when shrinking, and appends `fill` bytes when growing. `truncate`
leaves the buffer unchanged when `new_length >= length(buffer)`. `pop` returns `none` for an empty buffer.

`as_slice` and `as_slice_mut` return views dependent on `buffer`. Any operation requiring mutation or possible reallocation is
rejected while an incompatible view remains live by ordinary borrowing rules. A caller therefore cannot pass a view reaching
`buffer` as the `bytes` argument of `append` while also supplying the required mutable borrow of that buffer.

Private standard-library support may transfer a `Buffer` to or from its `RawBuffer<u8>` representation. That bridge is not part of
the public `std.bytes` surface and does not expose raw allocation facts to ordinary callers.

## Iteration

The compiler-known `Iterable` and `Iterator` traits define source-to-cursor and cursor-advance semantics. `std.iteration` provides
ordinary adapters and algorithms over those traits.

Adapters preserve the operation mode of their source:

- iteration over a shared borrow yields shared access according to the selected implementation,
- iteration over a mutable borrow yields mutation authority only where the implementation contract permits it,
- and consuming iteration may move elements from an owning source.

An adapter that stores a cursor or callable owns those values. An adapter over borrowed storage carries the corresponding
dependency. Lazy adapters do not evaluate source elements until iteration advances them. Terminal algorithms document whether
they short-circuit and whether they preserve source order.

Algorithms requiring multiple passes, exact size, stable ordering, random access, or contiguous storage use explicit additional
contracts. They do not infer those properties from `Iterable` alone.

## Collections

`std.collection` provides focused owning collection types rather than one universal container abstraction. The initial families
include:

- growable contiguous sequences,
- double-ended queues,
- hash maps and hash sets,
- ordered maps and ordered sets,
- and narrow stack or queue adapters when they add a useful contract.

Owning collections are movable and are not copyable because copying them can allocate and run element behavior. Explicit clone or
duplicate operations state their element requirements and failure behavior. Shared and mutable collection views are borrows whose
dependencies follow ordinary Bray rules.

Insertion transfers or constructs ownership according to the operation signature. Removal returns ownership of removed values
when applicable. Reallocation moves elements according to their movement and lifecycle contracts; it does not bitwise-relocate
values unless a recognized low-level contract permits that operation.

Sequence operations preserve element order. Ordered maps and sets define iteration through their ordering policy. Hash-based maps
and sets define lookup behavior but do not promise an order unless their concrete type explicitly provides one. Public output and
tests must not treat unspecified hash iteration order as canonical ordering.

Collection indexing uses the compiler-known indexing contracts where expression syntax participates. Named lookup operations use
typed results for absence and do not return fabricated default values. Bounds errors follow the declared result or panic contract
of the operation rather than relying on unchecked access.

## Formatting

`std.format` separates three concerns:

- a value's typed formatting implementation,
- the formatting request and options,
- and the destination sink receiving text or bytes.

Formatting writes incrementally to a sink. It does not require every formatted value to allocate an intermediate `string`.
Convenience operations that return a `string` are explicit allocation-bearing wrappers over the sink-based contract.

Format arguments retain their semantic types until the selected formatting implementation consumes them. The implementation does
not parse a type-erased host-language value or depend on debug reflection. Formatting options such as radix, precision, width,
alignment, sign, and escaping are typed policy values with deterministic defaults.

The core formatting contract is independent of terminals, files, locales, and operating-system streams. `std.io` adapts its
writers to the formatting sink contract. Compiler diagnostics continue to use `bray-messages`; the standard formatting library is
not a replacement for compiler message localization.

Default formatting is deterministic for equal values and equal options. Container formatting follows the container's specified
iteration order. A container without specified iteration order must not acquire a false canonical order through default
formatting.

## Hashing And Ordering

`std.hash` defines value-to-hash-state contribution separately from the chosen hash algorithm. Hashable implementations contribute
their semantic components to an abstract hash state in a defined order; containers choose the concrete state and policy they use.

Hash equality obeys the equality contract: values equal under the selected equality policy must contribute equal hashes under the
matching hash policy. A hash value is not an object identity, a serialization, or a persistence format unless a specifically named
stable hashing API defines that contract.

Hash-based containers own their hashing policy. Security-randomized and reproducible policies are distinct choices. Compiler and
build determinism must not depend on unspecified process-randomized hashes or hash-table iteration order.

`std.order` builds sorting, searching, minimum, maximum, and ordering adapters over the compiler-known `Comparable<Rhs>` contract
and `Ordering` result. Stable and unstable algorithms are named or typed distinctly. A comparison callback must define a coherent
ordering for the values presented to the algorithm; algorithms do not repair inconsistent comparison behavior.

## Numeric Utilities

`std.numeric` complements language-defined scalar operations without adding implicit conversions or promotions. It provides:

- numeric limits and classification,
- checked arithmetic and overflow results,
- parsing and text conversion,
- explicit rounding, truncation, saturation, and wrapping policies,
- and focused integer, real, and complex algorithms.

Utilities preserve the exact operand and result types stated by their signatures. Machine-sized types use the selected target's
defined width. Parsing never depends on the host process locale. Operations whose behavior differs for integer, real, or complex
domains expose that difference through overloads, traits, or typed policy rather than an untyped mode flag.

The recognized conversion and numeric-policy operations at the `std` root retain the exact identities and semantics defined by the
language specification. Named helpers may build on them but cannot weaken their range, representation, rounding, or failure
contracts.

## Dependency Direction

Core data modules follow this dependency direction:

1. compiler-known types, traits, and `std.memory` establish the substrate,
2. `std.bytes`, `std.string`, and `std.character` establish byte, text, and character utilities,
3. `std.iteration` establishes reusable traversal behavior,
4. `std.hash`, `std.order`, and `std.numeric` establish focused policies and algorithms,
5. `std.collection` builds owning containers over memory, iteration, hashing, and ordering,
6. `std.format` consumes text, bytes, iteration, and focused value contracts,
7. I/O, testing, networking, concurrency, and higher libraries consume the core data surface.

Cycles between public modules are not used to hide missing abstractions. A small shared contract belongs in the lowest module that
semantically owns it. An implementation-only dependency may live in a private `std.*` support package when it requires a separate
native or compilation boundary, but it does not become visible through the public package graph.

## Target And Artifact Contract

Target-independent declarations produce equal observable results for equal semantic inputs on every target. Target-width behavior
is explicit through types such as `usize`, `isize`, and target facts. Endianness, pointer width, native handles, and host locale do
not leak into portable text, byte, collection, formatting, hashing, ordering, or numeric contracts.

The core data modules compile as ordinary source in the canonical `std:library` product. Their public declarations are published
through the standard package interface. Native implementation support, when required, is selected through the standard-library
artifact and private runtime contracts rather than through undeclared host calls.

Package-interface compatibility records declaration identities and caller-visible semantic contracts. It does not expose private
collection layouts, allocator bookkeeping, hash-table capacity, string representation, or formatting implementation details.

## Conformance

Conformance coverage must verify:

- valid and invalid UTF-8 boundaries,
- byte and scalar indexing distinctions,
- ownership transfer and borrowed-view dependencies,
- buffer growth, initialized-prefix, and lifecycle behavior,
- consuming, shared, and mutable iteration,
- collection ordering and absence contracts,
- deterministic formatting for specified-order values,
- equality and matching hash behavior,
- stable and unstable ordering algorithms,
- numeric boundaries and explicit conversion policies,
- target-independent behavior across supported target profiles,
- and use of the public surface across compiled package boundaries.

Tests should use ordinary Bray source and the same standard-library artifacts selected for user products. Host-language mirrors may
test a private ABI or artifact boundary, but they cannot substitute for public Bray conformance.

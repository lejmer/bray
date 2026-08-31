# Core Data Standard Library

This document defines the package structure and contracts for Bray's ordinary core data library. It covers text, bytes,
collections, iteration utilities, formatting, hashing, ordering, and numeric utilities without turning those library
APIs into language primitives.

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

The public surface must remain usable by packages that provide their own allocators, containers, formatters, or I/O
layers. Core data abstractions therefore depend on language contracts and narrow standard-library interfaces rather than
on a particular host runtime or operating system.

## Module Layout

The public modules are:

| Module           | Responsibility                                                                     |
|------------------|------------------------------------------------------------------------------------|
| `std.string`     | UTF-8 validation, scalar and byte views, text search, comparison, and conversion   |
| `std.character`  | Unicode scalar conversion, classification, and encoding utilities                  |
| `std.bytes`      | Borrowed byte views, owned byte buffers, copying, comparison, and encoding support |
| `std.iteration`  | Iterator adapters and algorithms over the compiler-known iteration traits          |
| `std.collection` | General-purpose sequences, maps, sets, queues, and their views                     |
| `std.format`     | Typed formatting, format arguments, formatters, and text or byte sinks             |
| `std.hash`       | Hashing contracts, hash state, and standard hash implementations                   |
| `std.order`      | Ordering helpers and algorithms over compiler-known comparison contracts           |
| `std.numeric`    | Numeric limits, checked arithmetic helpers, parsing, and explicit numeric policies |
| `std.memory`     | The separately specified low-level memory and allocation surface                   |

Submodules may group focused families without changing these ownership boundaries. A module must not re-export another
module's complete surface merely to shorten paths. Cross-module convenience functions belong with the abstraction whose
contract they implement.

The `std` package root may expose a deliberately small set of universal operations such as recognized conversion
functions. It must not make the complete core data surface ambient or duplicate every declaration from the modules
above.

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

These declarations retain their compiler-known identities and language-defined contracts. The standard library does not
declare replacement `String`, `Character`, `Result`, `Ordering`, `Iterable`, or operator-trait types.

The recognized `std.string` and `std.memory` declarations are exactly those in the language conformance catalog. Their
stable package and declaration identities are part of compiler recognition. Other core data declarations are ordinary
declarations even when the compiler optimizes their bodies.

Collection types, formatting traits, hashing traits, parsing errors, and iterator adapters are not compiler-known merely
because they are widely used. Promoting any such declaration to a recognized identity requires an owning
language-specification change and an update to the conformance catalog.

## Text

`string` remains the immutable compiler-known UTF-8 text value. Its representation is protected and its language-defined
copy contract preserves its abstract sequence of Unicode scalar values.

`std.string` provides:

- scalar count, emptiness, equality, scalar indexing, and scalar slicing through the recognized operations,
- exact UTF-8 byte observation,
- validated construction from UTF-8 bytes,
- scalar and byte iteration,
- searching, prefix, suffix, splitting, trimming, and comparison utilities,
- and explicit conversions between text, characters, bytes, and parsed values.

Text APIs distinguish byte offsets, Unicode scalar indexes, and collection positions with typed values or unambiguous
parameter contracts. A byte offset is never silently interpreted as a scalar index. Operations that can encounter
malformed external bytes return a typed error. Operations over an existing `string` may rely on its valid UTF-8
invariant.

Borrowed text views carry a dependency on their source text or source byte storage. An API returning such a view must
express that dependency in its callable contract. An owning text transformation returns a new `string` and may allocate.
An observing operation accepts a shared borrow unless ownership transfer is intrinsic to the operation.

Text comparison is defined over Unicode scalar values unless an API explicitly states bytewise behavior.
Locale-sensitive collation, normalization, grapheme segmentation, and case conversion are separate policy-bearing
facilities and must not be hidden inside basic equality, ordering, indexing, or slicing.

### Public Text Surface

The text surface uses `&string` as its borrowed text view and `&[u8]` as its borrowed UTF-8 byte view. It does not
introduce `StringView`, `TextView`, `ByteView`, or index-wrapper types that add no invariant beyond those structural
forms. Scalar positions and UTF-8 byte offsets use `usize` and remain distinguished by the operation that accepts them.

The public text API has these exact signatures:

```bray
module std.string;

union Utf8Error
{
    InvalidEncoding;
}

impl string
{
    func length() -> usize;
    func is_empty() -> bool;
    func get(index: usize) -> char?;
    func slice(start: usize, end: usize) -> string;
    func characters() -> ScalarCursor;
    func as_bytes() -> &[u8];
    static func from_utf8(pos bytes: &[u8]) -> Result<string, Utf8Error>;
}

impl StringEquatable = string(Equatable<string>)
{
    func equals(pos rhs: &string) -> bool;
}

```

`get` returns `none` when `index` is outside the scalar sequence. `slice` uses the half-open scalar range `[start, end)`,
allocates an owning result, and panics when `start > end` or either bound exceeds `length()`. `as_bytes` returns a view
dependent on the receiver. `from_utf8` returns `Utf8Error.InvalidEncoding` for malformed UTF-8 and otherwise constructs
an owning `string` whose scalar sequence is the decoded input. `Equatable<string>` defines ordinary `==` and `!=`
behavior for strings.

These methods are ordinary Bray bodies over internal recognized primitives. The primitives retain the compiler hooks
for scalar counting, emptiness, equality, scalar access and slicing, UTF-8 byte observation, and validated decoding.
They are not part of the public API.

Scalar iteration uses one public cursor identity and an ordinary named implementation:

```bray
module std.string;

struct ScalarCursor;

impl ScalarCursorIterator = ScalarCursor(Iterator)
{
    type Element = char;

    mut func next() -> char?;
}
```

The cursor representation is private. `characters` returns a cursor that depends on its receiver. The cursor advances
in Unicode scalar order and remains exhausted after returning `none`. Byte iteration uses the slice returned by
`as_bytes` and the ordinary slice iteration contract rather than a second string-specific byte cursor.

The `std.character` API is:

```bray
module std.character;

struct Utf8Encoding
{
    internal bytes: [u8; 4];
    internal length: usize;

    func as_slice() -> &[u8];
}

impl char
{
    func code_point() -> u32;
    static func from_code_point(pos value: u32) -> char?;
    func encode_utf8() -> Utf8Encoding;
    func is_alphabetic() -> bool;
    func is_numeric() -> bool;
    func is_whitespace() -> bool;
}
```

`from_code_point` returns `none` for values that are not Unicode scalar values. `encode_utf8` returns a value containing
exactly one character's UTF-8 encoding. `as_slice` returns a slice of its initialized prefix. The character-classification
contract uses Unicode 17.0.0 and is independent of the host locale. The selected standard-library artifact owns the
exact data used by these operations. Changing the classification data requires rebuilt standard-library artifacts, so
an unchanged artifact cannot silently acquire new classification behavior from a host toolchain update. Case conversion
and normalization remain separate policy-bearing additions because one input scalar can produce multiple output scalars.

The standard-library artifact metadata records the Unicode data version, the digests of every Unicode Character Database
input, and the revision of the deterministic table generator. Generated tables are checked against those identities
during the standard library build. A Unicode update changes this metadata, generated tables, bundle identity,
conformance fixtures, and every affected semantic operation in one coordinated change. Host libraries and host locale
data are never an alternate source of Unicode behavior.

## Bytes And Buffers

Borrowed byte data uses slice and borrow forms over `u8`. `std.bytes` may provide named views when they add a real
contract, but a named wrapper must not exist solely to rename `&[u8]`.

An owned byte buffer:

- owns its allocation and initialized bytes,
- is movable but not copyable,
- exposes shared and mutable borrowed slices without transferring allocation ownership,
- tracks length separately from capacity,
- preserves initialized-prefix and bounds invariants,
- destroys its initialized values before releasing storage,
- and uses explicit reserve, resize, truncate, append, and extraction operations.

Growing a buffer may replace its allocation. Existing views prevent growth or mutation whenever ordinary borrowing rules
make the operation incompatible. Capacity is not part of byte-sequence equality or ordering.

Safe byte operations are implemented over the `std.memory` contracts. Trusted code may bridge between raw allocation
state and the safe buffer invariant, but callers of the safe surface do not inherit raw-memory obligations.

Encoding and decoding APIs name their encoding and failure policy. UTF-8 conversion uses the recognized `std.string`
operations. No byte API silently assumes host endianness, native integer width, or null termination.

### Public Buffer Surface

The owning byte-buffer identity is `std.bytes.Buffer`. Its representation is private and contains one
`std.memory.RawBuffer<u8>` whose initialized prefix is the buffer's byte sequence. The type is movable and not copyable.

```bray
let empty = try std.bytes.Buffer();
let reserved = try std.bytes.Buffer(capacity = 4096);
let copied = try std.bytes.Buffer.from_slice(source);
```

The public surface is:

```bray
module std.bytes;

struct Buffer
{
    trusted construct(capacity: usize = 0) -> Result<Self, std.memory.MemoryLayoutError>;
    trusted construct from_slice(pos bytes: &[u8]) -> Result<Self, std.memory.MemoryLayoutError>;

    func as_slice() -> &[u8];
    mut func as_slice_mut() -> &mut [u8];
}

func length(pos buffer: &Buffer) -> usize;

func capacity(pos buffer: &Buffer) -> usize;

func equals(pos left: &[u8], pos right: &[u8]) -> bool;

func reserve(
    pos buffer: &mut Buffer,
    additional: usize,
) -> Result<unit, std.memory.MemoryLayoutError>;

func reserve_exact(
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
    pos bytes: &[u8],
) -> Result<unit, std.memory.MemoryLayoutError>;

func append(
    pos buffer: &mut Buffer,
    pos value: u8,
    pos count: usize,
) -> Result<unit, std.memory.MemoryLayoutError>;

func pop(pos buffer: &mut Buffer) -> u8?;
```

The `new` overload selects empty, capacity, or byte-slice construction from the supplied arguments. Construction returns
`MemoryLayoutError` when the requested capacity cannot be represented. Allocation
failure follows the language allocation panic contract. `equals` compares complete byte sequences without allocation.
`reserve` guarantees capacity for `length(buffer) + additional` without changing the byte sequence and uses the stable
geometric growth policy. `reserve_exact` grows only to the required capacity for callers that know the final size.
`resize` preserves the existing prefix, truncates when shrinking, and appends `fill` bytes when growing. `truncate`
leaves the buffer unchanged when `new_length >= length(buffer)`. `pop` returns `none` for an empty buffer.

`as_slice` and `as_slice_mut` return views dependent on `buffer`. Any operation requiring mutation or possible
reallocation is rejected while an incompatible view remains live by ordinary borrowing rules. A caller therefore cannot
pass a view reaching `buffer` as the `bytes` argument of `append` while also supplying the required mutable borrow of
that buffer.

Private standard-library support may transfer a `Buffer` to or from its `RawBuffer<u8>` representation. That bridge is
not part of the public `std.bytes` surface and does not expose raw allocation details to ordinary callers.

Byte-buffer construction, append, and buffered I/O use the compiler-recognized byte-slice bulk transfer operation. The
operation accepts an initialized source slice and distinct writable destination storage, so these paths lower to one
native memory transfer instead of a standard-library loop while preserving slice bounds and aliasing checks at the
caller boundary.

## Iteration

The compiler-known `Iterable` and `Iterator` traits define source-to-cursor and cursor-advance semantics.
`std.iteration` provides ordinary adapters and algorithms over those traits.

Adapters preserve the operation mode of their source:

- iteration over a shared borrow yields shared access according to the selected implementation,
- iteration over a mutable borrow yields mutation authority only where the implementation contract permits it,
- and consuming iteration may move elements from an owning source.

An adapter that stores a cursor or callable owns those values. An adapter over borrowed storage carries the
corresponding dependency. Lazy adapters do not evaluate source elements until iteration advances them. Terminal
algorithms document whether they short-circuit and whether they preserve source order.

Algorithms requiring multiple passes, exact size, stable ordering, random access, or contiguous storage use explicit
additional contracts. They do not infer those properties from `Iterable` alone.

The standard adapter surface covers transformation, filtering, flattening, bounded traversal, indexing, peeking, and
chaining. `IteratorOperations` supplies `first`, `nth`, `take`, `skip`, and `enumerate` as default methods for every
`Iterator`. Adapters such as `map`, `filter`, `flat_map`, `take`, `skip`, `enumerate`, `peekable`, and `chain` own their
source cursors, advance them only when the adapter advances, and remain exhausted after their sources are exhausted.
`take` produces at most the requested count. `skip` consumes at most the requested count before producing the remaining
elements. `enumerate` pairs each produced element with a zero-based `usize` index and preserves source order.

Terminal algorithms cover element selection, searching, predicates, folds, comparison, counting, and collection.
Operations such as `first`, `nth`, `find`, `any`, and `all` consume one cursor, do not allocate, and stop once their
result is known. Folds and collection operations state their accumulation and allocation behavior. Algorithms that must
exhaust a source require caller-visible contracts that establish finiteness rather than assuming every `Iterator` is
finite.

## Collections

`std.collection` provides focused owning collection types rather than one universal container abstraction. Its
collection families are:

- growable contiguous sequences,
- double-ended queues,
- hash maps and hash sets,
- ordered maps and ordered sets,
- and narrow stack or queue adapters when they add a useful contract.

Owning collections are movable and are not copyable because copying them can allocate and run element behavior. Explicit
clone or duplicate operations state their element requirements and failure behavior. Shared and mutable collection views
are borrows whose dependencies follow ordinary Bray rules.

Insertion transfers or constructs ownership according to the operation signature. Removal returns ownership of removed
values when applicable. Reallocation moves elements according to their movement and lifecycle contracts. It does not
bitwise-relocate values unless a recognized low-level contract permits that operation.

Sequence operations preserve element order. Ordered maps and sets define iteration through their ordering policy.
Hash-based maps and sets define lookup behavior but do not promise an order unless their concrete type explicitly
provides one. Public output and tests must not treat unspecified hash iteration order as stable ordering.

Collection indexing uses the compiler-known indexing contracts where expression syntax participates. Named lookup
operations use typed results for absence and do not return fabricated default values. Bounds errors follow the declared
result or panic contract of the operation rather than relying on unchecked access.

### Contiguous Sequence Surface

`std.collection.List<T>` is an owning contiguous sequence. It keeps an initialized prefix and is movable but not
copyable. Its public surface is:

```bray
module std.collection;

struct List<T>
{
    trusted construct(capacity: usize = 0) -> Result<Self, std.memory.MemoryLayoutError>;

    func as_slice() -> &[T];
    mut func as_slice_mut() -> &mut [T];
}

func length<T>(pos list: &List<T>) -> usize;

func capacity<T>(pos list: &List<T>) -> usize;

func is_empty<T>(pos list: &List<T>) -> bool;

trusted func reserve<T>(
    pos list: &mut List<T>,
    additional: usize,
) -> Result<unit, std.memory.MemoryLayoutError>;

trusted func reserve_exact<T>(
    pos list: &mut List<T>,
    additional: usize,
) -> Result<unit, std.memory.MemoryLayoutError>;

trusted func push<T>(
    pos list: &mut List<T>,
    pos value: T,
) -> Result<unit, std.memory.MemoryLayoutError>;

trusted func pop<T>(pos list: &mut List<T>) -> T?;

trusted func insert<T>(
    pos list: &mut List<T>,
    pos value: T,
    index: usize,
) -> Result<bool, std.memory.MemoryLayoutError>;

trusted func remove<T>(pos list: &mut List<T>, index: usize) -> T?;

trusted func truncate<T>(pos list: &mut List<T>, new_length: usize) -> unit;

trusted func clear<T>(pos list: &mut List<T>) -> unit;

func reverse<T>(pos list: &mut List<T>) -> unit;
```

`reserve` uses deterministic geometric growth with a minimum non-zero capacity of four. `reserve_exact` grows only to
the required capacity. Both preserve existing elements in source order and leave the sequence unchanged when current
capacity is sufficient. Capacity arithmetic returns `MemoryLayoutError.SizeOverflow` rather than wrapping.

`insert` accepts positions from zero through `length(list)` and returns `false` without changing the list for larger
indexes. `remove` and `pop` return `none` when no element exists at the requested position. `truncate` destroys removed
elements from the end and leaves the list unchanged when the requested length is not smaller.

The `&List<T>`, `&mut List<T>`, and consuming `List<T>` implementations of `Iterable` preserve sequence order and
respectively yield `&T`, `&mut T`, and owned `T` values. Shared and mutable cursors retain the corresponding slice
borrow and its dependency. Consuming cursor destruction resolves every element that has not yet been produced before
releasing its allocation.

### Double-Ended Sequence Surface

`std.collection.Deque<T>` is an owning sequence optimized for insertion and removal at both ends. It provides front and
back access, push and pop operations at either end, indexed access where its complexity contract permits it, capacity
management, and shared, mutable, and consuming iteration in logical sequence order. Its representation may wrap
internally, but public views never expose uninitialized or out-of-order storage.

Construction accepts an optional initial capacity. `length`, `capacity`, and `is_empty` observe the logical sequence;
`front`, `back`, `front_mut`, and `back_mut` return `none` for an empty deque; and `push_front`, `push_back`, `pop_front`,
`pop_back`, `reserve`, `reserve_exact`, `shrink_to_fit`, and `clear` preserve logical order across wrapped storage.
Capacity arithmetic reports `MemoryLayoutError.SizeOverflow` rather than wrapping. Indexed access is constant time and
uses logical positions, while shared, mutable, and consuming iteration is linear in that same order. Growth and
normalization move every live element exactly once and never treat spare storage as an initialized `T`.

### Hash Collection Surface

`std.collection.HashMap<Key, Value, Hasher>` and `std.collection.HashSet<Value, Hasher>` provide expected constant-time
lookup under the selected hashing policy. Their construction makes the hashing policy explicit or selects the standard
process-local policy. Stable hashing is used only where a caller explicitly requests deterministic cross-run hashes.
Hash collections expose entry-style mutation, insertion, replacement, removal, containment, capacity management, and
shared, mutable, and consuming iteration without claiming a stable iteration order.

Hash collection keys require compatible hashing and equality contracts. Mutating a key through an alias while it belongs
to a hash collection is prevented by ownership and borrowing rather than tolerated as an invalid table state.

### Ordered Collection Surface

`std.collection.OrderedMap<Key, Value>` and `std.collection.OrderedSet<Value>` maintain keys according to their
comparison contract. They provide ordered lookup, insertion, replacement, removal, range traversal, and shared, mutable,
and consuming iteration. Ordering must be total and consistent for the stored key type. Range APIs represent inclusive
and exclusive bounds explicitly and preserve ascending order unless the operation explicitly requests reverse traversal.

### Collection Adapters

Stack and queue types are narrow adapters over sequence storage when their restricted interfaces communicate a useful
invariant. They do not duplicate storage engines solely to provide alternate names. Their direct operations expose only
the ordering policy that defines the adapter, while projection and conversion to and from the underlying owning
collection are explicit.

```bray
let stack = try trusted std.collection.Stack<Item>();
let queue = try trusted std.collection.Queue<Item>(capacity = 32);
let adopted = std.collection.Queue<Item>.from_deque(storage);
```

`std.collection.Stack<T>` exposes `push`, `pop`, `peek`, and `peek_mut` with last-in-first-out ordering.
`std.collection.Queue<T>` exposes `enqueue`, `dequeue`, `peek`, and `peek_mut` with first-in-first-out ordering. Their
primary constructors create empty adapters with an optional capacity. `from_deque` makes ownership-changing adoption of
an existing `Deque<T>` explicit. Both adapters share `length`, `capacity`, `is_empty`, `reserve`, and `clear` through
`DequeAdapter<T>`, store one `Deque<T>` without an additional dispatch layer, project it explicitly with `as_deque` and
`as_deque_mut`, and convert explicitly with `into_deque`.

## Formatting

`std.format` separates three concerns:

- a value's typed formatting implementation,
- the formatting request and options,
- and the destination sink receiving text or bytes.

Formatting writes incrementally to a sink. It does not require every formatted value to allocate an intermediate
`string`. Convenience operations that return a `string` are explicit allocation-bearing wrappers over the sink-based
contract.

```bray
let ordinary = std.format.Options();
let hexadecimal = std.format.Options(radix = std.format.Radix.HexadecimalLowercase, width = 8);
let argument = std.format.Argument<u32>(&value, options = hexadecimal);
```

Format arguments retain their semantic types until the selected formatting implementation consumes them. The
implementation does not parse a type-erased host-language value or depend on debug reflection. Formatting options such
as radix, precision, width, alignment, sign, and escaping are typed policy values with deterministic defaults.
`Options()` supplies ordinary defaults and named arguments override individual choices. Nullable precision distinguishes
an omitted precision from an explicit precision of zero. `Argument(value)` uses default options, while its named
`options` parameter accepts an explicit policy value.

The core formatting contract is independent of terminals, files, locales, and operating-system streams. `std.io` adapts
its writers to the formatting sink contract. Compiler diagnostics continue to use `bray-messages`. The standard
formatting library is not a replacement for compiler message localization.

Default formatting is deterministic for equal values and equal options. Container formatting follows the container's
specified iteration order. A container without specified iteration order must not acquire a false stable order through
default formatting.

## Hashing And Ordering

`std.hash` defines value-to-hash-state contribution separately from the chosen hash algorithm. Hashable implementations
contribute their semantic components to an abstract hash state in a defined order. Containers choose the concrete state
and policy they use.

Hash equality obeys the equality contract: values equal under the selected equality policy must contribute equal hashes
under the matching hash policy. A hash value is not an object identity, a serialization, or a persistence format unless
a specifically named stable hashing API defines that contract.

Hash-based containers own their hashing policy. Security-randomized and reproducible policies are distinct choices.
Compiler and build determinism must not depend on unspecified process-randomized hashes or hash-table iteration order.

The reproducible hashing surface uses the named Bray stable hash algorithm. Its state starts at `14695981039346656037`.
Each contributed `u128` component updates the state to
`((state + component) mod 170141183460469231731687303715884105727 * 1099511628211) mod 170141183460469231731687303715884105727`.
Compound values contribute their semantic components in their documented order. Equal values therefore produce equal
stable hashes across processes and supported targets. A different stable algorithm requires a separately named API
rather than silently changing this contract.

`std.order` builds sorting, searching, minimum, maximum, and ordering adapters over the compiler-known `Comparable<Rhs>`
contract and `Ordering` result. Stable and unstable algorithms are named or typed distinctly. A comparison callback must
define a coherent ordering for the values presented to the algorithm. Algorithms do not repair inconsistent comparison
behavior. `OrderedSequence` defines `find`, `binary_search`, `sort_stable`, and `sort_unstable` as default methods in
terms of its required length, comparison, and swap operations.

## Numeric Utilities

`std.numeric` complements language-defined scalar operations without adding implicit conversions or promotions. It
provides:

- numeric limits and classification,
- checked arithmetic and overflow results,
- parsing and text conversion,
- explicit rounding, truncation, saturation, and wrapping policies,
- and focused integer, real, and complex algorithms.

Utilities preserve the exact operand and result types stated by their signatures. Machine-sized types use the selected
target's defined width. Parsing never depends on the host process locale. Operations whose behavior differs for integer,
real, or complex domains expose that difference through overloads, traits, or typed policy rather than an untyped mode
flag.

The public `Integer` trait defines the common integer contract required by generic checked arithmetic. Its required
`bounds` member returns `IntegerBounds<Self>`, whose named fields state zero, negative one, minimum, and maximum without
positional tuple conventions. The trait defines checked add, subtract, multiply, and divide as default methods. Every
language-defined integer type implements the bounds contract.

The recognized conversion and numeric-policy operations at the `std` root retain the exact identities and semantics
defined by the language specification. Named helpers may build on them but cannot weaken their range, representation,
rounding, or failure contracts.

## Dependency Direction

Core data modules follow this dependency direction:

1. compiler-known types, traits, and `std.memory` establish the substrate,
2. `std.bytes`, `std.string`, and `std.character` establish byte, text, and character utilities,
3. `std.iteration` establishes reusable traversal behavior,
4. `std.hash`, `std.order`, and `std.numeric` establish focused policies and algorithms,
5. `std.collection` builds owning containers over memory, iteration, hashing, and ordering,
6. `std.format` consumes text, bytes, iteration, and focused value contracts,
7. I/O, testing, networking, concurrency, and higher libraries consume the core data surface.

Cycles between public modules are not used to hide missing abstractions. A small shared contract belongs in the lowest
module that semantically owns it. An implementation-only dependency may live in a private `std.*` support package when
it requires a separate native or compilation boundary, but it does not become visible through the public package graph.

## Target And Artifact Contract

Target-independent declarations produce equal observable results for equal semantic inputs on every target. Behavior
that depends on target width is explicit through types such as `usize`, `isize`, and target properties. Endianness,
pointer width, native handles, and host locale do not leak into portable text, byte, collection, formatting, hashing,
ordering, or numeric contracts.

The core data modules compile as ordinary source in the stable `std:library` product. Their public declarations are
published through the standard package interface. Native implementation support, when required, is selected through the
standard-library artifact and private runtime contracts rather than through undeclared host calls.

Package-interface compatibility records declaration identities and caller-visible semantic contracts. It does not expose
private collection layouts, allocator bookkeeping, hash-table capacity, string representation, or formatting
implementation details.

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

Tests should use ordinary Bray source and the same standard-library artifacts selected for user products. Host-language
mirrors may test a private ABI or artifact boundary, but they cannot substitute for public Bray conformance.

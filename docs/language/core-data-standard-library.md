# Core data standard library

Core data APIs are ordinary declarations in `std`. They reuse the language's text, numeric, iteration, comparison, and
memory contracts. The [library design](../design/core-data-standard-library.md) describes their structure. This chapter
collects caller-visible behavior that is specific to these library operations.

## Text and characters

Text positions count Unicode scalars unless an operation explicitly accepts UTF-8 byte offsets. `string.get` returns
`none` outside the scalar sequence. `string.slice` returns an owning string for a half-open scalar range and panics for
reversed or out-of-bounds ranges. `as_bytes` returns a dependent view, while `from_utf8` validates external bytes and
returns an owning string or `Utf8Error.InvalidEncoding`.

Scalar cursors preserve scalar order and remain exhausted after returning `none`. Byte traversal uses the ordinary byte
slice contract. Basic equality and ordering do not implicitly normalize text or apply locale-sensitive collation.

Character conversion rejects values outside the Unicode scalar domain. UTF-8 encoding produces exactly one scalar's
encoding. Character classification uses Unicode 17.0.0 data selected with the standard-library artifact and is
independent of host locale. Case conversion and normalization have separate policy contracts.

## Bytes and sequences

`std.bytes.Buffer` owns a byte sequence. Its shared and mutable views depend on that owner. Equality compares byte
contents, not capacity. Construction and capacity arithmetic report `MemoryLayoutError` for unrepresentable layouts,
while allocation failure follows the language's allocation panic contract.

`reserve` provides capacity for the current length plus the requested additional amount without changing contents.
`reserve_exact` requests only the required capacity. `resize` preserves the existing prefix, truncates when shrinking,
and appends the supplied fill byte when growing. Truncating to a length no smaller than the current length changes
nothing. `pop` returns `none` for an empty buffer. Mutation and reallocation require the ordinary exclusive access, so
an incompatible live view cannot be used as an append source into the same buffer.

`std.collection.List<T>` preserves sequence order. Insertion accepts positions through the current length and returns
`false` without mutation for larger indexes. Removal and pop return `none` when no element exists. Truncation destroys
removed elements from the end. Shared, mutable, and consuming iteration respectively yield shared borrows, mutable
borrows, and owned values in order. A consuming cursor resolves elements it has not yielded before releasing ownership.

`Deque<T>` uses logical positions and preserves sequence order across storage wrapping. Front and back observations
return `none` when empty. Indexed access is constant time, and traversal is linear. `Stack<T>` exposes last-in-first-out
operations and `Queue<T>` exposes first-in-first-out operations. Construction creates an empty adapter, while
`from_deque` explicitly adopts existing storage.

## Iteration

Adapters retain their source cursor, callable, and borrowed dependencies. They advance lazily. `take` yields at most its
count, `skip` consumes at most its count before yielding the remainder, and `enumerate` preserves order with zero-based
`usize` indexes. Adapters remain exhausted after their sources are exhausted.

Selection, search, and predicate operations such as `first`, `nth`, `find`, `any`, and `all` consume a cursor without
allocation and stop when their result is known. Operations requiring complete traversal require a contract establishing
finiteness. The basic iterator contract alone does not promise random access, repeated traversal, or a finite sequence.

## Maps and sets

Hash collections require compatible hashing and equality and provide expected constant-time lookup under healthy load.
Borrowed lookup does not allocate or copy the queried value. Stored keys cannot be mutated through collection access.
Mutable map iteration permits value mutation while keeping keys shared. Hash iteration order is unspecified.

Ordered collections require coherent total key ordering and iterate in ascending order unless reverse traversal is
explicit. Range bounds distinguish inclusion and exclusion. Lookup, insertion, and removal are logarithmic, including
for already sorted input. Removal transfers owned values where the operation's result provides them.

## Hashing and ordering

Equal values under the selected equality policy must contribute equal hashes under its matching hashing policy. A hash
is not an identity or serialization unless a named stable algorithm supplies that contract.

`std.hash.StableHasher` starts at `14695981039346656037`. For each contributed `u128` component, its mathematical update
is `((state + component) * 1099511628211) mod 170141183460469231731687303715884105727`. This uses modular arithmetic
without intermediate machine overflow. Compound values contribute semantic components in their documented order. Equal
contributions produce equal hashes across processes and targets. Changing this algorithm requires a separately named
API.

Stable and unstable sorting are distinct policies. Comparisons must define coherent ordering, and algorithms do not
repair inconsistent callbacks. Formatting or tests cannot assume stable order for an unordered container.

## Formatting and numeric policy

Formatting options retain typed radix, precision, width, alignment, sign, and escaping choices. Ordinary construction
uses defaults, and named options override them. Omitted precision differs from an explicit zero. Formatting streams to a
sink, while convenience operations returning a string make allocation explicit. Equal values and options produce equal
output when the value's ordering contract is defined.

Numeric utilities preserve their stated operand and result types. Target-sized integers use the selected target width,
and parsing is locale-independent. Checked arithmetic reports failure rather than silently applying another rounding,
wrapping, or saturation policy. Those policies require explicit operations.

## Related chapters

- [Language index](index.md)
- [Compiler-known declarations](compiler-known-and-standard-library.md)
- [Memory and allocation](targets-layout-abi-and-raw-memory/standard-library-memory-surface.md)
- [I/O and platform services](io-and-platform-services.md)

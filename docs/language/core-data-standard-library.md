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

### Real and complex values

The standard `Format` implementations cover `r32`, `r64`, `c64`, and `c128`, and the corresponding `r16`, `r128`, `c32`,
and `c256` forms when the selected target provides them. Formatting is independent of locale and native endianness.

For a finite real value in decimal radix, omitted precision selects the shortest significant decimal that rounds back
to that value in its real type. Among equally short candidates, choose the nearest, with ties to an even last digit.
Use the shorter of ordinary decimal and scientific notation, preferring ordinary notation when their lengths match.
Scientific notation uses lowercase `e`, omits a positive exponent sign and leading exponent zeroes, and omits a decimal
point when there is only one significant digit. Thus `1.25` formats as `1.25`, while `1000.0` formats as `1e3`.

Explicit decimal precision counts digits after the decimal point and uses fixed notation. Round the exact binary value
to that precision, with ties to even, and retain the requested trailing zeroes. Explicit zero precision produces an
integer without a decimal point. For example, the `r64` value `2.675` formats with precision two as `2.67`, because that
binary value is slightly below the decimal midpoint.

Binary, octal, and hexadecimal radix select normalized binary-exponent notation with `0b`, `0o`, or `0x` prefixes and a
`p` exponent. A nonzero finite significand lies in `[1, 2)`. Omitted precision retains its exact fraction and removes
trailing zeroes. Explicit precision counts fractional digits in the selected radix and rounds with ties to even.
Hexadecimal uppercase uses `0X`, uppercase digits and `P`. Exponents omit a positive sign and leading zeroes. For
example, hexadecimal `1.5` formats as `0x1.8p0`. Zero has significand and exponent zero.

Preserve the sign of zero, including after rounding. Infinities format as `inf` and NaNs as `nan`, preserving their sign
but omitting NaN payloads and the signaling distinction. Hexadecimal uppercase selects `INF` and `NAN`. Precision does
not add fractional digits to these special values. The selected sign policy applies to every real value.

Complex values format as `real+imaginaryi` or `real-imaginaryi`. Each component uses the same real formatting rules,
radix and precision. The sign policy applies to the real component, and the imaginary component always has its own
`+` or `-` separator, including for signed zero and special values. Width and alignment apply to the entire complex
output, with no component padding. For example, `1.25 - 2.5i` with precision two formats as `1.25-2.50i`.

Real and complex formatting use the sink-generic protocol with bounded local conversion storage. A fixed destination
performs no heap allocation. Precision and width beyond the significant digits produce bulk repeated writes, and sink
failures retain their original type and partial-output behavior.

### Numeric operations

Numeric utilities preserve their stated operand and result types. Target-sized integers use the selected target width,
and parsing is locale-independent. Checked arithmetic reports failure rather than silently applying another rounding,
wrapping, or saturation policy. Those policies require explicit operations.

## Related chapters

- [Language index](index.md)
- [Compiler-known declarations](compiler-known-and-standard-library.md)
- [Memory and allocation](targets-layout-abi-and-raw-memory/standard-library-memory-surface.md)
- [I/O and platform services](io-and-platform-services.md)

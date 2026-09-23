# Core data standard library

Core data modules provide ordinary Bray abstractions for text, bytes, iteration, collections, formatting, hashing,
ordering, and numeric utilities. They reuse language-owned types and traits. Widespread use alone does not make a
library declaration compiler-known.

## Modules and dependencies

| Module                        | Responsibility                                       |
|-------------------------------|------------------------------------------------------|
| `std.string`, `std.character` | Text observation, conversion, and Unicode operations |
| `std.bytes`                   | Byte storage and encoding support                    |
| `std.iteration`               | Lazy adapters and traversal algorithms               |
| `std.collection`              | Owning containers and their borrowed views           |
| `std.format`                  | Typed formatting requests and sinks                  |
| `std.hash`, `std.order`       | Hashing and ordering policies and algorithms         |
| `std.numeric`                 | Explicit numeric policies and utilities              |

Compiler-known contracts and `std.memory` support bytes, text, and traversal. Hashing and ordering support collections.
Formatting consumes these lower-level abstractions, and I/O and other service libraries adapt them. Shared contracts
belong with their lowest semantic owner rather than being hidden behind module cycles or broad re-exports.

## Ownership and representation

Owning containers expose allocation and movement through their APIs. Borrowed views use existing structural forms when
those forms already express the invariant, such as `&string` and slices. A wrapper needs a stronger contract than a new
name. Views retain their source dependency, while transformations that allocate or transfer ownership say so.
Fixed byte data uses ordinary fixed `u8` arrays. The `bytes` spelling and byte string literals introduce no separate
storage or runtime type.

Safe buffers and collections build on trusted memory operations with an initialized-storage invariant. Length and
capacity remain distinct. Growth and element movement respect ordinary lifecycle rules, and consuming iteration owns
cleanup of elements it has not produced. Bulk byte transfer uses the recognized memory operation instead of duplicating
it as library loops.

Collections have focused representations. Contiguous sequences favor indexed traversal, deques support both ends, hash
tables use probing and deletion that preserves probe reachability, and ordered maps and sets share balanced-tree
machinery. Stack and queue adapters reuse sequence storage. Sets store their values directly rather than fabricating map
payloads. These choices support each family's operation costs without exposing private layout as a caller contract.

## Text and portability

`string` keeps its language-owned UTF-8 identity. Text operations distinguish scalar positions from byte offsets, and
basic comparison does not hide locale collation or normalization. Malformed external bytes cross a typed validation
boundary. Unicode classification uses pinned data packaged with the standard library, with version, source digests, and
generator provenance participating in artifact identity.

Portable operations have target-independent results. Target-width behavior is explicit in types and target properties.
Host locale, native endianness, and private handle representation do not become hidden inputs to core data operations.

## Reusable algorithms

Iterator adapters own their cursors and callables, retain borrowed dependencies, and evaluate lazily. Algorithms request
additional capabilities when they need multiple passes, random access, or finite traversal. They do not infer those
properties from the basic iterator contract. Default trait methods supply common traversal behavior across
implementations.

Formatting separates typed value formatting, policy options, and the destination sink. Streaming to a sink avoids an
intermediate allocation for every value. I/O provides sink adapters, while compiler diagnostics keep their separate
message-catalog system. Formatting preserves the order promised by the value's own contract.

Hashing separates semantic contributions from the selected algorithm. Equality and hashing use compatible policies.
Reproducible and security-randomized hashing are explicit choices, and unspecified hash iteration order cannot determine
reproducible output. Ordering algorithms similarly distinguish stable and unstable policies and share comparison
contracts across containers.

Numeric utilities build on the language's exact types and conversion rules. Typed bounds and default checked-arithmetic
methods share behavior across integer implementations. Policy-bearing operations expose rounding, saturation, or
wrapping explicitly rather than weakening the language's ordinary arithmetic.

## Related documents

- [Standard-library design](standard-library.md)
- [Core data behavior](../language/core-data-standard-library.md)
- [Memory and allocation](../language/targets-layout-abi-and-raw-memory/standard-library-memory-surface.md)

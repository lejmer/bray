# Unicode character classification

`std.character` uses the pinned Unicode 17.0.0 `Alphabetic`, `Numeric_Type` and `White_Space` properties.
`is_numeric` accepts all three numeric types, Decimal, Digit and Numeric. See the
[character contract](https://github.com/lejmer/bray/blob/develop/docs/language/core-data-standard-library.md#text-and-characters).

Generate with `cargo xtask standard-library unicode generate`. Use `generate --check` to verify freshness.
Both standard-library builds and `cargo xtask standard-library verify` check the generated source and metadata.
Input SHA-256 digests are pinned in the generator after normalizing line endings. Every target bundle publishes the
same `unicode-data.json` dependency artifact, containing the Unicode version, input digests, generator revision and
source digest, generated source digest, and table sizes.

The immutable tables form a three-level trie. Bits 10 and above select one of 1,088 root entries. Bits 5 through 9
select one of 32 entries in a deduplicated branch. The low five bits select a bit in a 32-scalar leaf bitmap.
Each predicate performs exactly two `u16` index reads and one `u8` bitmap read, plus fixed arithmetic and bounds checks.
There is no range search, allocation, locale input or host-library classification. Classifying N scalars takes O(N)
work. Invalid scalar values are rejected by `char.from_code_point` before they can reach classification.

Root and branch indexes are shared by all predicates. Leaf identity uses all three properties so the common indexes
are stored once, while each predicate retains only its own bitmap. Data size is:

| Table | Bytes |
| --- | ---: |
| Root index | 2,176 |
| Branch index | 5,248 |
| Alphabetic bitmap | 1,556 |
| Numeric bitmap | 1,556 |
| Whitespace bitmap | 1,556 |
| All tables | 12,092 |

A product with one reachable predicate needs 8,980 table bytes. All predicates together need 12,092 bytes. A product
without classification needs zero. These are table payload sizes, excluding code, alignment and product lifecycle
metadata. The native conformance command reports actual product size and table contribution and rejects duplicate
indexes or unrelated retained bitmaps.

Run `cargo test --release -p xtask unicode_data` for exhaustive comparison of every code point against the pinned
property ranges and generator freshness checks. Public API tests cover property categories, boundaries, Unicode 17
additions, supplementary planes and invalid scalar conversions. Standard-library verification checks all six target
profiles. The integer tables have identical values on every target and do not use target properties.

Run `cargo xtask standard-library test --part unicode` for imported-product retention, zero-allocation observation,
and ASCII-heavy, multilingual, mixed-plane and boundary workload conformance. Each workload classifies 65,536 scalars
per invocation. Controlled timing compares one and four invocations using three samples at each scale. A native opaque
scalar function prevents the optimizer from precomputing classifications. The generous scaling bound allows timing
noise while rejecting superlinear growth. Timing values describe the current host and are reported with the sample
counts. Memory observation separately requires zero allocations and zero copied bytes.

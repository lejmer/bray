# Recognized standard-library operations

The standard library can provide operations whose contracts are known to the compiler.

The recognized standard-library operations include:

- `std.convert<Target>(source)`, the fallible conversion operation backed by `CheckedConvertTo<Target>`,
- numeric policy operations such as `std.round_to<Target>(source, rule = ...)`, `std.truncate_to<Target>(source)`, `std.saturate_to<Target>(source)`, and `std.wrap_to<Target>(source)`,
- string operations under the `std.string` module, including scalar-value count, emptiness, equality, scalar indexing, scalar slicing, UTF-8 views, and UTF-8 construction,
- raw memory helpers under `std.memory`, including raw pointer helpers, allocation owners, raw buffers, device memory helpers, and ABI/layout helpers, when imported,
- standard storage policy types and helpers used with compiler-known type forms, when imported.

These operations are not syntax.

They are ordinary declarations with ordinary name resolution.

If the relevant standard-library declaration is not visible, the call is rejected by ordinary name resolution.

If a visible declaration is not the recognized standard-library declaration, it is checked as an ordinary call to that declaration.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Standard-library recognition](standard-library-recognition.md)
- Next: [Availability summary](availability-summary.md)

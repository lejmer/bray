# Recognized standard-library operations

The standard library can provide operations whose contracts are known to the compiler.

The exact recognized identities are listed by the
[conformance catalog](conformance-catalog.md).

`std.convert<Target, Source>(source)` returns
`Result<Target, Source(CheckedConvertTo<Target>).Error>`.

The recognized numeric-policy operations are `std.round_to`, `std.truncate_to`,
`std.saturate_to`, and `std.wrap_to`.

`std.round_to` accepts the recognized `std.RoundingRule` union.

The recognized string operations are `std.string.scalar_count`, `std.string.is_empty`,
`std.string.equals`, `std.string.scalar_at`, `std.string.scalar_slice`, `std.string.utf8`, and
`std.string.from_utf8`.

The recognized raw-memory operations, layout declarations, allocation owner, and raw-buffer
declarations are the exact `std.memory` declarations listed by the conformance catalog.

Device-memory helpers are not recognized.

These operations are not syntax.

They are ordinary declarations with ordinary name resolution.

If the relevant standard-library declaration is not visible, the call is rejected by ordinary name resolution.

If a visible declaration is not the recognized standard-library declaration, it is checked as an ordinary call to that declaration.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Standard-library recognition](standard-library-recognition.md)
- Next: [Availability summary](availability-summary.md)

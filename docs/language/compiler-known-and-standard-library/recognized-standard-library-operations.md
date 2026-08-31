# Recognized standard-library operations

The standard library can provide operations whose contracts are known to the compiler.

The exact recognized identities are listed by the [conformance catalog](conformance-catalog.md).

`std.convert<Target, Source>(source)` returns `Result<Target, Source(CheckedConvertTo<Target>).Error>`.

The recognized numeric-policy operations are `std.round_to`, `std.truncate_to`, `std.saturate_to`, and `std.wrap_to`.

`std.round_to` accepts the recognized `std.RoundingRule` union.

The public string surface is `string.length()`, `string.is_empty()`, `string.get(index)`,
`string.slice(start, end)`, `string.characters()`, `string.as_bytes()`, `string.from_utf8(bytes)`, and
`Equatable<string>`. These are ordinary standard-library implementations over the compiler-known `string` type.

The compiler recognizes their exact implementation and member identities together with the internal primitive
declarations that implement scalar counting, emptiness, equality, scalar access and slicing, UTF-8 byte observation,
and validated UTF-8 decoding. A public member is not replaced by spelling-based compiler behavior; its ordinary Bray
body selects the recognized internal primitive.

The public character surface is `char.code_point()`, `char.from_code_point(value)`, `char.encode_utf8()`,
`char.is_alphabetic()`, `char.is_numeric()`, and `char.is_whitespace()`. `std.character.Utf8Encoding.as_slice()` exposes
the initialized encoded bytes. Their exact standard-library identities and internal character primitives are recognized
in the same way.

The recognized raw-memory operations, layout declarations, allocation owner, and raw-buffer declarations are the exact
`std.memory` declarations listed by the conformance catalog.

The recognized callback-context operation is `std.ffi.callback_state<State>(context)`. It can reconstruct a state borrow
only for the live context parameter of an exported ABI callback entry and never synthesizes a callable capture.

Device-memory helpers are not recognized.

These operations are not syntax.

They are ordinary declarations with ordinary name resolution.

If the relevant standard-library declaration is not visible, the call is rejected by ordinary name resolution.

If a visible declaration is not the recognized standard-library declaration, it is checked as an ordinary call to that
declaration.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Standard-library recognition](standard-library-recognition.md)
- Next: [Availability summary](availability-summary.md)

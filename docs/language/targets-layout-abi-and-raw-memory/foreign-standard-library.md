# Foreign standard library

`std.ffi`, `std.ffi.c`, and `std.dynamic` provide ordinary library contracts over
[the language's foreign boundary](extern-declarations-and-ffi.md). Target-specific `std.os.*` modules expose native
facilities. None of these modules is ambient.

## C values and strings

Each C scalar wrapper uses its own exact `target.c` mapping. Available wrappers have transparent layout and expose the
selected Bray scalar through `value()`. An `*_exact` constructor accepts that representation. Integer wrappers also
provide explicit checked and wrapping conversions from the corresponding signed or unsigned wide integer domain.
Checked range failure is `ConversionError.OutOfRange`. Boolean and real wrappers have no wrapping conversion. An
unavailable C mapping contributes no approximate replacement type.

Borrowed and owned C strings have separate types. Narrow strings preserve raw byte units and do not assume UTF-8 until
text conversion is requested. `OwnedNarrowString.from_bytes` copies raw units, `from_utf8` names text encoding, and
`BorrowedNarrowString.to_string` decodes UTF-8. The wide forms use the target's `WCHAR` representation to choose UTF-16
or UTF-32 for text conversion. Invalid encoding and interior NUL units produce `StringError` rather than replacement text.

Borrowed narrow views provide `as_bytes` and `as_bytes_with_nul`. Wide views provide `as_units` and
`as_units_with_nul`. Owned strings expose dependent views through `as_borrowed`. Borrowing a raw pointer retains that
source dependency. Trusted pointer construction needs a readable extent or an obligation permitting bounded terminator
search. Safe APIs do not scan untrusted storage without a bound.

## Resources and failures

Concrete foreign owners retain their resource kind, release operation, thread affinity, and borrowing rules. Raw
representation does not establish ownership. Duplication and foreign reference retention are explicit operations that
create a new obligation only on success.

`ForeignError` carries a stable category and optional numeric native code. Host-authored prose is not semantic data.
Declaration-specific wrappers adapt sentinels, status records, out parameters, and other foreign conventions into typed
results. Normal completion follows the owner's explicit finalization and completion contract.

## Dynamic libraries

`DynamicLibrary.open` accepts an explicit path or target-defined system-library identity with a named load policy.
Unsupported policies fail before loading. Lookup takes an exact symbol name and requested ABI-qualified type and returns
a `DynamicSymbol<T>` dependent on the library. A loader proves address existence, not the requested callable or data
contract. Typed lookup therefore remains a trusted boundary.

`close` mutably borrows the library. Success establishes completion. On failure, it retains any module still owned by
the platform so the caller can retry or transfer it. `is_complete` exposes that state through checked postconditions.
`into_handle` consumes the owner to transfer native ownership. Active symbol borrows prevent closing or transferring the
library. Loaded Bray providers also obey [product lifetime rules](../modules-and-packages/library-and-executable-products.md).

Dynamic loading is conditional on `target.platform.dynamic_loading`. Public value and error types can remain usable in
generic signatures even when open and lookup are unavailable. Runtime loading does not add declarations to the compiler
package graph.

## Callback contexts

[Foreign callback rules](extern-declarations-and-ffi.md) govern capture-free callable entry and explicit
`CallbackContext<State>` ownership. A wrapper states whether registration permits only immediate calls, scoped use,
retention until deregistration, or ownership transfer. Retained registration must stop new invocations and wait for
already entered invocations before context destruction. Concurrent and reentrant use require explicit contracts.

## Target-specific modules

`std.os.windows`, `std.os.linux`, and `std.os.darwin` provide exact target-specific declarations, constants, handles, and
raw operations. Target gates select their contributions, and unsupported targets receive no fallback module. Numeric
similarity between native values does not make resource types interchangeable. Portable service wrappers keep these
representations out of their public contracts.

## Related documents

- [Target properties](target-profiles-and-properties.md)
- [Foreign symbols](foreign-data-and-symbols.md)
- [Interoperability design](../../design/foreign-and-platform-interoperability.md)

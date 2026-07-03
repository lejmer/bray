# Conformance catalog

The compiler-known and recognized standard-library conformance catalog is closed for a conforming Bray implementation.

A conforming compiler and standard library must preserve the declaration identity, availability, contract, and observable semantics of each catalog entry.

## Always-available compiler-known type entries

The always-available compiler-known type entries are:

- `bool`,
- `char`,
- `unit`,
- `never`,
- `i8`,
- `i16`,
- `i32`,
- `i64`,
- `i128`,
- `u8`,
- `u16`,
- `u32`,
- `u64`,
- `u128`,
- `r32`,
- `r64`,
- `c64`,
- `c128`,
- `usize`,
- `isize`,
- `string`,
- `Result<T, E>`,
- `RunResult<T>`,
- `PanicReport`,
- `ConversionError`,
- `Task<T>`,
- `Thread<T>`,
- structural tuple type forms,
- structural fixed-size array type forms,
- slice type forms,
- nullable type forms,
- borrow type forms,
- trait-view type forms,
- callable type forms,
- `RawPointer<T>`.

The always-available compiler-known value entries are:

- `true`,
- `false`,
- `unit`,
- `none`.

## Target-available compiler-known entries

The target-available compiler-known entries are:

- `r16`,
- `r128`,
- `c32`,
- `c256`,
- target-conditional scalar operations,
- target-conditional raw memory declarations under `core.memory`,
- target-conditional atomic declarations and facts,
- target-conditional ABI declarations and facts,
- target-conditional address-space declarations and facts,
- target-conditional allocation declarations and facts.

## Compiler-known traits and contracts

The compiler-known traits and contracts include:

- `Storage<T>`,
- `Iterable`,
- `Iterator`,
- `ConvertTo<Target>`,
- `CheckedConvertTo<Target>`,
- `Copyable`,
- overloadable operator traits defined by the type rules.

## Compiler-known paths

The reserved compiler-known paths include:

- `core.memory`,
- `std.target`.

`std.target` is a compiler-known path even though it uses the `std` prefix.

It is not an ordinary standard-library module.

## Recognized standard-library entries

The recognized standard-library entries include:

- `std.convert<Target>(source)`,
- numeric policy conversion operations under `std`,
- string operations under `std.string`,
- raw-memory helper operations under `std.memory`,
- standard storage policy types and helpers used with compiler-known type forms.

Recognized standard-library entries are usable only through ordinary visibility, import, and path rules.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Availability summary](availability-summary.md)
- Next: [Summary](summary.md)

# Conversion expressions

A **conversion expression** explicitly converts a source expression to a target type.

Plain conversion uses `as`.

```bray
let b: i64 = a as i64;
let y: r64 = x as r64;
let z: c128 = w as c128;
```

The expression before `as` is the source expression.

The type after `as` is the target type.

The target type is syntactically present in a plain conversion expression.

A plain `as` conversion is explicit, total, and value-preserving.

A plain `as` conversion is valid only when every possible source value can be represented by the target type without failure, truncation, wrapping, saturation, rounding loss, domain loss, shape change, layout reinterpretation, allocation change, or hidden construction behavior.

A conversion expression evaluates the source expression and produces a value of the target type when the conversion is valid.

A conversion expression consumes the source value unless the source is copyable or borrowed explicitly.

If the source expression reaches a non-copyable owned value, the conversion moves from that source access path.

After a conversion moves from a source access path, the source access path is moved-from until reinitialized.

If the source expression reaches a copyable value and the conversion context uses copy behavior, the conversion copies according to the source type's copy contract.

If the source expression is a borrow, the conversion operates through the borrow according to the borrow and conversion contracts.

A conversion expression cannot silently drop, complete, or erase a finalization obligation.

A conversion that changes finalization behavior must be an explicitly declared conversion operation with a contract defining that lifecycle behavior.

Plain `as` supports recursive explicit convertibility.

A value of source type `S` can be converted to target type `T` with plain `as` when one of these rules applies:

1. `S` and `T` are the same type.
2. `S` and `T` are built-in scalar numeric types and Bray defines a total value-preserving explicit scalar conversion from `S` to `T`.
3. `S` is a two-element tuple and `T` is a built-in complex type, and both tuple element types are explicitly convertible to `T`'s component real type.
4. `S` and `T` are tuple types with the same arity, and each source element type is explicitly convertible to the corresponding target element type.
5. `S` and `T` are array types with the same length, and the source element type is explicitly convertible to the target element type.
6. `S` and `T` are nullable types, and the source contained type is explicitly convertible to the target contained type.
7. `S` has a participating `ConvertTo<T>` implementation.

Composite conversion preserves structure.

Composite conversion can convert elements recursively.

Composite conversion does not reshape, flatten, transpose, reinterpret layout, allocate a different container shape, or infer user-defined construction.

Tuple-to-tuple conversion requires the same arity.

```bray
let a: (i32, r32) = (1, 2.0);
let b: (i64, r64) = a as (i64, r64);
```

Each source tuple element is converted to the corresponding target tuple element.

A two-element tuple can be converted to a built-in complex type when both tuple elements can be explicitly converted to the complex type's component real type.

```bray
let z: c128 = (real, imag) as c128;
```

The first tuple element becomes the real component.

The second tuple element becomes the imaginary component.

Array-to-array conversion requires the same length.

```bray
let a: [i32; 4] = [1, 2, 3, 4];
let b: [i64; 4] = a as [i64; 4];
```

Each source array element is converted to the target array element type.

Array conversion preserves length and shape.

Nullable-to-nullable conversion converts the present value recursively and preserves the absent state.

```bray
let a: i32? = ...;
let b: i64? = a as i64?;
```

The absence expression is `none`.

Nullable propagation is defined by the nullable propagation expression.

A built-in scalar conversion is valid with plain `as` only when it is total and value-preserving.

Examples of valid plain scalar conversions include widening same-domain conversions such as:

```text
i8  -> i16
i16 -> i32
i32 -> i64
i64 -> i128

u8  -> u16
u16 -> u32
u32 -> u64
u64 -> u128

isize -> i128
usize -> u128

r32 -> r64

c64 -> c128
```

Signed-to-unsigned and unsigned-to-signed integer conversion is valid with plain `as` only when every source value is representable in the target type.

Integer-to-real conversion is valid with plain `as` only when every source value is exactly representable in the target real type.

Real-to-integer conversion is not a plain `as` conversion.

Narrowing real conversion is not a plain `as` conversion.

Complex-to-real conversion is not a plain `as` conversion.

Non-literal real-to-complex conversion is not a plain `as` conversion.

A complex value constructed from non-literal real and imaginary parts uses explicit two-element tuple conversion.

```bray
let real: r64 = 1.0;
let imag: r64 = 2.0;

let z: c128 = (real, imag) as c128;
```

User-defined plain conversions are declared by implementing `ConvertTo<Target>` for the source type.

Fallible conversions are ordinary call expressions, not conversion expression modes.

The standard library fallible conversion operation is `std.convert<Target>(source)`.

`std.convert<Target>(source)` produces `Result<Target, E>`.

[Compiler-known declarations and standard library recognition](../compiler-known-and-standard-library.md) defines visibility and compiler recognition for standard-library operations.

For built-in fallible scalar conversions, `E` is the compiler-known `ConversionError` type.

`ConversionError` reports the built-in conversion failure category as `OutOfRange`, `NonFinite`, or `NonRepresentable`.

For user-defined fallible conversions, `E` is the selected `Error` type from the `CheckedConvertTo<Target>` implementation.

User-defined fallible conversions are declared by implementing `CheckedConvertTo<Target>` for the source type.

```bray
impl PortToU16 = Port(ConvertTo<u16>)
{
    consume func convert() -> u16
    {
        return self.value;
    }
}

impl TextToPort = string(CheckedConvertTo<Port>)
{
    type Error = ParseError;

    consume func convert_checked() -> Result<Port, Error>
    {
        ...
    }
}
```

For a source expression of type `S`, `source as T` selects `S(ConvertTo<T>)` when no built-in recursive conversion rule applies.

For a source expression of type `S`, `std.convert<T>(source)` selects `S(CheckedConvertTo<T>)` when no built-in fallible conversion rule applies.

```bray
let parsed: Result<Port, ParseError> = std.convert<Port>(text);
let port: Port = try std.convert<Port>(text);

let narrowed: Result<i32, ConversionError> = std.convert<i32>(value);
let count: i32 = try std.convert<i32>(value);
```

`try` does not select a conversion.

`try` only unwraps or propagates the `Result` value produced by `std.convert<T>(source)`.

Lossy numeric policies such as rounding, truncating, saturating, and wrapping are ordinary standard-library operations.

They are not conversion expression modes.

```bray
let rounded: r32 = std.round_to<r32>(value, rule = NearestEven);
let truncated: i32 = std.truncate_to<i32>(value);
let saturated: u8 = std.saturate_to<u8>(value);
let wrapped: u8 = std.wrap_to<u8>(value);
```

The target type is always syntactically present.

For `source as T`, the target type is the type after `as`.

For `std.convert<T>(source)`, the target type is the explicit type argument.

The expected type of the surrounding expression does not select the target type, error type, or conversion implementation.

The source type, target type, selected conversion operation, and compiler-known conversion member name can select a conversion implementation.

Result type, expected type, and type-valued member outputs do not select a conversion implementation.

Conversion implementation selection performs no ranking.

If no participating conversion implementation matches, the conversion expression or fallible conversion call is rejected.

If more than one participating conversion implementation remains possible, the conversion expression or fallible conversion call is rejected as ambiguous.

Conversion expressions do not create implicit conversions for calls, assignments, operators, overload selection, construction, or pattern matching.

Fallible conversion calls do not create implicit conversions for calls, assignments, operators, overload selection, construction, or pattern matching.

Conversion expressions participate in type checking, ownership checking, initialization checking, destruction checking, finalization tracking, effect checking, capability checking, trusted obligation checking, and condition refinement.

A conversion expression can require conditions declared by the selected conversion contract.

A conversion expression can establish conditions declared by the selected conversion contract.

Trusted caller obligations used by a conversion expression must be available at that program point, explicitly acknowledged at a trust boundary, or exposed through the surrounding declaration's contract.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Await expressions](await-expressions.md)
- Next: [Pattern-bearing expressions](pattern-bearing-expressions.md)

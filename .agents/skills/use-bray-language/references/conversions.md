# Conversions

**Specifications:** [Conversion expressions](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/conversion-expressions.md), [literal expressions](https://github.com/lejmer/bray/blob/develop/docs/language/expressions/literal-expressions.md), and [conversion traits](https://github.com/lejmer/bray/blob/develop/docs/language/types/traits.md#conversion-traits)

**Core model:** Literal adaptation is contextual. Every conversion is explicit and names its target.

```bray
module conversion_example;

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
        return parse_port(self);
    }
}

struct ConversionSources
{
    pair: (i32, r32);
    values: [i32; 4];
    optional: i32?;
    complex_parts: (r64, r64);
}

func convert_plain(signed: i32, real: r32, complex: c64, port: Port)
{
    let adapted: i64 = 1;
    let widened: i64 = signed as i64;
    let wider_real: r64 = real as r64;
    let wider_complex: c128 = complex as c128;
    let port_number: u16 = port as u16;
}

func convert_structures(sources: ConversionSources)
{
    let converted_pair: (i64, r64) = sources.pair as(i64, r64);
    let converted_values: [i64; 4] = sources.values as[i64; 4];
    let converted_optional: i64? = sources.optional as i64?;
    let constructed_complex: c128 = sources.complex_parts as c128;
}

func convert_with_policy(text: string, measured: r64) -> Result<Port, ParseError>
{
    let narrowed: Result<i32, ConversionError> = std.convert<i32>(measured);
    let rounded: r32 = std.round_to<r32>(measured, rule = NearestEven);
    let truncated: i32 = std.truncate_to<i32>(measured);
    let saturated: u8 = std.saturate_to<u8>(measured);
    let wrapped: u8 = std.wrap_to<u8>(measured);

    return try std.convert<Port>(text);
}
```

Plain conversion recursively preserves tuple arity, array length, and nullable presence. A two-element tuple converts to a complex value as its real and imaginary components. User-defined `ConvertTo<Target>` and `CheckedConvertTo<Target>` implementations follow ordinary implementation selection.

A conversion consumes its source unless copying or explicit borrowing applies. The target is always written in the operation, expected types do not select it, and conversion never introduces implicit coercion for calls, assignment, operators, overloads, construction, or patterns.

**Remember:** Use adaptation for literals, `as Target` only for total value-preserving conversion, `std.convert<Target>` for failure, and a named policy operation for rounding, truncation, saturation, or wrapping.

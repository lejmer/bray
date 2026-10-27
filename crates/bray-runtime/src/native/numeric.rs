use bray_numeric::BinaryFormat;
use bray_runtime_abi::NativeComplexBits;

/// Multiplies complex IEEE components using the language's deterministic numeric policy.
pub extern "C" fn bray_runtime_complex_multiply(
    width: u32,
    left: &NativeComplexBits,
    right: &NativeComplexBits,
    output: &mut NativeComplexBits,
) {
    *output = NativeComplexBits::new(bray_numeric::multiply(
        format(width),
        left.components(),
        right.components(),
    ));
}

/// Divides complex IEEE components using the language's deterministic numeric policy.
pub extern "C" fn bray_runtime_complex_divide(
    width: u32,
    left: &NativeComplexBits,
    right: &NativeComplexBits,
    output: &mut NativeComplexBits,
) {
    *output = NativeComplexBits::new(bray_numeric::divide(
        format(width),
        left.components(),
        right.components(),
    ));
}

/// Computes principal complex powers using the language's deterministic numeric policy.
pub extern "C" fn bray_runtime_complex_power(
    width: u32,
    left: &NativeComplexBits,
    right: &NativeComplexBits,
    output: &mut NativeComplexBits,
) {
    *output = NativeComplexBits::new(bray_numeric::power(
        format(width),
        left.components(),
        right.components(),
    ));
}

fn format(width: u32) -> BinaryFormat {
    match width {
        16 => BinaryFormat::Binary16,
        32 => BinaryFormat::Binary32,
        64 => BinaryFormat::Binary64,
        128 => BinaryFormat::Binary128,
        _ => panic!("checked complex arithmetic has a supported component width: {width}"),
    }
}

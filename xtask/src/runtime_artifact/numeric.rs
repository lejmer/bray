use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use bray_numeric::BinaryFormat;
use bray_runtime_abi::NativeComplexBits;

use super::command::CommandError;

// Differential inputs are fixed arithmetic work, independent of machine speed.
// Include nonfinite recovery, cancellation, subnormal ties and ordinary powers.
pub(super) fn vectors(directory: &Path) -> Result<PathBuf, CommandError> {
    let path = directory.join("numeric-vectors.bin");
    let file = File::create(&path).map_err(|error| CommandError::write(&path, error))?;
    let mut output = BufWriter::new(file);
    let mut state = 0x9e37_79b9_7f4a_7c15;

    for (width, format, fraction, exponent) in [
        (16, BinaryFormat::Binary16, 10_u32, 5_u32),
        (32, BinaryFormat::Binary32, 23, 8),
        (64, BinaryFormat::Binary64, 52, 11),
        (128, BinaryFormat::Binary128, 112, 15),
    ] {
        let sign = 1_u128 << (width - 1);
        let infinity = ((1_u128 << exponent) - 1) << fraction;
        let one = ((1_u128 << (exponent - 1)) - 1) << fraction;

        let cases = [
            0,
            sign,
            1,
            sign | 1,
            one,
            one | sign,
            one + (1 << fraction),
            infinity,
            infinity | sign,
            infinity | (1 << (fraction - 1)),
            infinity - 1,
            sign | (infinity - 1),
            one | 1,
            one - (1 << fraction),
            1 << fraction,
        ];

        let midpoint = ((one >> fraction) + 1 - u128::from(fraction)) << fraction;
        let large_power_of_two = infinity - (1 << fraction);

        for (operation, left, right) in [
            (1, [infinity - 1; 2], [infinity - 1; 2]),
            (1, [one; 2], [one; 2]),
            (1, [one | 1, 0], [one, 0]),
            (2, [infinity - 1; 2], [infinity - 1; 2]),
            (2, [1; 2], [1; 2]),
            (2, [one, 1], [1, 0]),
            (2, [midpoint, 1], [large_power_of_two; 2]),
            (2, [midpoint, sign | 1], [large_power_of_two; 2]),
            (3, [1, 0], [infinity - 1, 0]),
            (3, [one | 1, 0], [(infinity - 1) | sign, 0]),
            (4, [1, 0], [infinity - 1, 0]),
            (4, [sign, one | sign], [0, one]),
        ] {
            write_case(&mut output, width, format, operation, left, right)
                .map_err(|error| CommandError::write(&path, error))?;
        }

        if width == 16 {
            for bits in 0..=u16::MAX {
                for operation in [1, 3] {
                    write_case(
                        &mut output,
                        width,
                        format,
                        operation,
                        [u128::from(bits), 0],
                        [one, 0],
                    )
                    .map_err(|error| CommandError::write(&path, error))?;
                }
            }
        }

        for operation in [1, 2] {
            for index in 0..3000 {
                let mut values = [0; 4];

                for value in &mut values {
                    let random =
                        u128::from(advance(&mut state)) | (u128::from(advance(&mut state)) << 64);

                    *value = if index < 1000 {
                        cases[(advance(&mut state) as usize) % cases.len()]
                    } else {
                        random & (sign | (sign - 1))
                    };
                }

                write_case(
                    &mut output,
                    width,
                    format,
                    operation,
                    [values[0], values[1]],
                    [values[2], values[3]],
                )
                .map_err(|error| CommandError::write(&path, error))?;
            }
        }

        for index in 0..1000 {
            let base = if index < 300 {
                cases[(advance(&mut state) as usize) % cases.len()]
            } else {
                let bits =
                    u128::from(advance(&mut state)) | (u128::from(advance(&mut state)) << 64);

                (one + (u128::from(advance(&mut state) % 3) << fraction))
                    | (bits & ((1 << fraction) - 1))
            };

            let power = if index < 300 {
                cases[(advance(&mut state) as usize) % cases.len()]
            } else {
                let bits =
                    u128::from(advance(&mut state)) | (u128::from(advance(&mut state)) << 64);

                let magnitude = (one - (1 << fraction)) | (bits & ((1 << fraction) - 1));

                magnitude
                    | if advance(&mut state) & 1 != 0 {
                        sign
                    } else {
                        0
                    }
            };

            write_case(&mut output, width, format, 3, [base, 0], [power, 0])
                .map_err(|error| CommandError::write(&path, error))?;
        }

        for index in 0..500 {
            let mut values = [0; 4];

            for (component, value) in values.iter_mut().enumerate() {
                let bits =
                    u128::from(advance(&mut state)) | (u128::from(advance(&mut state)) << 64);

                let negative = if advance(&mut state) & 1 != 0 {
                    sign
                } else {
                    0
                };

                *value = if index < 100 {
                    if component < 2 {
                        cases[(advance(&mut state) as usize) % cases.len()]
                    } else {
                        let exponents = [
                            0,
                            one,
                            one - (1 << fraction),
                            one + (1 << fraction),
                            sign | one,
                            sign | (one + (1 << fraction)),
                        ];

                        exponents[(advance(&mut state) as usize) % exponents.len()]
                    }
                } else {
                    ((one - (1 << fraction)) | (bits & ((1 << fraction) - 1))) | negative
                };
            }

            write_case(
                &mut output,
                width,
                format,
                4,
                [values[0], values[1]],
                [values[2], values[3]],
            )
            .map_err(|error| CommandError::write(&path, error))?;
        }
    }

    let one = 16383_u128 << 112;

    for growth in [100, 500, 1000, 2200, 8000, 16000] {
        write_case(
            &mut output,
            128,
            BinaryFormat::Binary128,
            4,
            [one + (1 << 112), 0],
            [one - (1 << 112), (16383_u128 + growth) << 112],
        )
        .map_err(|error| CommandError::write(&path, error))?;
    }

    output
        .flush()
        .map_err(|error| CommandError::write(&path, error))?;

    Ok(path)
}

fn advance(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;

    *state
}

fn write_case(
    output: &mut impl Write,
    width: u32,
    format: BinaryFormat,
    operation: u64,
    left: [u128; 2],
    right: [u128; 2],
) -> io::Result<()> {
    let expected = match operation {
        1 => bray_numeric::multiply(format, left, right),
        2 => bray_numeric::divide(format, left, right),
        3 => [bray_numeric::real_power(format, left[0], right[0]), 0],
        4 => bray_numeric::complex_power(format, left, right),
        _ => unreachable!("numeric fixture selects a known operation"),
    };

    output.write_all(&operation.to_ne_bytes())?;
    output.write_all(&u64::from(width).to_ne_bytes())?;

    for value in [left, right, expected] {
        let bits = NativeComplexBits::new(value);

        for word in bits.real.words.into_iter().chain(bits.imaginary.words) {
            output.write_all(&word.to_ne_bytes())?;
        }
    }

    Ok(())
}

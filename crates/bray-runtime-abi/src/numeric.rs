/// Bit-preserving complex operand shared by generated code and numeric runtime helpers.
/// Each component contains its low word followed by its high word, independent of byte order.
/// Unused upper bits are zero for IEEE component widths smaller than 128 bits.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeComplexBits {
    /// Real component words.
    pub real: [u64; 2],
    /// Imaginary component words.
    pub imaginary: [u64; 2],
}

impl NativeComplexBits {
    /// Encodes the selected IEEE component bits into the native word record.
    pub fn new(components: [u128; 2]) -> Self {
        let words = |bits: u128| {
            [
                u64::try_from(bits & u128::from(u64::MAX)).expect("low component word fits u64"),
                u64::try_from(bits >> 64).expect("high component word fits u64"),
            ]
        };

        Self {
            real: words(components[0]),
            imaginary: words(components[1]),
        }
    }

    /// Decodes component words without converting through host floating-point values.
    pub fn components(self) -> [u128; 2] {
        let bits = |words: [u64; 2]| u128::from(words[0]) | (u128::from(words[1]) << 64);

        [bits(self.real), bits(self.imaginary)]
    }
}

#[cfg(test)]
mod tests {
    use super::NativeComplexBits;

    #[test]
    fn numeric_helper_record_has_the_generated_word_layout() {
        assert_abi_layout!(NativeComplexBits, size: 32, align: 8, fields: {
            real: 0,
            imaginary: 16,
        });

        let components = [1_u128 << 127, u128::MAX];

        assert_eq!(NativeComplexBits::new(components).components(), components);
    }
}

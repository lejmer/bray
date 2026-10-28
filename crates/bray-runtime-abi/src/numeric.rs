/// Bit-preserving real operand shared by generated code and numeric runtime helpers.
/// The low word precedes the high word, independent of byte order. Unused upper bits are zero.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeRealBits {
    /// Selected IEEE bits as low and high words.
    pub words: [u64; 2],
}

impl NativeRealBits {
    /// Encodes selected IEEE bits into the native word record.
    pub fn new(bits: u128) -> Self {
        Self {
            words: [
                u64::try_from(bits & u128::from(u64::MAX)).expect("low component word fits u64"),
                u64::try_from(bits >> 64).expect("high component word fits u64"),
            ],
        }
    }

    /// Decodes the word record without a host floating-point conversion.
    pub fn bits(self) -> u128 {
        u128::from(self.words[0]) | (u128::from(self.words[1]) << 64)
    }
}

/// Bit-preserving complex operand containing real and imaginary native word records.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeComplexBits {
    /// Real component words.
    pub real: NativeRealBits,
    /// Imaginary component words.
    pub imaginary: NativeRealBits,
}

impl NativeComplexBits {
    /// Encodes the selected IEEE component bits into the native word record.
    pub fn new(components: [u128; 2]) -> Self {
        Self {
            real: NativeRealBits::new(components[0]),
            imaginary: NativeRealBits::new(components[1]),
        }
    }

    /// Decodes component words without converting through host floating-point values.
    pub fn components(self) -> [u128; 2] {
        [self.real.bits(), self.imaginary.bits()]
    }
}

#[cfg(test)]
mod tests {
    use super::{NativeComplexBits, NativeRealBits};

    #[test]
    fn numeric_helper_record_has_the_generated_word_layout() {
        assert_abi_layout!(NativeRealBits, size: 16, align: 8, fields: {
            words: 0,
        });

        assert_abi_layout!(NativeComplexBits, size: 32, align: 8, fields: {
            real: 0,
            imaginary: 16,
        });

        let components = [1_u128 << 127, u128::MAX];

        assert_eq!(NativeComplexBits::new(components).components(), components);

        for bits in [0, 1_u128 << 127, u128::MAX] {
            assert_eq!(NativeRealBits::new(bits).bits(), bits);
        }
    }
}

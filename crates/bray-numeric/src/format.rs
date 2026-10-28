use astro_float_num::{BigFloat, INF_NEG, INF_POS, NAN, Sign, WORD_BIT_SIZE};

/// IEEE binary format used for each real component of a complex value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryFormat {
    /// IEEE binary16.
    Binary16,
    /// IEEE binary32.
    Binary32,
    /// IEEE binary64.
    Binary64,
    /// IEEE binary128.
    Binary128,
}

impl BinaryFormat {
    pub(crate) const fn fraction_bits(self) -> usize {
        match self {
            Self::Binary16 => 10,
            Self::Binary32 => 23,
            Self::Binary64 => 52,
            Self::Binary128 => 112,
        }
    }

    const fn exponent_bits(self) -> usize {
        match self {
            Self::Binary16 => 5,
            Self::Binary32 => 8,
            Self::Binary64 => 11,
            Self::Binary128 => 15,
        }
    }

    pub(crate) const fn sign_mask(self) -> u128 {
        1 << (self.fraction_bits() + self.exponent_bits())
    }

    pub(crate) const fn infinity(self) -> u128 {
        ((1 << self.exponent_bits()) - 1) << self.fraction_bits()
    }

    pub(crate) const fn nan(self) -> u128 {
        self.infinity() | (1 << (self.fraction_bits() - 1))
    }

    pub(crate) fn is_nan(self, bits: u128) -> bool {
        bits & !self.sign_mask() > self.infinity()
    }

    pub(crate) fn is_infinite(self, bits: u128) -> bool {
        bits & !self.sign_mask() == self.infinity()
    }

    pub(crate) fn is_zero(self, bits: u128) -> bool {
        bits & !self.sign_mask() == 0
    }

    pub(crate) fn decode(self, bits: u128, precision: usize) -> BigFloat {
        let mask = self.sign_mask() | (self.sign_mask() - 1);

        assert_eq!(
            bits & !mask,
            0,
            "component bits fit the selected IEEE format"
        );

        let sign = if bits & self.sign_mask() == 0 {
            Sign::Pos
        } else {
            Sign::Neg
        };

        let magnitude = bits & !self.sign_mask();

        if magnitude >= self.infinity() {
            return if magnitude > self.infinity() {
                NAN
            } else if sign == Sign::Pos {
                INF_POS
            } else {
                INF_NEG
            };
        }

        let fraction_bits = self.fraction_bits();
        let field = magnitude >> fraction_bits;
        let bias = (1_i32 << (self.exponent_bits() - 1)) - 1;
        let fraction = magnitude & ((1 << fraction_bits) - 1);

        let significand = if field == 0 {
            fraction
        } else {
            fraction | (1 << fraction_bits)
        };

        let mut result = BigFloat::from_u128(significand, precision);

        if significand != 0 {
            let exponent = i32::try_from(field.max(1)).expect("IEEE exponent fits i32")
                - bias
                - i32::try_from(fraction_bits).expect("IEEE precision fits i32");

            let original = result
                .exponent()
                .expect("finite significand has an exponent");

            result.set_exponent(original + exponent);
        }

        result.set_sign(sign);

        result
    }

    pub(crate) fn encode(self, value: &BigFloat) -> u128 {
        assert!(
            value.err().is_none(),
            "complex arithmetic failed: {:?}",
            value.err()
        );

        if value.is_nan() {
            return self.nan();
        }

        let sign = if value.sign() == Some(Sign::Neg) {
            self.sign_mask()
        } else {
            0
        };

        if value.is_inf() {
            return sign | self.infinity();
        }

        if value.is_zero() {
            return sign;
        }

        let (words, _, _, exponent, _) = value.as_raw_parts().expect("finite value has a mantissa");

        let fraction_bits = self.fraction_bits();
        let bias = (1_i32 << (self.exponent_bits() - 1)) - 1;
        let field = exponent - 1 + bias;

        let retained =
            i32::try_from(fraction_bits + 1).expect("IEEE precision fits i32") + field.min(1) - 1;

        if retained < 0 {
            return sign;
        }

        if field >= (1_i32 << self.exponent_bits()) - 1 {
            return sign | self.infinity();
        }

        let total_bits = words.len() * WORD_BIT_SIZE;

        let retained =
            usize::try_from(retained).expect("nonnegative retained precision fits usize");

        let shift = total_bits - retained;
        let bit = |index: usize| (words[index / WORD_BIT_SIZE] >> (index % WORD_BIT_SIZE)) & 1 != 0;
        let mut significand = 0_u128;

        for index in (shift..total_bits).rev() {
            significand = (significand << 1) | u128::from(bit(index));
        }

        if shift != 0 && bit(shift - 1) && (significand & 1 != 0 || (0..shift - 1).any(bit)) {
            significand += 1;
        }

        if field <= 0 {
            return sign | significand;
        }

        let field = u128::try_from(field).expect("normal exponent field is positive");

        sign | ((field - 1) << fraction_bits) + significand
    }
}

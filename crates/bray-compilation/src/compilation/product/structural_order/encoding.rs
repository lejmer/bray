use bray_symbols::{IntegerConstant, IntegerSign, RealConstantBits};

/// A self-delimiting structural key. Only raw atoms need byte escaping.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct OrderKey(Vec<u8>);

impl OrderKey {
    pub(super) fn into_bytes(self) -> std::sync::Arc<[u8]> {
        self.0.into()
    }
}

impl From<Vec<u8>> for OrderKey {
    fn from(value: Vec<u8>) -> Self {
        let mut bytes = vec![1];

        for byte in value {
            if byte == 0 {
                bytes.extend_from_slice(&[0, 1]);
            } else {
                bytes.push(byte);
            }
        }

        bytes.extend_from_slice(&[0, 0]);

        Self(bytes)
    }
}

pub(super) fn sequence(parts: impl IntoIterator<Item = OrderKey>) -> OrderKey {
    let mut bytes = sequence_prefix(parts);

    bytes.push(0);

    OrderKey(bytes)
}

pub(super) fn sequence_prefix(parts: impl IntoIterator<Item = OrderKey>) -> Vec<u8> {
    let mut bytes = vec![2];

    for part in parts {
        bytes.extend(part.0);
    }

    bytes
}

pub(super) fn term(name: &str, children: impl IntoIterator<Item = OrderKey>) -> OrderKey {
    let children = children.into_iter().collect::<Vec<_>>();

    sequence([
        name.as_bytes().to_vec().into(),
        u64::try_from(children.len())
            .expect("structural term arity must fit u64")
            .to_be_bytes()
            .to_vec()
            .into(),
        sequence(children),
    ])
}

pub(super) fn integer(value: &IntegerConstant) -> OrderKey {
    let negative = value.sign() == IntegerSign::Negative;

    let mut bytes = Vec::with_capacity(9 + value.magnitude().len());

    bytes.push(u8::from(!negative));

    bytes.extend_from_slice(
        &u64::try_from(value.magnitude().len())
            .expect("integer magnitude length must fit u64")
            .to_be_bytes(),
    );

    bytes.extend_from_slice(value.magnitude());

    if negative {
        bytes[1..].iter_mut().for_each(|byte| *byte = !*byte);
    }

    bytes.into()
}

pub(super) fn real(value: RealConstantBits) -> OrderKey {
    let (name, mut bits) = match value {
        RealConstantBits::Binary16(bits) => ("r16", bits.to_be_bytes().to_vec()),
        RealConstantBits::Binary32(bits) => ("r32", bits.to_be_bytes().to_vec()),
        RealConstantBits::Binary64(bits) => ("r64", bits.to_be_bytes().to_vec()),
        RealConstantBits::Binary128(bits) => ("r128", bits.to_vec()),
    };

    // The sign transform implements IEEE totalOrder, including signed zeros,
    // infinities, signaling/quiet NaNs and the reversed negative NaN payload order.
    if bits[0] & 0x80 != 0 {
        bits.iter_mut().for_each(|byte| *byte = !*byte);
    } else {
        bits[0] ^= 0x80;
    }

    term(name, [bits.into()])
}

#[cfg(test)]
mod tests {
    use super::{OrderKey, integer, real, sequence, term};
    use bray_symbols::{IntegerConstant, IntegerSign, RealConstantBits};

    #[test]
    fn sequences_compare_elements_before_length_and_escape_zeroes() {
        let values = [
            vec![],
            vec![vec![]],
            vec![vec![0]],
            vec![vec![0], vec![]],
            vec![vec![0, 0]],
            vec![vec![1]],
            vec![vec![255].into()],
        ];

        for pair in values.windows(2) {
            assert!(pair[0] < pair[1]);

            assert!(
                sequence(pair[0].clone().into_iter().map(OrderKey::from))
                    < sequence(pair[1].clone().into_iter().map(OrderKey::from))
            );
        }

        assert!(
            sequence([b"a".to_vec().into(), b"z".to_vec().into()])
                < sequence([b"b".to_vec().into()])
        );
    }

    #[test]
    fn constructors_compare_name_then_arity_then_children() {
        assert!(term("array", [vec![255].into()]) < term("tuple", []));
        assert!(term("tuple", [vec![255].into()]) < term("tuple", [vec![].into(), vec![].into()]));
        assert!(term("tuple", [vec![0].into()]) < term("tuple", [vec![1].into()]));
    }

    #[test]
    fn nesting_grows_linearly() {
        let mut key = OrderKey::from(vec![0; 32]);
        let initial = key.0.len();

        for _ in 0..100 {
            key = sequence([key]);
        }

        assert_eq!(key.0.len(), initial + 200);
    }

    #[test]
    fn integers_compare_mathematically_across_signs_and_widths() {
        let values = [
            IntegerConstant::new(IntegerSign::Negative, [1, 0]),
            IntegerConstant::new(IntegerSign::Negative, [255]),
            IntegerConstant::new(IntegerSign::Negative, [1]),
            IntegerConstant::from_u64(0),
            IntegerConstant::from_u64(1),
            IntegerConstant::from_u64(255),
            IntegerConstant::from_u64(256),
        ];

        assert!(
            values
                .windows(2)
                .all(|pair| integer(&pair[0]) < integer(&pair[1]))
        );
    }

    #[test]
    fn real_order_matches_ieee_for_nan_payloads_and_signed_zero() {
        let bits = [
            0xffff_ffff_u32,
            0xffc0_0001,
            0xffc0_0000,
            0xff80_0001,
            0xff80_0000,
            0xbf80_0000,
            0x8000_0000,
            0,
            0x3f80_0000,
            0x7f80_0000,
            0x7f80_0001,
            0x7fc0_0000,
            0x7fc0_0001,
            0x7fff_ffff,
        ];

        for pair in bits.windows(2) {
            assert!(
                f32::from_bits(pair[0])
                    .total_cmp(&f32::from_bits(pair[1]))
                    .is_lt()
            );

            assert!(
                real(RealConstantBits::Binary32(pair[0]))
                    < real(RealConstantBits::Binary32(pair[1]))
            );
        }

        for bytes in [2, 4, 8, 16] {
            let negative_zero = match bytes {
                2 => RealConstantBits::Binary16(0x8000),
                4 => RealConstantBits::Binary32(0x8000_0000),
                8 => RealConstantBits::Binary64(0x8000_0000_0000_0000),
                _ => RealConstantBits::Binary128((1_u128 << 127).to_be_bytes()),
            };

            let zero = match bytes {
                2 => RealConstantBits::Binary16(0),
                4 => RealConstantBits::Binary32(0),
                8 => RealConstantBits::Binary64(0),
                _ => RealConstantBits::Binary128([0; 16]),
            };

            assert!(real(negative_zero) < real(zero));
        }
    }
}

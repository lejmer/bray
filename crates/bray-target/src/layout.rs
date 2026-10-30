use std::num::NonZeroU64;

/// The physical layout contract selected for one value representation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetLayoutContract {
    /// Compiler-selected default layout.
    Default,
    /// Bray stable layout for the selected target profile.
    Stable,
    /// C-compatible layout for the selected target ABI.
    C,
    /// Layout identical to one represented storage field.
    Transparent,
}

/// A selected physical value layout.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetValueLayout {
    size: u64,
    alignment: NonZeroU64,
    contract: TargetLayoutContract,
}

impl TargetValueLayout {
    /// Creates a selected physical layout.
    pub const fn new(size: u64, alignment: NonZeroU64, contract: TargetLayoutContract) -> Self {
        Self {
            size,
            alignment,
            contract,
        }
    }

    /// Returns the represented size in bytes.
    pub const fn size(self) -> u64 {
        self.size
    }

    /// Returns the required alignment in bytes.
    pub const fn alignment(self) -> NonZeroU64 {
        self.alignment
    }

    /// Returns the selected physical layout contract.
    pub const fn contract(self) -> TargetLayoutContract {
        self.contract
    }
}

/// Returns the largest target-sized count whose prefix and repeated stride fit.
/// A zero stride accepts every target-sized count when the prefix itself fits.
pub fn allocation_count_limit(prefix: u64, stride: u64, maximum: u64) -> Option<u64> {
    let available = maximum.checked_sub(prefix)?;

    Some(if stride == 0 {
        maximum
    } else {
        available / stride
    })
}

#[cfg(test)]
mod tests {
    use super::allocation_count_limit;

    #[test]
    fn allocation_count_bounds_agree_with_wide_exact_arithmetic() {
        for maximum in [u64::from(u32::MAX), u64::MAX] {
            for prefix in [0, 8, maximum, u64::MAX] {
                for stride in [0, 1, 8, u64::MAX] {
                    let limit = allocation_count_limit(prefix, stride, maximum);

                    for count in [
                        0,
                        1,
                        maximum,
                        limit.unwrap_or(0),
                        limit.unwrap_or(0).saturating_add(1),
                    ] {
                        let exact = u128::from(prefix) + u128::from(stride) * u128::from(count);

                        assert_eq!(
                            limit.is_some_and(|limit| count <= limit),
                            exact <= u128::from(maximum) && count <= maximum,
                            "prefix={prefix}, stride={stride}, count={count}, maximum={maximum}"
                        );
                    }
                }
            }
        }
    }
}

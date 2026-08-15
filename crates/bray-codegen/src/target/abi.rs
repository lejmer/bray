use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_symbols::CallableAbi;

/// Canonical target calling convention selected for one language-level ABI.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetCallingConvention(NonEmptySharedStr);

impl TargetCallingConvention {
    /// Creates a calling convention unless its canonical name is empty.
    pub fn try_new(value: impl Into<Arc<str>>) -> Option<Self> {
        NonEmptySharedStr::try_new(value).map(Self)
    }

    /// Returns the canonical target calling-convention name.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Exact target convention selected for one Bray callable ABI.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableAbiMapping {
    abi: CallableAbi,
    convention: TargetCallingConvention,
}

impl CallableAbiMapping {
    /// Creates one callable ABI mapping.
    pub const fn new(abi: CallableAbi, convention: TargetCallingConvention) -> Self {
        Self { abi, convention }
    }

    /// Returns the language-level callable ABI.
    pub const fn abi(&self) -> CallableAbi {
        self.abi
    }

    /// Returns the exact target calling convention.
    pub const fn convention(&self) -> &TargetCallingConvention {
        &self.convention
    }
}

/// Complete callable ABI mapping for one target.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetAbi(Arc<[CallableAbiMapping]>);

impl TargetAbi {
    /// Creates a mapping that covers every language-level callable ABI exactly once.
    pub fn try_new(
        mappings: impl IntoIterator<Item = CallableAbiMapping>,
    ) -> Result<Self, TargetAbiBuildError> {
        let mut mappings: Vec<_> = mappings.into_iter().collect();

        mappings.sort_unstable_by_key(CallableAbiMapping::abi);

        if mappings
            .windows(2)
            .any(|pair| pair[0].abi() == pair[1].abi())
        {
            return Err(TargetAbiBuildError::DuplicateAbi);
        }

        for abi in [CallableAbi::Bray, CallableAbi::C, CallableAbi::System] {
            if mappings
                .binary_search_by_key(&abi, CallableAbiMapping::abi)
                .is_err()
            {
                return Err(TargetAbiBuildError::MissingAbi(abi));
            }
        }

        Ok(Self(mappings.into()))
    }

    /// Returns ABI mappings in canonical callable-ABI order.
    pub fn mappings(&self) -> &[CallableAbiMapping] {
        &self.0
    }

    /// Returns the exact convention selected for one callable ABI.
    pub fn convention(&self, abi: CallableAbi) -> Option<&TargetCallingConvention> {
        self.0
            .binary_search_by_key(&abi, CallableAbiMapping::abi)
            .ok()
            .map(|index| self.0[index].convention())
    }
}

/// A contract violation that prevents creation of a target ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetAbiBuildError {
    /// Two mappings describe the same language-level callable ABI.
    DuplicateAbi,
    /// One language-level callable ABI has no target convention.
    MissingAbi(CallableAbi),
}

#[cfg(test)]
mod tests {
    use bray_symbols::CallableAbi;

    use super::{CallableAbiMapping, TargetAbi, TargetCallingConvention};

    #[test]
    fn target_abis_are_complete_and_canonical() {
        let Some(bray) = TargetCallingConvention::try_new("bray-x86_64") else {
            panic!("test Bray calling convention must be valid");
        };

        let Some(c) = TargetCallingConvention::try_new("sysv64") else {
            panic!("test C calling convention must be valid");
        };

        let Some(system) = TargetCallingConvention::try_new("sysv64") else {
            panic!("test system calling convention must be valid");
        };

        let Ok(abi) = TargetAbi::try_new([
            CallableAbiMapping::new(CallableAbi::System, system),
            CallableAbiMapping::new(CallableAbi::Bray, bray),
            CallableAbiMapping::new(CallableAbi::C, c),
        ]) else {
            panic!("complete test ABI mapping must be valid");
        };

        assert_eq!(
            abi.mappings()
                .iter()
                .map(CallableAbiMapping::abi)
                .collect::<Vec<_>>(),
            [CallableAbi::Bray, CallableAbi::C, CallableAbi::System]
        );
    }
}

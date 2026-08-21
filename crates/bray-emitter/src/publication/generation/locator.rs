use bray_base::lowercase_hex;

use crate::ProductGenerationIdentity;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct GenerationLocator([u8; 8]);

impl GenerationLocator {
    pub(super) fn for_identity(identity: ProductGenerationIdentity, attempt: u32) -> Self {
        let digest = if attempt == 0 {
            identity.as_bytes()
        } else {
            let mut hasher =
                blake3::Hasher::new_derive_key("bray managed product generation locator");

            hasher.update(&identity.as_bytes());
            hasher.update(&attempt.to_le_bytes());

            *hasher.finalize().as_bytes()
        };

        let mut locator = [0; 8];

        locator.copy_from_slice(&digest[..8]);

        Self(locator)
    }

    pub(super) fn try_from_hex(value: &str) -> Option<Self> {
        bray_base::decode_lowercase_hex::<8>(value).map(Self)
    }

    pub(super) fn to_hex(self) -> String {
        lowercase_hex(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::GenerationLocator;
    use crate::ProductGenerationIdentity;

    #[test]
    fn generation_locators_are_compact_and_support_collision_probing() {
        let identity = ProductGenerationIdentity::new([0x5a; 32]);
        let primary = GenerationLocator::for_identity(identity, 0).to_hex();
        let alternate = GenerationLocator::for_identity(identity, 1).to_hex();

        assert_eq!(primary, "5a5a5a5a5a5a5a5a");
        assert_ne!(alternate, primary);
        assert!(GenerationLocator::try_from_hex(&alternate).is_some());
    }
}

use std::collections::BTreeMap;
use std::hash::Hash;

use bray_base::StableDigestHasher;
use bray_source::SourceId;
use bray_symbols::ImportedInterfaceId;

use super::CompilationFactKey;

const COMPILER_SEMANTIC_REVISION: u32 = 1;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum CompilationInputKey {
    PackageIdentity,
    PackageSourceAuthority,
    SourceSet,
    Source(SourceId),
    SourceDiagnostics,
    ProductKind,
    SelectedTarget,
    NativeLinkInputs,
    SemanticRecursionLimit,
    SemanticPairwiseLimit,
    DependencySet,
    DependencyInterface(ImportedInterfaceId),
    DependencyImplementation(ImportedInterfaceId),
    PlatformServices,
    PackageInterfaceExport,
    CodegenConfiguration,
    StandardLibrary,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct FactFingerprint([u8; 32]);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CompilationInputs(BTreeMap<CompilationInputKey, FactFingerprint>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FactDependencyRecord {
    fingerprint: FactFingerprint,
    facts: BTreeMap<CompilationFactKey, FactFingerprint>,
    inputs: BTreeMap<CompilationInputKey, FactFingerprint>,
}

impl CompilationInputs {
    pub(crate) fn insert<T>(&mut self, key: CompilationInputKey, value: &T)
    where
        T: Hash + ?Sized,
    {
        self.0.insert(key.clone(), fingerprint(&key, value));
    }

    pub(crate) fn get(&self, key: &CompilationInputKey) -> Option<FactFingerprint> {
        self.0.get(key).copied()
    }

    pub(crate) fn has_same_identity_namespace(&self, other: &Self) -> bool {
        self.0
            .iter()
            .filter(|(key, _)| key.affects_identity_namespace())
            .eq(other
                .0
                .iter()
                .filter(|(key, _)| key.affects_identity_namespace()))
    }
}

impl CompilationInputKey {
    const fn affects_identity_namespace(&self) -> bool {
        matches!(
            self,
            Self::PackageIdentity
                | Self::SourceSet
                | Self::Source(_)
                | Self::DependencySet
                | Self::DependencyInterface(_)
                | Self::DependencyImplementation(_)
                | Self::StandardLibrary
        )
    }
}

impl FactDependencyRecord {
    pub(crate) fn new(
        fingerprint: FactFingerprint,
        facts: BTreeMap<CompilationFactKey, FactFingerprint>,
        inputs: BTreeMap<CompilationInputKey, FactFingerprint>,
    ) -> Self {
        Self {
            fingerprint,
            facts,
            inputs,
        }
    }

    pub(crate) const fn fingerprint(&self) -> FactFingerprint {
        self.fingerprint
    }

    pub(crate) fn facts(&self) -> &BTreeMap<CompilationFactKey, FactFingerprint> {
        &self.facts
    }

    pub(crate) fn inputs(&self) -> &BTreeMap<CompilationInputKey, FactFingerprint> {
        &self.inputs
    }
}

pub(crate) fn fact_fingerprint<T>(key: &CompilationFactKey, value: &T) -> FactFingerprint
where
    T: Hash + ?Sized,
{
    let mut hasher = StableDigestHasher::new();

    COMPILER_SEMANTIC_REVISION.hash(&mut hasher);
    key.hash(&mut hasher);
    value.hash(&mut hasher);

    FactFingerprint(hasher.finalize())
}

pub(crate) fn fingerprint<T>(key: &CompilationInputKey, value: &T) -> FactFingerprint
where
    T: Hash + ?Sized,
{
    let mut hasher = StableDigestHasher::new();

    COMPILER_SEMANTIC_REVISION.hash(&mut hasher);
    key.hash(&mut hasher);
    value.hash(&mut hasher);

    FactFingerprint(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::fact_fingerprint;
    use crate::fact::CompilationFactKey;

    #[test]
    fn fact_fingerprints_include_result_content() {
        let key = CompilationFactKey::SyntaxTree;

        assert_ne!(
            fact_fingerprint(&key, &11_u8),
            fact_fingerprint(&key, &12_u8)
        );

        assert_eq!(
            fact_fingerprint(&key, &11_u8),
            fact_fingerprint(&key, &11_u8)
        );
    }
}

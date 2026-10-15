use std::collections::BTreeMap;
use std::hash::Hash;
use std::sync::Arc;

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
    RuntimeRoles,
    PackageInterfaceExport,
    CodegenConfiguration,
    StandardLibrary,
    NativeImplementations,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct FactFingerprint([u8; 32]);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CompilationInputs(BTreeMap<CompilationInputKey, FactFingerprint>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FactDependencyRecord {
    fingerprint: FactFingerprint,
    facts: Arc<[(CompilationFactKey, FactFingerprint)]>,
    inputs: Arc<[(CompilationInputKey, FactFingerprint)]>,
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
    pub(super) const FIXED: [Self; 16] = [
        Self::PackageIdentity,
        Self::PackageSourceAuthority,
        Self::SourceSet,
        Self::SourceDiagnostics,
        Self::ProductKind,
        Self::SelectedTarget,
        Self::NativeLinkInputs,
        Self::SemanticRecursionLimit,
        Self::SemanticPairwiseLimit,
        Self::DependencySet,
        Self::PlatformServices,
        Self::RuntimeRoles,
        Self::PackageInterfaceExport,
        Self::CodegenConfiguration,
        Self::StandardLibrary,
        Self::NativeImplementations,
    ];

    pub(super) const fn fixed_bit(&self) -> Option<u32> {
        let index = match self {
            Self::PackageIdentity => 0,
            Self::PackageSourceAuthority => 1,
            Self::SourceSet => 2,
            Self::SourceDiagnostics => 3,
            Self::ProductKind => 4,
            Self::SelectedTarget => 5,
            Self::NativeLinkInputs => 6,
            Self::SemanticRecursionLimit => 7,
            Self::SemanticPairwiseLimit => 8,
            Self::DependencySet => 9,
            Self::PlatformServices => 10,
            Self::RuntimeRoles => 11,
            Self::PackageInterfaceExport => 12,
            Self::CodegenConfiguration => 13,
            Self::StandardLibrary => 14,
            Self::NativeImplementations => 15,
            Self::Source(_) | Self::DependencyInterface(_) | Self::DependencyImplementation(_) => {
                return None;
            }
        };

        Some(1 << index)
    }

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
            // Published dependencies are immutable and shared across snapshots.
            facts: facts.into_iter().collect(),
            inputs: inputs.into_iter().collect(),
        }
    }

    pub(crate) const fn fingerprint(&self) -> FactFingerprint {
        self.fingerprint
    }

    pub(crate) fn facts(&self) -> &[(CompilationFactKey, FactFingerprint)] {
        &self.facts
    }

    pub(crate) fn inputs(&self) -> &[(CompilationInputKey, FactFingerprint)] {
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
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use super::{CompilationInputKey, CompilationInputs, FactDependencyRecord, fact_fingerprint};
    use crate::fact::CompilationFactKey;

    #[test]
    fn published_dependencies_keep_order_and_share_snapshot_storage() {
        let fingerprint = fact_fingerprint(&CompilationFactKey::SyntaxTree, &());

        let facts = BTreeMap::from([
            (CompilationFactKey::SyntaxTree, fingerprint),
            (CompilationFactKey::DeclaredUnits, fingerprint),
        ]);

        let inputs = BTreeMap::from([
            (CompilationInputKey::SelectedTarget, fingerprint),
            (CompilationInputKey::PackageIdentity, fingerprint),
        ]);

        let expected_facts = facts
            .iter()
            .map(|(key, value)| (key.clone(), *value))
            .collect::<Vec<_>>();

        let expected_inputs = inputs
            .iter()
            .map(|(key, value)| (key.clone(), *value))
            .collect::<Vec<_>>();

        let record = FactDependencyRecord::new(fingerprint, facts, inputs);
        let snapshot = record.clone();

        assert_eq!(record.facts(), expected_facts);
        assert_eq!(record.inputs(), expected_inputs);
        assert_eq!(snapshot, record);
        assert!(Arc::ptr_eq(&record.facts, &snapshot.facts));
        assert!(Arc::ptr_eq(&record.inputs, &snapshot.inputs));
    }

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

    #[test]
    fn provider_artifacts_do_not_change_the_semantic_identity_namespace() {
        let mut first = CompilationInputs::default();

        first.insert(CompilationInputKey::NativeImplementations, &"providers-a");

        let mut second = CompilationInputs::default();

        second.insert(CompilationInputKey::NativeImplementations, &"providers-b");

        assert!(first.has_same_identity_namespace(&second));

        first.insert(CompilationInputKey::StandardLibrary, &"semantics-a");
        second.insert(CompilationInputKey::StandardLibrary, &"semantics-b");

        assert!(!first.has_same_identity_namespace(&second));
    }

    #[test]
    fn fixed_input_bits_follow_the_declared_input_order() {
        for (index, input) in CompilationInputKey::FIXED.iter().enumerate() {
            assert_eq!(input.fixed_bit(), Some(1 << index));
        }

        assert_eq!(
            CompilationInputKey::Source(bray_source::SourceId::new(0)).fixed_bit(),
            None
        );
    }
}
